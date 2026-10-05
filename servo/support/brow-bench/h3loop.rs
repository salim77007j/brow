/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HTTP/3 loopback bench via brow-net-core's H3Client (the production code
//! path brow ships in Phase 2), against a real quinn + h3 server.

use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use h3_quinn::quinn;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};

use crate::Sample;

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

async fn spawn_h3_server(payload: Vec<u8>) -> std::net::SocketAddr {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let cert_der = cert.cert.der().clone();
    let key = rustls_pki_types::PrivateKeyDer::try_from(cert.signing_key.serialize_der()).unwrap();

    let mut server_crypto = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .with_no_client_auth()
    .with_single_cert(vec![cert_der], key)
    .unwrap();
    server_crypto.alpn_protocols = vec![b"h3".to_vec()];

    let server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto).unwrap(),
    ));
    let mut endpoint =
        quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = endpoint.local_addr().unwrap();

    // quinn endpoints must be polled for incoming connections.
    tokio::spawn(async move {
        while let Some(incoming) = endpoint.accept().await {
            let payload = payload.clone();
            tokio::spawn(async move {
                let Ok(connection) = incoming.await else { return };
                let Ok(h3_conn) = h3::server::builder()
                    .build(h3_quinn::Connection::new(connection))
                    .await
                else {
                    return;
                };
                let mut h3 = h3_conn;
                while let Ok(Some(resolver)) = h3.accept().await {
                    let payload = payload.clone();
                    tokio::spawn(async move {
                        let Ok((request, mut stream)) = resolver.resolve_request().await else {
                            return;
                        };
                        let _ = request.into_body();
                        let response = http::Response::builder()
                            .status(200)
                            .header("content-type", "application/octet-stream")
                            .body(())
                            .unwrap();
                        if stream.send_response(response).await.is_ok() {
                            let _ = stream.send_data(Bytes::from(payload)).await;
                            let _ = stream.finish().await;
                        }
                    });
                }
            });
        }
    });
    addr
}

pub async fn run_h3(requests: usize, payload: &[u8]) -> Vec<Sample> {
    let addr = spawn_h3_server(payload.to_vec()).await;

    let config =
        brow_net_core::h3::H3Config::from_rustls(bench_client_tls());
    let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    endpoint.set_default_client_config(config.quinn_client_config().clone());
    let client = brow_net_core::h3::H3Client::with_endpoint(
        config,
        brow_net_core::dns::DohResolver::system_only(),
        endpoint,
    );
    client
        .preconnect("localhost", addr.port(), addr)
        .await
        .expect("preconnect");

    let mut samples = Vec::with_capacity(requests);
    for i in 0..requests {
        let uri: http::Uri = format!("https://localhost:{}/bench/{i}", addr.port())
            .parse()
            .unwrap();
        let start = Instant::now();
        let response = client
            .request(
                "localhost",
                addr.port(),
                http::Method::GET,
                uri,
                http::HeaderMap::new(),
            )
            .await
            .expect("h3 request");
        let ttfb = start.elapsed();
        let body = http_body_util::BodyExt::collect(response.into_body())
            .await
            .expect("body")
            .to_bytes();
        let sample = Sample {
            ttfb,
            body: start.elapsed() - ttfb,
        };
        if i == 0 {
            assert_eq!(body.len(), payload.len(), "payload size mismatch");
        }
        samples.push(sample);
    }
    client.shutdown().await;
    samples
}

/// TEST-ONLY rustls config: accepts the loopback self-signed cert.
fn bench_client_tls() -> rustls::ClientConfig {
    rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .unwrap()
    .dangerous()
    .with_custom_certificate_verifier(Arc::new(NoVerify))
    .with_no_client_auth()
}
