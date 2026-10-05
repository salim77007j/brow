/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Errors produced by brow-net-core.

use thiserror::Error;

/// Top-level error type for brow-net-core operations.
#[derive(Debug, Error)]
pub enum BrowNetError {
    /// The DNS-over-HTTPS exchange failed (transport, malformed answer, or timeout).
    #[error("DoH failure: {0}")]
    Doh(String),

    /// All resolvers (DoH and system fallback) failed for a lookup.
    #[error("resolution failed for {host}: last error: {last_error}")]
    Resolution {
        /// The host we failed to resolve.
        host: String,
        /// The last error message from the resolver chain.
        last_error: String,
    },

    /// Establishing the QUIC connection or the HTTP/3 control state failed.
    #[error("QUIC/H3 connection failure: {0}")]
    H3Connect(String),

    /// The HTTP/3 request/response exchange failed.
    #[error("H3 request failure: {0}")]
    H3Request(String),

    /// A rustls operation failed.
    #[error("TLS error: {0}")]
    Tls(#[from] rustls::Error),

    /// A QUIC endpoint/connection operation failed.
    #[error("QUIC transport error: {0}")]
    QuicTransport(#[from] quinn::ConnectionError),

    /// A generic I/O failure.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// An argument was invalid or unsupported.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),
}
