/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # brow-net-core
//!
//! Modern network protocol core for the brow browser (Phase 2).
//!
//! This crate is deliberately **self-contained**: it depends only on mainstream,
//! actively maintained crates (quinn, h3, hyper, rustls, hickory) and carries no
//! dependency on the Servo engine itself. This lets it be unit- and integration-
//! tested in isolation (including a full loopback HTTP/3 test), while the
//! `servo-net` component provides the thin glue that wires these capabilities
//! into the engine's fetch pipeline.
//!
//! ## Modules
//!
//! * [`altsvc`] — RFC 7838 `Alt-Svc` parsing and a persistent origin cache.
//! * [`dns`] — RFC 8484 DNS-over-HTTPS resolver with bootstrap addresses,
//!   TTL cache, and system-resolver fallback, plus a hyper-compatible
//!   connector resolver ([`dns::BrowDnsResolver`]).
//! * [`h3`] — HTTP/3 (RFC 9114) fetch client over QUIC (quinn + h3),
//!   with an origin-keyed connection pool.
//! * [`sec_headers`] — COOP / COEP / CORP parsing and the policy decision
//!   engine used by the engine to enforce cross-origin isolation rules.
//! * [`tls_policy`] — TLS version policy parsing shared by the engine's
//!   rustls configuration.

pub mod altsvc;
pub mod dns;
pub mod error;
pub mod h3;
pub mod sec_headers;
pub mod tls_policy;

pub use error::BrowNetError;

/// Build a rustls [`rustls::ClientConfig`] used internally by this crate
/// (DoH transport, H3 loopback tests) with the platform-independent
/// webpki root store.
pub(crate) fn internal_client_config() -> rustls::ClientConfig {
    let provider = rustls::crypto::ring::default_provider();
    let root_store =
        rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    rustls::ClientConfig::builder_with_provider(provider.into())
        .with_safe_default_protocol_versions()
        .expect("ring provider supports the default protocol versions")
        .with_root_certificates(root_store)
        .with_no_client_auth()
}
