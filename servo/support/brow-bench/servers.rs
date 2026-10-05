/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Loopback servers and clients for HTTP/1.1, HTTP/2 (cleartext) and
//! HTTP/2 (TLS 1.3).

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::TcpListener;

use crate::{Proto, Sample};

async fn serve_fixed(req: Request<Incoming>, payload: Vec<u8>) -> Result<Response<Full<Bytes>>, Infallible> {
    let _ = req.into_body().collect().await;
    Ok(Response::new(Full::new(Bytes::from(payload))))
}

/// Spawn a cleartext TCP server speaking HTTP/1.1 or HTTP/2.
async fn spawn_plain(proto: Proto, payload: Vec<u8>) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else { return };
            let payload = payload.clone();
            let io = TokioIo::new(tcp);
            let service = service_fn(move |req: Request<Incoming>| {
                let payload = payload.clone();
                async move { serve_fixed(req, payload).await }
            });
            match proto {
                Proto::H1 => {
                    tokio::spawn(async move {
                        let _ = hyper::server::conn::http1::Builder::new()
                            .serve_connection(io, service)
                            .await;
                    });
                },
                Proto::H2 => {
                    tokio::spawn(async move {
                        let _ = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                            .serve_connection(io, service)
                            .await;
                    });
                },
                _ => unreachable!(),
            }
        }
    });
    addr
}

pub async fn run_plain(proto: Proto, requests: usize, payload: &[u8]) -> Vec<Sample> {
    let addr = spawn_plain(proto, payload.to_vec()).await;

    type PlainClient = hyper_util::client::legacy::Client<
        hyper_util::client::legacy::connect::HttpConnector,
        Full<Bytes>,
    >;
    let client: PlainClient = if proto == Proto::H2 {
        hyper_util::client::legacy::Client::builder(TokioExecutor::new())
            .http2_only(true)
            .build_http()
    } else {
        hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build_http()
    };

    let mut samples = Vec::with_capacity(requests);
    for i in 0..requests {
        let uri = format!("http://{addr}/bench/{i}").parse::<http::Uri>().unwrap();
        let request = http::Request::builder()
            .method(http::Method::GET)
            .uri(uri)
            .body(Full::new(Bytes::new()))
            .unwrap();

        let start = Instant::now();
        let response = client.request(request).await.expect("request");
        let ttfb = start.elapsed();
        let body = response.into_body().collect().await.expect("body").to_bytes();
        let sample = Sample {
            ttfb,
            body: start.elapsed() - ttfb,
        };
        if i == 0 {
            assert_eq!(body.len(), payload.len(), "payload size mismatch");
        }
        samples.push(sample);
    }
    samples
}

/// TEST-ONLY accept-everything verifier for the loopback TLS bench.
#[derive(Debug)]
struct NoVerify;

impl rustls::client::danger::ServerCertVerifier for NoVerify {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &rustls_pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls_pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
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
        dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

pub async fn run_h2_tls(requests: usize, payload: &[u8]) -> Vec<Sample> {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let cert_der = cert.cert.der().clone();
    let key = PrivateKeyDer::from_pem_slice(cert.signing_key.serialize_pem().as_bytes()).unwrap();

    let mut server_crypto = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![cert_der], key)
    .unwrap();
    server_crypto.alpn_protocols = vec![b"h2".to_vec()];
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_crypto));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let payload_server = payload.to_vec();
    tokio::spawn(async move {
        loop {
            let Ok((tcp, _)) = listener.accept().await else { return };
            let acceptor = acceptor.clone();
            let payload = payload_server.clone();
            tokio::spawn(async move {
                let Ok(tls) = acceptor.accept(tcp).await else { return };
                let io = TokioIo::new(tls);
                let service = service_fn(move |req: Request<Incoming>| {
                    let payload = payload.clone();
                    async move { serve_fixed(req, payload).await }
                });
                let _ = hyper::server::conn::http2::Builder::new(TokioExecutor::new())
                    .serve_connection(io, service)
                    .await;
            });
        }
    });

    // Client: hyper-rustls with the test verifier, ALPN h2.
    let mut client_tls = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(NoVerify))
    .with_no_client_auth();
    // Note: ALPN is set by the HttpsConnectorBuilder (enable_http2), which
    // rejects pre-defined ALPN lists in the TLS config.

    // Like the engine's ServoHttpConnector, the inner plain connector must
    // accept the https destination (it only dials host:port; TLS wraps it).
    let mut plain = hyper_util::client::legacy::connect::HttpConnector::new();
    plain.enforce_http(false);
    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_tls_config(client_tls)
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .wrap_connector(plain);
    let client: hyper_util::client::legacy::Client<_, Full<Bytes>> =
        hyper_util::client::legacy::Client::builder(TokioExecutor::new()).build(https);

    // Map the numeric loopback address onto the SNI name via the URI host is
    // impossible with rustls; hyper-rustls dials the URI host and verifies
    // against it. We therefore dial "localhost" via /etc/hosts (always
    // resolvable) — the server listens on 127.0.0.1, so rewrite the port.
    let port = addr.port();
    let mut samples = Vec::with_capacity(requests);
    for i in 0..requests {
        let uri = format!("https://localhost:{port}/bench/{i}")
            .parse::<http::Uri>()
            .unwrap();
        let request = http::Request::builder()
            .method(http::Method::GET)
            .uri(uri)
            .body(Full::new(Bytes::new()))
            .unwrap();

        let start = Instant::now();
        let response = client.request(request).await.expect("request");
        let ttfb = start.elapsed();
        let body = response.into_body().collect().await.expect("body").to_bytes();
        let sample = Sample {
            ttfb,
            body: start.elapsed() - ttfb,
        };
        if i == 0 {
            assert_eq!(body.len(), payload.len(), "payload size mismatch");
        }
        samples.push(sample);
    }
    samples
}
