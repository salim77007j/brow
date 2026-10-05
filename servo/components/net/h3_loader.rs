/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! brow (phase 2): opportunistic HTTP/3 fetch path.
//!
//! When an origin has advertised HTTP/3 through `Alt-Svc` (captured in
//! [`HttpState::alt_svc_cache`]) and the request is eligible, this module
//! performs the fetch over QUIC through `brow_net_core::h3::H3Client`. Any
//! transport-level failure returns the error to the caller, which falls back
//! to the classic hyper path — the h3 → h2 → h1.1 fallback matrix.
//!
//! Certificate verification is identical on both paths: the engine's rustls
//! config (platform verifier + user overrides, honoring
//! `ignore_certificate_errors`) is handed to `H3Config::from_rustls`, which
//! switches ALPN to `h3`; QUIC then mandates TLS 1.3 (RFC 9001 §4.1.1).

use std::sync::Arc;

use http_body_util::BodyExt;
use hyper::{HeaderMap, Method};
use net_traits::NetworkError;
use servo_config::pref;
use servo_url::ServoUrl;
use tokio::sync::OnceCell;

use crate::connector::{create_dns_resolver, create_tls_config, CertificateErrorOverrideManager};
use crate::decoder::Decoder;

/// The per-[`HttpState`] H3 client cell. The QUIC endpoint (one shared UDP
/// socket, like real browsers) is bound on the first eligible request.
pub type H3ClientCell = OnceCell<Arc<brow_net_core::h3::H3Client>>;

/// Build the H3 client used by one HttpState (public or private).
///
/// CA handling and certificate-error overrides flow from the embedder exactly
/// as for the classic stack, so QUIC's certificate behavior matches TCP+TLS.
pub fn build_h3_client(
    ca_certificates: crate::connector::CACertificates<'static>,
    ignore_certificate_errors: bool,
    override_manager: CertificateErrorOverrideManager,
) -> brow_net_core::h3::H3Client {
    let tls_config = create_tls_config(
        ca_certificates,
        ignore_certificate_errors,
        override_manager,
    );
    let config = brow_net_core::h3::H3Config::from_rustls(tls_config);
    brow_net_core::h3::H3Client::new(config, create_dns_resolver())
        .expect("failed to bind the HTTP/3 UDP endpoint")
}

/// Is this request eligible for the opportunistic H3 path? Returns the
/// (host, port) to dial when it is.
///
/// Eligibility (all required):
/// * the `network.http3.enabled` pref is on,
/// * the URL is `https` (browsers do not speak h3 to plain origins),
/// * the method is GET or HEAD with no request body,
/// * no proxy is configured (proxies terminate QUIC),
/// * the request carries no interactive authentication headers (the fallback
///   path runs the full auth flow),
/// * the origin advertised a live `h3` Alt-Svc entry.
fn h3_eligible(
    url: &ServoUrl,
    method: &Method,
    request_headers: &HeaderMap,
    state: &crate::http_loader::HttpState,
) -> Option<(String, u16)> {
    if !pref!(network_http3_enabled) {
        return None;
    }
    if url.scheme() != "https" {
        return None;
    }
    if !matches!(*method, Method::GET | Method::HEAD) {
        return None;
    }
    if !pref!(network_http_proxy_uri).is_empty() || !pref!(network_https_proxy_uri).is_empty() {
        return None;
    }
    if request_headers.contains_key(hyper::header::AUTHORIZATION) ||
        request_headers.contains_key(hyper::header::PROXY_AUTHORIZATION)
    {
        return None;
    }
    let host = url.host_str().filter(|host| !host.is_empty())?;
    let port = url.port_or_known_default()?;
    let (alt_host, alt_port) = state
        .alt_svc_cache
        .lock()
        .h3_for_origin("https", host, port)?;
    log::debug!("brow: origin {host}:{port} advertises h3 at {alt_host}:{alt_port}");
    Some((host.to_string(), port))
}

/// Attempt the request over HTTP/3.
///
/// * `Ok(Some(response))` — the h3 path produced a response head; the body is
///   streamed over the QUIC connection through the decoder pipeline.
/// * `Ok(None)` — the request is not eligible for h3; the caller must use the
///   classic stack.
/// * `Err(_)` — the h3 attempt failed (connection or protocol error); the
///   caller falls back to the classic stack. The Alt-Svc advertisement stays
///   cached until expiry, matching browser racing behavior.
pub(crate) async fn fetch_via_h3(
    state: &Arc<crate::http_loader::HttpState>,
    ca_certificates: crate::connector::CACertificates<'static>,
    ignore_certificate_errors: bool,
    url: &ServoUrl,
    method: &Method,
    request_headers: &HeaderMap,
    request_has_body: bool,
) -> Result<Option<hyper::Response<Decoder>>, NetworkError> {
    let Some((host, port)) = h3_eligible(url, method, request_headers, state) else {
        return Ok(None);
    };
    // The h3 client in this phase sends body-less requests only; requests
    // carrying a body use the classic stack.
    if request_has_body {
        return Ok(None);
    }

    let client = match state.h3_client.get() {
        Some(client) => Arc::clone(client),
        None => {
            let ca_certificates = ca_certificates.clone();
            let override_manager = state.override_manager.clone();
            Arc::clone(
                state
                    .h3_client
                    .get_or_init(move || async move {
                        Arc::new(build_h3_client(
                            ca_certificates,
                            ignore_certificate_errors,
                            override_manager,
                        ))
                    })
                    .await,
            )
        },
    };

    // Rebuild the header map without hop-by-hop headers, which are
    // meaningless (and forbidden by RFC 9114 §4.2) on a fresh h3 request.
    let mut headers = HeaderMap::new();
    for (name, value) in request_headers.iter() {
        if matches!(
            name.as_str(),
            "connection" | "transfer-encoding" | "keep-alive" | "upgrade" | "host"
        ) {
            continue;
        }
        headers.insert(name, value.clone());
    }

    let uri: hyper::Uri = url
        .as_str()
        .parse()
        .map_err(|error| NetworkError::HttpError(format!("invalid h3 uri: {error}")))?;

    let response = client
        .request(&host, port, method.clone(), uri, headers)
        .await
        .map_err(|error| {
            log::debug!("brow: h3 fetch failed for {url}, falling back: {error}");
            NetworkError::ConnectionFailure
        })?;

    let decoded = Decoder::detect_h3(response.map(|body| body.boxed()), url.is_secure_scheme());
    log::debug!("brow: fetched {url} over HTTP/3");
    Ok(Some(decoded))
}
