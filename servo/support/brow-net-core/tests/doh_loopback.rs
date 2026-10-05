/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Full-loopback DoH test: a real TLS DNS-over-HTTPS server (RFC 8484 wire
//! format) answered through the brow resolver, including cache behavior and
//! system fallback on transport failure.

use std::net::IpAddr;
use std::sync::Arc;

use hickory_proto::op::{Message, OpCode, ResponseCode};
use hickory_proto::rr::rdata::a::A;
use hickory_proto::rr::{RData, Record, RecordType};
use hickory_proto::serialize::binary::{BinDecodable, BinEncodable};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::service::service_fn;
use hyper::{Request as HyperRequest, Response as HyperResponse};
use rustls_pki_types::pem::PemObject;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

/// Answer every A query with 93.184.216.34 (TTL 300) and every AAAA query
/// with ::1 — a real DNS message exchange over real TLS.
async fn doh_service(req: HyperRequest<hyper::body::Incoming>) -> Result<HyperResponse<Full<Bytes>>, std::convert::Infallible> {
    if req.uri().path() != "/dns-query" {
        return Ok(HyperResponse::builder()
            .status(404)
            .body(Full::new(Bytes::from("not found")))
            .unwrap());
    }
    let body = req.into_body().collect().await.unwrap().to_bytes();
    let query = match Message::from_vec(&body) {
        Ok(m) => m,
        Err(_) => {
            return Ok(HyperResponse::builder()
                .status(400)
                .body(Full::new(Bytes::from("bad dns message")))
                .unwrap())
        },
    };

    let mut response = Message::response(query.metadata.id, OpCode::Query);
    response.metadata.response_code = ResponseCode::NoError;
    for q in &query.queries {
        response.add_query(q.clone());
        match q.query_type() {
            RecordType::A => {
                response.add_answer(Record::from_rdata(
                    q.name().clone(),
                    300,
                    RData::A(A("93.184.216.34".parse().unwrap())),
                ));
            },
            RecordType::AAAA => {
                response.add_answer(Record::from_rdata(
                    q.name().clone(),
                    300,
                    RData::AAAA(hickory_proto::rr::rdata::aaaa::AAAA("::1".parse().unwrap())),
                ));
            },
            _ => {},
        }
    }

    let wire = response.to_vec().unwrap();
    Ok(HyperResponse::builder()
        .status(200)
        .header("content-type", "application/dns-message")
        .body(Full::new(Bytes::from(wire)))
        .unwrap())
}

/// Spawn a TLS DoH server bound to 127.0.0.1:0; returns (port, cert_der).
async fn spawn_doh_server() -> (u16, CertificateDer<'static>) {
    let cert = rcgen::generate_simple_self_signed(vec!["dns.example.test".to_string()])
        .expect("cert generation");
    let cert_der = cert.cert.der().clone();
    let key = PrivateKeyDer::from_pem_slice(cert.signing_key.serialize_pem().as_bytes())
        .expect("private key");

    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let server_crypto = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert_der.clone()], key)
        .unwrap();
    let acceptor = TlsAcceptor::from(Arc::new(server_crypto));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        while let Ok((tcp, _)) = listener.accept().await {
            let acceptor = acceptor.clone();
            tokio::spawn(async move {
                if let Ok(tls) = acceptor.accept(tcp).await {
                    let io = hyper_util::rt::TokioIo::new(tls);
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, service_fn(doh_service))
                        .await;
                }
            });
        }
    });

    (port, cert_der)
}

fn client_config_trusting(root: &CertificateDer<'static>) -> rustls::ClientConfig {
    let mut roots = rustls::RootCertStore::empty();
    roots.add(root.clone()).expect("valid root");
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

#[tokio::test(flavor = "multi_thread")]
async fn doh_resolves_through_real_tls_server() {
    let (port, cert) = spawn_doh_server().await;

    let config = brow_net_core::dns::DohConfig {
        mode: brow_net_core::dns::DnsMode::DohOnly,
        templates: vec![format!("https://dns.example.test:{port}/dns-query")],
        bootstrap: vec!["127.0.0.1".parse().unwrap()],
        timeout: std::time::Duration::from_secs(5),
        tls_config: Some(client_config_trusting(&cert)),
    };
    let resolver = brow_net_core::dns::DohResolver::new(config);

    let resolved = resolver
        .resolve("www.example.test", 443)
        .await
        .expect("DoH resolution");
    assert_eq!(
        resolved.addrs,
        vec!["93.184.216.34".parse::<IpAddr>().unwrap()]
    );
    assert_eq!(resolved.ttl, std::time::Duration::from_secs(300));

    // Second lookup must be served from cache (resolver state, no new query —
    // we assert the cache grew exactly by this one host).
    assert_eq!(resolver.cache_len(), 1);
    let again = resolver.resolve("www.example.test", 443).await.unwrap();
    assert_eq!(again.addrs, resolved.addrs);
}

#[tokio::test(flavor = "multi_thread")]
async fn doh_falls_back_to_system_on_transport_failure() {
    // Port 1 is never listening; DoH fails, system fallback resolves localhost.
    let config = brow_net_core::dns::DohConfig {
        mode: brow_net_core::dns::DnsMode::DohWithSystemFallback,
        templates: vec!["https://dns.example.test:1/dns-query".to_string()],
        bootstrap: vec!["127.0.0.1".parse().unwrap()],
        timeout: std::time::Duration::from_millis(500),
        tls_config: None,
    };
    let resolver = brow_net_core::dns::DohResolver::new(config);
    let resolved = resolver
        .resolve("localhost", 80)
        .await
        .expect("system fallback");
    assert!(!resolved.addrs.is_empty());
}
