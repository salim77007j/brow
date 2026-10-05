/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! TLS version policy shared between the engine's rustls configuration and
//! the QUIC/H3 transport.
//!
//! The engine exposes a `network.tls.min-version` pref whose value is one of
//! the strings accepted by [`parse_min_version`]. rustls already refuses
//! TLS 1.0/1.1 and SSL; this policy additionally lets the embedder raise the
//! floor to TLS 1.3 while keeping the ability to opt back down for
//! compatibility testing.

use rustls::ProtocolVersion;

/// Parsed lower bound for the TLS version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TlsMinVersion {
    /// TLS 1.2 — the compatibility floor used by mainstream browsers.
    Tls12,
    /// TLS 1.3 — hardened floor (also mandatory for QUIC).
    Tls13,
}

/// A TLS-1.3-only protocol version list (used for the hardened floor and
/// always for QUIC, which mandates TLS 1.3).
pub static TLS13_ONLY: &[&rustls::SupportedProtocolVersion] = &[&rustls::version::TLS13];

impl TlsMinVersion {
    /// The corresponding rustls [`ProtocolVersion`].
    pub fn protocol_version(self) -> ProtocolVersion {
        match self {
            TlsMinVersion::Tls12 => ProtocolVersion::TLSv1_2,
            TlsMinVersion::Tls13 => ProtocolVersion::TLSv1_3,
        }
    }

    /// The rustls `SupportedProtocolVersion` statics this maps to.
    pub fn supported_versions(self) -> &'static [&'static rustls::SupportedProtocolVersion] {
        match self {
            TlsMinVersion::Tls12 => rustls::ALL_VERSIONS,
            TlsMinVersion::Tls13 => TLS13_ONLY,
        }
    }
}

/// Parse a `network.tls.min-version` pref value.
///
/// Accepted (case-insensitive): `"TLSv1.2"`, `"TLS 1.2"`, `"1.2"` → [`TlsMinVersion::Tls12`];
/// `"TLSv1.3"`, `"TLS 1.3"`, `"1.3"` → [`TlsMinVersion::Tls13`].
/// Empty/unknown values fall back to the supplied default.
pub fn parse_min_version(value: &str, default: TlsMinVersion) -> TlsMinVersion {
    let normalized: String = value
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase()
        .trim_start_matches("tlsv")
        .trim_start_matches("tls")
        .trim_start_matches('v')
        .to_string();
    match normalized.as_str() {
        "1.2" => TlsMinVersion::Tls12,
        "1.3" => TlsMinVersion::Tls13,
        _ => default,
    }
}

/// QUIC mandates TLS 1.3 (RFC 9001 §4.1.1) — the H3 transport always uses this.
pub const QUIC_SUPPORTED_VERSIONS: &[&rustls::SupportedProtocolVersion] = &[&rustls::version::TLS13];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_spellings() {
        for v in ["TLSv1.2", "TLS 1.2", "tls1.2", "1.2", "v1.2"] {
            assert_eq!(parse_min_version(v, TlsMinVersion::Tls13), TlsMinVersion::Tls12, "{v}");
        }
        for v in ["TLSv1.3", "TLS 1.3", "tls1.3", "1.3"] {
            assert_eq!(parse_min_version(v, TlsMinVersion::Tls12), TlsMinVersion::Tls13, "{v}");
        }
    }

    #[test]
    fn unknown_falls_back_to_default() {
        assert_eq!(parse_min_version("", TlsMinVersion::Tls12), TlsMinVersion::Tls12);
        assert_eq!(
            parse_min_version("banana", TlsMinVersion::Tls12),
            TlsMinVersion::Tls12
        );
    }

    #[test]
    fn supported_versions_slice_is_correct() {
        assert_eq!(TlsMinVersion::Tls12.supported_versions().len(), 2);
        assert_eq!(TlsMinVersion::Tls13.supported_versions().len(), 1);
        assert_eq!(
            TlsMinVersion::Tls13.protocol_version(),
            ProtocolVersion::TLSv1_3
        );
    }
}
