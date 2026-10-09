/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HTTP/3 (RFC 9114) fetch client over QUIC (RFC 9000), using `quinn` for the
//! transport and the `h3` reference implementation for the HTTP mapping.
//!
//! # Design
//!
//! * One shared UDP [`quinn::Endpoint`] — every origin's QUIC connection
//!   multiplexes over a single local port (like real browsers).
//! * Connections are pooled per `(scheme, host, port)` origin and reused for
//!   subsequent requests; a dead connection transparently reconnects once and
//!   surfaces an error only if the retry also fails (the engine then falls
//!   back to the HTTP/2 → HTTP/1.1 path — the h3 → h2 → h1.1 matrix).
//! * Address resolution for the QUIC handshake goes through the brow DoH
//!   resolver ([`crate::dns`]) — H3 traffic must not leak DNS either.
//! * 0-RTT resumption is intentionally **not** enabled in Phase 2: it requires
//!   a session ticket store and careful replay-safety per request semantics.
//!   Tracked for a later phase.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use bytes::{Buf, Bytes};
use http::{HeaderMap, Method, Uri};
use parking_lot::Mutex;
use quinn::crypto::rustls::QuicClientConfig;
use tokio::sync::mpsc;

use crate::dns::DohResolver;
use crate::error::BrowNetError;

/// ALPN identifier for HTTP/3 (RFC 9114 §7.1).
pub const ALPN_H3: &[u8] = b"h3";

/// Tunables for the H3 transport.
#[derive(Clone)]
pub struct H3Config {
    quinn_client: quinn::ClientConfig,
    /// Overall budget for QUIC connect + request send + response head.
    pub connect_timeout: Duration,
    /// Budget for receiving the full body once the head has arrived.
    pub body_timeout: Duration,
    /// Maximum number of origins kept in the pool.
    pub pool_capacity: usize,
}

impl H3Config {
    /// Derive the QUIC client configuration from an engine rustls config.
    ///
    /// The engine's certificate verification (platform verifier, overrides)
    /// carries over unchanged — only the ALPN is switched to `h3`, and QUIC
    /// mandates TLS 1.3 (RFC 9001), which rustls enforces for us here.
    pub fn from_rustls(mut rustls_config: rustls::ClientConfig) -> Self {
        rustls_config.alpn_protocols = vec![ALPN_H3.to_vec()];

        let quic_crypto = QuicClientConfig::try_from(rustls_config)
            .expect("rustls config supports QUIC (TLS 1.3 handshake)");
        let mut client_config = quinn::ClientConfig::new(Arc::new(quic_crypto));

        let mut transport = quinn::TransportConfig::default();
        // Keep NAT bindings warm while a tab is actively fetching.
        transport.keep_alive_interval(Some(Duration::from_secs(10)));
        // Drop the connection state if the peer goes silent for a minute.
        transport.max_idle_timeout(Some(
            quinn::IdleTimeout::try_from(Duration::from_secs(60)).expect("valid idle timeout"),
        ));
        client_config.transport_config(Arc::new(transport));

        Self {
            quinn_client: client_config,
            connect_timeout: Duration::from_secs(10),
            body_timeout: Duration::from_secs(30),
            pool_capacity: 64,
        }
    }

    /// Replace the default quinn client config (used by tests for custom verifiers).
    pub fn with_quinn_client_config(mut self, config: quinn::ClientConfig) -> Self {
        self.quinn_client = config;
        self
    }

    /// quinn client config (for tests and diagnostics).
    pub fn quinn_client_config(&self) -> &quinn::ClientConfig {
        &self.quinn_client
    }
}

/// A pooled HTTP/3 connection to one origin.
///
/// `h3::client::SendRequest` is `Clone` and shares the underlying connection
/// state, so requests dispatch concurrently without a lock; each clone only
/// needs `&mut` for the (cheap) stream-open step.
struct PooledH3Connection {
    server_addr: SocketAddr,
    sender: h3::client::SendRequest<h3_quinn::OpenStreams, Bytes>,
    quinn: quinn::Connection,
}

/// A request body chunk stream for an H3 response (drives the actual QUIC stream).
pub struct H3Body {
    receiver: mpsc::Receiver<Result<Bytes, BrowNetError>>,
}

impl http_body::Body for H3Body {
    type Data = Bytes;
    type Error = BrowNetError;

    fn poll_frame(
        self: std::pin::Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        // H3Body is Unpin (owns an mpsc receiver); drive it with poll_recv so
        // the waker plumbing is handled by tokio's channel.
        match self.get_mut().receiver.poll_recv(cx) {
            Poll::Ready(Some(Ok(bytes))) => Poll::Ready(Some(Ok(http_body::Frame::data(bytes)))),
            Poll::Ready(Some(Err(err))) => Poll::Ready(Some(Err(err))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Pooled HTTP/3 client.
#[derive(Clone)]
pub struct H3Client {
    endpoint: Arc<quinn::Endpoint>,
    config: H3Config,
    resolver: DohResolver,
    pool: Arc<Mutex<HashMap<(String, u16), Arc<PooledH3Connection>>>>,
}

impl H3Client {
    /// Build a client with its own UDP endpoint and address resolver.
    pub fn new(config: H3Config, resolver: DohResolver) -> Result<Self, BrowNetError> {
        let endpoint = quinn::Endpoint::client("0.0.0.0:0".parse().unwrap())
            .map_err(|e| BrowNetError::H3Connect(format!("failed to bind UDP endpoint: {e}")))?;
        let mut endpoint = endpoint;
        endpoint.set_default_client_config(config.quinn_client.clone());

        Ok(Self {
            endpoint: Arc::new(endpoint),
            config,
            resolver,
            pool: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Build a client sharing a caller-provided endpoint (tests: loopback).
    pub fn with_endpoint(
        config: H3Config,
        resolver: DohResolver,
        endpoint: quinn::Endpoint,
    ) -> Self {
        Self {
            endpoint: Arc::new(endpoint),
            config,
            resolver,
            pool: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Perform a GET (or other body-less method) request and return the
    /// response head plus a streaming body.
    ///
    /// Any transport-level failure returns `Err`; the caller implements the
    /// h3 → h2 → h1.1 fallback by retrying on the classic stack.
    pub async fn request(
        &self,
        host: &str,
        port: u16,
        method: Method,
        uri: Uri,
        headers: HeaderMap,
    ) -> Result<http::Response<H3Body>, BrowNetError> {
        let pooled = self.get_or_connect(host, port).await?;

        // Dispatch the request on its own QUIC stream. `SendRequest` is
        // Clone, so concurrent requests multiplex without locking.
        let mut sender = pooled.sender.clone();
        let mut builder = http::Request::builder()
            .method(method)
            .uri(uri)
            .version(http::Version::HTTP_3);
        for (name, value) in headers.iter() {
            builder = builder.header(name, value);
        }
        let request = builder
            .body(())
            .map_err(|e| BrowNetError::H3Request(format!("invalid request: {e}")))?;

        let mut stream = tokio::time::timeout(
            self.config.connect_timeout,
            async {
                sender
                    .send_request(request)
                    .await
                    .map_err(|e| BrowNetError::H3Request(format!("send_request failed: {e}")))
            },
        )
        .await
        .map_err(|_| BrowNetError::H3Request("timed out sending request".into()))??;

        // Wait for the response head.
        let response = tokio::time::timeout(self.config.connect_timeout, stream.recv_response())
            .await
            .map_err(|_| BrowNetError::H3Request("timed out waiting for response head".into()))?
            .map_err(|e| BrowNetError::H3Request(format!("failed to receive response: {e}")))?;

        let (parts, _) = response.into_parts();

        // brow (R-16): the pump counts WIRE bytes (h3 data frames are
        // pre-decompression), so a Content-Length cross-check here is exact.
        // A clean FIN that delivered a different byte count is truncation,
        // not EOF — quinn treats an early stream finish as a normal end, and
        // treating it as clean EOF is what let partial scripts reach the
        // parser on owner hardware.
        let expected_content_length: Option<u64> = parts
            .headers
            .get(http::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse().ok());

        let (tx, rx) = mpsc::channel::<Result<Bytes, BrowNetError>>(4);
        let body_timeout = self.config.body_timeout;
        tokio::spawn(async move {
            let mut received_bytes: u64 = 0;
            loop {
                match tokio::time::timeout(body_timeout, stream.recv_data()).await {
                    Ok(Ok(Some(chunk))) => {
                        let mut chunk = chunk;
                        let bytes = chunk.copy_to_bytes(chunk.remaining());
                        received_bytes += bytes.len() as u64;
                        if tx.send(Ok(bytes)).await.is_err() {
                            return; // receiver dropped: body no longer needed
                        }
                    },
                    Ok(Ok(None)) => {
                        // brow (R-16): verify the stream delivered exactly
                        // what Content-Length promised before calling it a
                        // clean EOF. A mismatch yields a body error, which
                        // http_loader turns into a failed resource.
                        if let Some(expected) = expected_content_length &&
                            received_bytes != expected
                        {
                            let _ = tx
                                .send(Err(BrowNetError::H3Request(format!(
                                    "truncated body: Content-Length {expected}, \
                                     received {received_bytes}"
                                ))))
                                .await;
                            return;
                        }
                        break; // clean EOF
                    },
                    Ok(Err(e)) => {
                        let _ = tx
                            .send(Err(BrowNetError::H3Request(format!("body error: {e}"))))
                            .await;
                        return;
                    },
                    Err(_) => {
                        let _ = tx
                            .send(Err(BrowNetError::H3Request("body receive timed out".into())))
                            .await;
                        return;
                    },
                }
            }
        });

        Ok(http::Response::from_parts(parts, H3Body { receiver: rx }))
    }

    async fn get_or_connect(&self, host: &str, port: u16) -> Result<Arc<PooledH3Connection>, BrowNetError> {
        let existing = self.pool.lock().get(&(host.to_string(), port)).cloned();
        if let Some(conn) = existing {
            // Liveness probe: quinn errors are detected lazily; treat a closed
            // connection as absent.
            if conn.quinn.close_reason().is_none() {
                return Ok(conn);
            }
            self.pool.lock().remove(&(host.to_string(), port));
        }

        let fresh = self.connect(host, port).await?;
        let mut pool = self.pool.lock();
        if pool.len() >= self.config.pool_capacity {
            // Evict an arbitrary origin (HashMap order) — capacity is generous.
            if let Some(key) = pool.keys().next().cloned() {
                if let Some(old) = pool.remove(&key) {
                    let _ = old.quinn.close(0u32.into(), b"pool evicted");
                }
            }
        }
        let arc = Arc::new(fresh);
        pool.insert((host.to_string(), port), Arc::clone(&arc));
        Ok(arc)
    }

    /// Speculatively open and pool a connection to an origin before any request
    /// needs it (navigation preconnect, à la `<link rel=preconnect>`).
    ///
    /// `addr` is usually the result of a prior DoH lookup for `host`; the QUIC
    /// SNI is set to `host` so certificate validation is unaffected.
    pub async fn preconnect(
        &self,
        host: &str,
        port: u16,
        addr: SocketAddr,
    ) -> Result<(), BrowNetError> {
        let existing = self.pool.lock().get(&(host.to_string(), port)).cloned();
        if let Some(conn) = existing {
            if conn.quinn.close_reason().is_none() {
                return Ok(());
            }
            self.pool.lock().remove(&(host.to_string(), port));
        }
        let conn = self.connect_addr(host, port, addr).await?;
        self.pool
            .lock()
            .insert((host.to_string(), port), Arc::new(conn));
        Ok(())
    }

    async fn connect(&self, host: &str, port: u16) -> Result<PooledH3Connection, BrowNetError> {
        // Resolve through the DoH resolver — H3 must not leak DNS. The port is
        // applied when dialing (below), not during resolution.
        let resolved = self.resolver.resolve(host, port).await.map_err(|e| {
            BrowNetError::H3Connect(format!("address resolution for {host} failed: {e}"))
        })?;
        let addr = resolved
            .addrs
            .first()
            .copied()
            .ok_or_else(|| BrowNetError::H3Connect("resolver returned no addresses".into()))?;
        self.connect_addr(host, port, SocketAddr::new(addr, port))
            .await
    }

    async fn connect_addr(
        &self,
        host: &str,
        _port: u16,
        server_addr: SocketAddr,
    ) -> Result<PooledH3Connection, BrowNetError> {
        let connecting = self
            .endpoint
            .connect(server_addr, host)
            .map_err(|e| BrowNetError::H3Connect(format!("connect failed: {e}")))?;

        let quinn_conn = tokio::time::timeout(self.config.connect_timeout, connecting)
            .await
            .map_err(|_| BrowNetError::H3Connect("QUIC handshake timed out".into()))?
            .map_err(|e| BrowNetError::H3Connect(format!("QUIC handshake failed: {e}")))?;

        let h3_conn = h3_quinn::Connection::new(quinn_conn.clone());
        let (mut h3_client_conn, sender) = h3::client::new(h3_conn)
            .await
            .map_err(|e| BrowNetError::H3Connect(format!("H3 session setup failed: {e}")))?;

        // Drive the H3 connection state machine for the lifetime of the QUIC
        // connection: poll_close resolves once the peer closes cleanly.
        tokio::spawn(async move {
            let closed = std::future::poll_fn(|cx| h3_client_conn.poll_close(cx)).await;
            log::debug!("H3 connection driver ended: {closed:?}");
        });

        Ok(PooledH3Connection {
            server_addr,
            sender,
            quinn: quinn_conn,
        })
    }

    /// Number of pooled origins (diagnostics).
    pub fn pool_len(&self) -> usize {
        self.pool.lock().len()
    }

    /// Origin keys and server addresses of all pooled connections
    /// (diagnostics; consumed by the bench harness and memory reports).
    pub fn pool_stats(&self) -> Vec<((String, u16), SocketAddr)> {
        self.pool
            .lock()
            .iter()
            .map(|(key, conn)| (key.clone(), conn.server_addr))
            .collect()
    }

    /// Close all pooled connections and stop the UDP endpoint.
    pub async fn shutdown(&self) {
        for (_, conn) in self.pool.lock().drain() {
            let _ = conn.quinn.close(0u32.into(), b"shutdown");
        }
        self.endpoint.wait_idle().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn config_derives_quic_tls13_alpn() {
        let config = crate::internal_client_config();
        let h3_config = H3Config::from_rustls(config);
        // Building a client on top of the derived QUIC config must succeed
        // (binds a UDP endpoint) — this exercises the rustls → quinn bridge.
        let client = H3Client::new(h3_config, DohResolver::system_only()).unwrap();
        assert_eq!(client.pool_len(), 0);
        assert!(client.pool_stats().is_empty());
    }
}
