/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Full-loopback HTTP/3 test: a real quinn+h3 server over UDP on localhost,
//! exercised through the brow H3 client (including connection-pool reuse and
//! streamed bodies).

use std::sync::Arc;

use bytes::Bytes;
use h3_quinn::quinn;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};

/// Accept-everything verifier — TEST ONLY. The production engine uses the
/// platform verifier; tests use loopback with self-signed certificates.
#[derive(Debug)]
struct NoVerify;

impl ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

fn test_client_rustls_config() -> rustls::ClientConfig {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(NoVerify))
        .with_no_client_auth()
}

/// Spawn a minimal but real H3 server: routes echo their path in the body and
/// mirror the `x-brow-echo` request header back as `x-brow-echo-reply`.
async fn spawn_h3_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()])
        .expect("self-signed cert generation");
    let cert_der = cert.cert.der().clone();
    let key_der = rustls::pki_types::PrivateKeyDer::try_from(cert.signing_key.serialize_der())
        .expect("valid private key");

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut server_crypto = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_no_client_auth()
        .with_single_cert(vec![cert_der], key_der)
        .expect("server cert");
    // QUIC carries the HTTP/3 ALPN inside the TLS handshake — both sides must
    // offer/accept `h3` (RFC 9114 §7.1).
    server_crypto.alpn_protocols = vec![b"h3".to_vec()];

    let server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)
            .expect("quinn server crypto"),
    ));
    let endpoint = quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap())
        .expect("bind UDP endpoint");
    let addr = endpoint.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            let connection = match incoming.await {
                Ok(c) => c,
                Err(_) => continue,
            };
            tokio::spawn(async move {
                let h3_conn = h3_quinn::Connection::new(connection);
                let mut h3 = match h3::server::builder().build(h3_conn).await {
                    Ok(conn) => conn,
                    Err(_) => return,
                };

                while let Ok(Some(resolver)) = h3.accept().await {
                    tokio::spawn(async move {
                        let Ok((request, mut stream)) = resolver.resolve_request().await else {
                            return;
                        };
                        let path = request.uri().path().to_string();
                        let echo = request
                            .headers()
                            .get("x-brow-echo")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("")
                            .to_string();

                        if path == "/large" {
                            // 256 KiB body to exercise chunked streaming.
                            let body = vec![b'b'; 256 * 1024];
                            let response = http::Response::builder()
                                .status(200)
                                .header("content-length", body.len().to_string())
                                .body(())
                                .unwrap();
                            if stream.send_response(response).await.is_ok() {
                                let _ = stream.send_data(Bytes::from(body)).await;
                                let _ = stream.finish().await;
                            }
                            return;
                        }

                        if path == "/truncated" {
                            // brow (R-16): promise 100 bytes but deliver 40,
                            // then finish the stream cleanly — quinn surfaces
                            // an early finish as a normal end, so the CLIENT
                            // must detect the Content-Length mismatch.
                            let response = http::Response::builder()
                                .status(200)
                                .header("content-length", "100")
                                .body(())
                                .unwrap();
                            if stream.send_response(response).await.is_ok() {
                                let _ =
                                    stream.send_data(Bytes::from_static(&[b'x'; 40])).await;
                                let _ = stream.finish().await;
                            }
                            return;
                        }

                        let mut response = http::Response::builder()
                            .status(200)
                            .header("content-type", "text/plain");
                        if !echo.is_empty() {
                            response = response.header("x-brow-echo-reply", echo);
                        }
                        let response = response.body(()).unwrap();
                        if stream.send_response(response).await.is_ok() {
                            let _ = stream.send_data(Bytes::from(format!("path={path}"))).await;
                            let _ = stream.finish().await;
                        }
                    });
                }
            });
        }
    });

    (addr, handle)
}

async fn h3_client(addr: std::net::SocketAddr) -> brow_net_core::h3::H3Client {
    let config = brow_net_core::h3::H3Config::from_rustls(test_client_rustls_config());
    let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    endpoint.set_default_client_config(config.quinn_client_config().clone());
    let client = brow_net_core::h3::H3Client::with_endpoint(
        config,
        brow_net_core::dns::DohResolver::system_only(),
        endpoint,
    );
    // Prewarm the pooled connection with an explicit loopback address; the SNI
    // stays "localhost" so this mirrors a real preconnect.
    client
        .preconnect("localhost", addr.port(), addr)
        .await
        .expect("preconnect");
    client
}

#[tokio::test(flavor = "multi_thread")]
async fn h3_roundtrip_and_pool_reuse() {
    let (addr, _server) = spawn_h3_server().await;
    let client = h3_client(addr).await;

    // 1. Plain roundtrip.
    let uri: http::Uri = format!("https://localhost:{}/hello", addr.port())
        .parse()
        .unwrap();
    let response = client
        .request("localhost", addr.port(), http::Method::GET, uri, http::HeaderMap::new())
        .await
        .expect("h3 roundtrip");
    assert_eq!(response.status(), 200);
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    assert_eq!(&body[..], b"path=/hello");

    // 2. Pool reuse: second request must hit the same pooled connection.
    assert_eq!(client.pool_len(), 1);
    let uri2: http::Uri = format!("https://localhost:{}/second", addr.port())
        .parse()
        .unwrap();
    let response2 = client
        .request(
            "localhost",
            addr.port(),
            http::Method::GET,
            uri2,
            http::HeaderMap::new(),
        )
        .await
        .expect("second h3 request on pooled connection");
    assert_eq!(response2.status(), 200);
    assert_eq!(client.pool_len(), 1, "pool must not grow on reuse");
}

#[tokio::test(flavor = "multi_thread")]
async fn h3_header_echo_and_streamed_body() {
    let (addr, _server) = spawn_h3_server().await;
    let client = h3_client(addr).await;

    let mut headers = http::HeaderMap::new();
    headers.insert("x-brow-echo", "phase2".parse().unwrap());
    let uri: http::Uri = format!("https://localhost:{}/echo", addr.port())
        .parse()
        .unwrap();
    let response = client
        .request("localhost", addr.port(), http::Method::GET, uri, headers)
        .await
        .expect("h3 request");
    assert_eq!(
        response.headers().get("x-brow-echo-reply").unwrap(),
        "phase2"
    );

    // Large body streamed over multiple QUIC stream frames.
    let uri: http::Uri = format!("https://localhost:{}/large", addr.port())
        .parse()
        .unwrap();
    let response = client
        .request("localhost", addr.port(), http::Method::GET, uri, http::HeaderMap::new())
        .await
        .expect("large h3 request");
    let body = http_body_util::BodyExt::collect(response.into_body())
        .await
        .unwrap()
        .to_bytes();
    assert_eq!(body.len(), 256 * 1024);
    assert!(body.iter().all(|&b| b == b'b'));
}

// brow (R-16): a clean FIN that delivered fewer bytes than Content-Length
// promised is truncation, not EOF — the body must error (which http_loader
// turns into a failed resource) instead of completing a partial body.
#[tokio::test(flavor = "multi_thread")]
async fn h3_truncated_body_is_an_error_not_a_partial_body() {
    let (addr, _server) = spawn_h3_server().await;
    let client = h3_client(addr).await;

    let uri: http::Uri = format!("https://localhost:{}/truncated", addr.port())
        .parse()
        .unwrap();
    let response = client
        .request("localhost", addr.port(), http::Method::GET, uri, http::HeaderMap::new())
        .await
        .expect("truncated h3 request (head arrives fine)");
    assert_eq!(response.status(), 200);

    let collected = http_body_util::BodyExt::collect(response.into_body()).await;
    assert!(
        collected.is_err(),
        "a body short of its Content-Length must not complete cleanly"
    );
}
