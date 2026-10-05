/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! RFC 8484 DoH HTTPS transport.
//!
//! One TLS session per query (DoH exchanges are infrequent thanks to the
//! resolver's TTL cache); the connection is established to a *bootstrap IP*
//! while SNI and certificate validation use the template's real hostname.

use std::net::IpAddr;
use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::client::conn::http1;
use hyper::Request;
use rustls_pki_types::ServerName;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::error::BrowNetError;

/// The HTTPS transport for DoH exchanges.
#[derive(Clone)]
pub struct DohTransport {
    tls: TlsConnector,
}

/// Content-Type mandated by RFC 8484 §6.5.
const DNS_MESSAGE_CONTENT_TYPE: &str = "application/dns-message";

impl DohTransport {
    /// Build the transport. Uses the supplied TLS config if present, otherwise
    /// a webpki-roots-backed config (public CAs — correct for public DoH servers).
    pub fn new(config: &super::DohConfig) -> Result<Self, BrowNetError> {
        let tls_config = match &config.tls_config {
            Some(override_config) => override_config.clone(),
            None => crate::internal_client_config(),
        };
        Ok(Self {
            tls: TlsConnector::from(std::sync::Arc::new(tls_config)),
        })
    }

    /// Perform one DoH exchange:
    /// connect to `bootstrap_ip:port(template)` with SNI of the template host,
    /// POST the wire-format query, and return the response message bytes.
    pub async fn exchange(
        &self,
        template: &str,
        bootstrap_ip: IpAddr,
        query: Vec<u8>,
        timeout: Duration,
    ) -> Result<Vec<u8>, BrowNetError> {
        let (host, port, path) = parse_template(template)?;

        tokio::time::timeout(timeout, self.exchange_inner(host, port, path, bootstrap_ip, query))
            .await
            .map_err(|_| BrowNetError::Doh("DoH exchange timed out".into()))?
    }

    async fn exchange_inner(
        &self,
        host: String,
        port: u16,
        path: String,
        bootstrap_ip: IpAddr,
        query: Vec<u8>,
    ) -> Result<Vec<u8>, BrowNetError> {
        // 1. TCP to the bootstrap address (never resolved via DNS — no loops).
        let tcp = TcpStream::connect((bootstrap_ip, port))
            .await
            .map_err(|e| BrowNetError::Doh(format!("tcp connect to bootstrap failed: {e}")))?;
        tcp.set_nodelay(true).ok();

        // 2. TLS with the real hostname as SNI; certificate checks are against
        //    that name, not the IP we dialed.
        let server_name = ServerName::try_from(host.clone())
            .map_err(|e| BrowNetError::Doh(format!("bad SNI name '{host}': {e}")))?;
        let tls_stream = self
            .tls
            .connect(server_name, tcp)
            .await
            .map_err(|e| BrowNetError::Doh(format!("TLS handshake with DoH server failed: {e}")))?;

        // 3. HTTP/1.1 over the TLS session.
        let io = hyper_util::rt::TokioIo::new(tls_stream);
        let (mut sender, connection) = http1::handshake(io)
            .await
            .map_err(|e| BrowNetError::Doh(format!("http handshake failed: {e}")))?;
        tokio::spawn(async move {
            if let Err(err) = connection.await {
                log::debug!("DoH connection driver ended: {err}");
            }
        });

        let uri = format!("https://{host}:{port}{path}");
        let request = Request::builder()
            .method(http::Method::POST)
            .uri(&uri)
            .header(http::header::CONTENT_TYPE, DNS_MESSAGE_CONTENT_TYPE)
            .header(http::header::ACCEPT, DNS_MESSAGE_CONTENT_TYPE)
            .body(Full::new(Bytes::from(query)))
            .map_err(|e| BrowNetError::Doh(format!("failed to build request: {e}")))?;

        // 4. Send and collect the answer.
        sender.ready().await.map_err(|e| BrowNetError::Doh(e.to_string()))?;
        let response = sender
            .send_request(request)
            .await
            .map_err(|e| BrowNetError::Doh(format!("DoH request failed: {e}")))?;

        if response.status() != http::StatusCode::OK {
            return Err(BrowNetError::Doh(format!(
                "DoH server returned HTTP {}",
                response.status()
            )));
        }
        let content_type = response
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !content_type.starts_with(DNS_MESSAGE_CONTENT_TYPE) {
            return Err(BrowNetError::Doh(format!(
                "unexpected DoH content-type: {content_type}"
            )));
        }

        let body = response
            .into_body()
            .collect()
            .await
            .map_err(|e| BrowNetError::Doh(format!("failed reading DoH response: {e}")))?
            .to_bytes();
        Ok(body.to_vec())
    }
}

/// Parse `https://host[:port]/path` template (RFC 8484 §3.1).
/// Returns (host, port, path).
fn parse_template(template: &str) -> Result<(String, u16, String), BrowNetError> {
    let url: hyper::Uri = template
        .parse()
        .map_err(|e| BrowNetError::InvalidArgument(format!("bad DoH template '{template}': {e}")))?;

    if url.scheme_str() != Some("https") {
        return Err(BrowNetError::InvalidArgument(format!(
            "DoH template must be https: {template}"
        )));
    }
    let host = url
        .host()
        .ok_or_else(|| BrowNetError::InvalidArgument(format!("DoH template lacks host: {template}")))?
        .to_string();
    let port = url.port_u16().unwrap_or(443);
    let path = match (url.path(), url.query()) {
        ("/", Some(q)) => format!("/?{q}"),
        (p, Some(q)) => format!("{p}?{q}"),
        (p, None) => p.to_string(),
    };
    Ok((host, port, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_templates() {
        assert_eq!(
            parse_template("https://mozilla.cloudflare-dns.com/dns-query").unwrap(),
            (
                "mozilla.cloudflare-dns.com".to_string(),
                443,
                "/dns-query".to_string()
            )
        );
        assert_eq!(
            parse_template("https://dns.google/dns-query?ct").unwrap(),
            ("dns.google".to_string(), 443, "/dns-query?ct".to_string())
        );
        assert_eq!(
            parse_template("https://dns.example:8443/").unwrap(),
            ("dns.example".to_string(), 8443, "/".to_string())
        );
    }

    #[test]
    fn rejects_non_https() {
        assert!(parse_template("http://dns.example/dns-query").is_err());
    }
}
