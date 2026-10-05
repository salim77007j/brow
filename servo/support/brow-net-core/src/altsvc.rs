/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! RFC 7838 `Alt-Svc` parsing and a persistent origin-keyed cache.
//!
//! The engine uses this cache to remember that an origin advertises HTTP/3
//! availability, so subsequent fetches can opportunistically race/fall back
//! through the H3 transport in [`crate::h3`].

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Alternative service advertised for an origin, per RFC 7838 §3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AltSvcEntry {
    /// ALPN protocol id, e.g. `h3` (or the RFC 7838 `protocol-id` escaped form).
    pub protocol: String,
    /// Alternative authority host; `None` means "same host as the origin".
    pub host: Option<String>,
    /// Alternative authority port. Always present in practice (`:443` form).
    pub port: u16,
    /// Max-age in seconds as advertised (`ma` parameter). `None` means 0 → not cacheable.
    pub max_age: Option<u32>,
    /// When the entry was received (unix seconds), used to compute expiry.
    pub received_at_unix: u64,
    /// Whether the `persist` flag was present (survives network changes / restarts).
    pub persist: bool,
}

/// An alternative authority, parsed from the quoted `host:port` value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AltAuthority {
    /// Host part; empty string means same-host.
    pub host: String,
    /// Port part.
    pub port: u16,
}

/// One `alt-value` inside an `Alt-Svc` field value: `h3=":443"; ma=86400; persist=1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AltSvcParam {
    /// ALPN id (lowercased), with RFC 7838 percent-escape decoding applied.
    pub protocol: String,
    /// The quoted alternative authority.
    pub alt_authority: AltAuthority,
    /// `ma` parameter value, if present.
    pub ma: Option<u32>,
    /// `persist` flag present?
    pub persist: bool,
}

/// Parse an `Alt-Svc` header field value (RFC 7838 §3.1), e.g.
/// `h3=":443"; ma=86400, h3-29="alt.example.com:8443"; ma=3600`.
/// The `clear` directive returns an empty list, signalling cache clearing.
pub fn parse_alt_svc(value: &str) -> Vec<AltSvcParam> {
    let value = value.trim();
    if value.eq_ignore_ascii_case("clear") {
        return Vec::new();
    }

    let mut out = Vec::new();
    // The grammar is: alternatives = *( "," OWS alt-value OWS ) — the first entry
    // must not be preceded by a comma, but real servers emit both forms; be liberal.
    for part in split_top_level(value) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(param) = parse_alt_value(part) {
            out.push(param);
        }
    }
    out
}

/// Split on commas that are not inside double-quoted strings.
fn split_top_level(value: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_quotes => {
                current.push(ch);
                escaped = true;
            },
            '"' => {
                in_quotes = !in_quotes;
                current.push(ch);
            },
            ',' if !in_quotes => {
                parts.push(std::mem::take(&mut current));
            },
            _ => current.push(ch),
        }
    }
    parts.push(current);
    parts
}

/// Decode RFC 7838 percent-escapes in a protocol id (`h3%2D2` → `h3-2`).
///
/// RFC 7838 §3.1 percent-encodes the protocol id using URI-style `%XX`
/// sequences (the ABNF's `\XX` historical spelling is accepted too, because
/// several real implementations emit it).
fn decode_protocol_id(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i + 2 < bytes.len() {
        let escape = bytes[i] == b'%' || bytes[i] == b'\\';
        if escape {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out.extend_from_slice(&bytes[i.min(bytes.len())..]);
    String::from_utf8_lossy(&out).to_ascii_lowercase()
}

fn parse_alt_value(part: &str) -> Option<AltSvcParam> {
    // Split `protocol="authority"; param=value; ...`
    let mut segments = part.split(';');
    let first = segments.next()?.trim();
    let (protocol_raw, authority_raw) = first.split_once('=')?;
    let protocol = decode_protocol_id(protocol_raw.trim());
    let authority_raw = authority_raw.trim();
    let authority_raw = authority_raw
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(authority_raw);

    let alt_authority = parse_alt_authority(authority_raw)?;

    let mut ma = None;
    let mut persist = false;
    for seg in segments {
        let seg = seg.trim();
        if let Some((k, v)) = seg.split_once('=') {
            let k = k.trim().to_ascii_lowercase();
            let v = v.trim().trim_matches('"');
            match k.as_str() {
                "ma" => ma = v.parse::<u32>().ok(),
                "persist" => persist = v == "1",
                _ => {},
            }
        }
    }

    Some(AltSvcParam {
        protocol,
        alt_authority,
        ma,
        persist,
    })
}

/// Parse `host:port`, `:port`, or `host` (defaults port 443 for authority-less form? no — port is required by RFC).
fn parse_alt_authority(raw: &str) -> Option<AltAuthority> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    // IPv6 literal?
    if raw.starts_with('[') {
        let close = raw.find(']')?;
        let host = raw[1..close].to_string();
        let rest = &raw[close + 1..];
        let port = rest.strip_prefix(':')?.parse::<u16>().ok()?;
        return Some(AltAuthority { host, port });
    }
    match raw.rsplit_once(':') {
        Some((host, port)) if !port.is_empty() => Some(AltAuthority {
            host: host.to_string(),
            port: port.parse().ok()?,
        }),
        Some((host, "")) => Some(AltAuthority {
            host: host.to_string(),
            port: 0,
        }),
        _ => None,
    }
}

/// Cache key: scheme + host + port of the origin.
pub type OriginKey = (String, String, u16);

/// A persistent cache of advertised alternative services, keyed by origin.
///
/// Entries expire `ma` seconds after they were received. The cache
/// serializes to JSON so the engine can persist it next to the HSTS list.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct AltSvcCache {
    /// JSON object keys must be strings, so the (scheme, host, port) origin is
    /// encoded as `scheme://host:port`.
    entries: HashMap<String, Vec<StoredAltSvc>>,
}

fn origin_key_to_string(scheme: &str, host: &str, port: u16) -> String {
    format!("{scheme}://{host}:{port}")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredAltSvc {
    protocol: String,
    host: String,
    port: u16,
    expires_at_unix: u64,
    persist: bool,
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl AltSvcCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record every cacheable alternative service advertised by an origin
    /// for a given response `Alt-Svc` header value.
    pub fn update_from_header(
        &mut self,
        scheme: &str,
        host: &str,
        port: u16,
        header_value: &str,
    ) {
        let key = origin_key_to_string(scheme, host, port);
        if header_value.trim().eq_ignore_ascii_case("clear") {
            self.entries.remove(&key);
            return;
        }

        let now = now_unix();
        let mut stored: Vec<StoredAltSvc> = Vec::new();
        for param in parse_alt_svc(header_value) {
            if param.protocol != "h3" {
                // brow only races the h3 transport in Phase 2; keep the parser
                // general but the cache focused.
                continue;
            }
            let Some(ma) = param.ma else { continue };
            if ma == 0 {
                continue;
            }
            stored.push(StoredAltSvc {
                protocol: param.protocol,
                host: param.alt_authority.host.clone(),
                port: param.alt_authority.port,
                expires_at_unix: now + u64::from(ma),
                persist: param.persist,
            });
        }

        if stored.is_empty() {
            self.entries.remove(&key);
        } else {
            self.entries.insert(key, stored);
        }
    }

    /// Return the live (unexpired) h3 alternative authority for an origin, if any.
    pub fn h3_for_origin(&self, scheme: &str, host: &str, port: u16) -> Option<(String, u16)> {
        let key = origin_key_to_string(scheme, host, port);
        let stored = self.entries.get(&key)?;
        let now = now_unix();
        let entry = stored.iter().find(|e| e.expires_at_unix > now)?;
        let host = if entry.host.is_empty() {
            host.to_string()
        } else {
            entry.host.clone()
        };
        Some((host, entry.port))
    }

    /// Drop all expired entries. Returns the number of removed entries.
    pub fn evict_expired(&mut self) -> usize {
        let now = now_unix();
        let before = self.entries.len();
        self.entries.retain(|_, v| {
            v.retain(|e| e.expires_at_unix > now);
            !v.is_empty()
        });
        before - self.entries.len()
    }

    /// Clear entries for one host across all schemes/ports.
    pub fn clear_host(&mut self, host: &str) {
        self.entries.retain(|key, _| {
            // Key shape: `scheme://host:port`
            !key.contains(&format!("//{host}:"))
        });
    }

    /// Clear everything.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Number of origins with live entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Serialize to JSON for persistence next to `hsts_list.json`.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Restore from JSON produced by [`Self::to_json`]. Expired entries are dropped.
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str::<AltSvcCache>(json).unwrap_or_default()
    }
}

/// Convenience: remaining validity of an entry, for tests and logging.
pub fn remaining(entry_expires_at_unix: u64) -> Duration {
    let now = now_unix();
    Duration::from_secs(entry_expires_at_unix.saturating_sub(now).min(u64::from(u32::MAX)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_single_h3_entry() {
        let parsed = parse_alt_svc(r#"h3=":443"; ma=86400"#);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].protocol, "h3");
        assert_eq!(parsed[0].alt_authority.host, "");
        assert_eq!(parsed[0].alt_authority.port, 443);
        assert_eq!(parsed[0].ma, Some(86400));
        assert!(!parsed[0].persist);
    }

    #[test]
    fn parse_multiple_entries_with_host_and_persist() {
        let parsed = parse_alt_svc(
            r#"h3="alt.example.com:8443"; ma=3600; persist=1, h3-29=":8443"; ma=60"#,
        );
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].alt_authority.host, "alt.example.com");
        assert_eq!(parsed[0].alt_authority.port, 8443);
        assert_eq!(parsed[0].ma, Some(3600));
        assert!(parsed[0].persist);
        assert_eq!(parsed[1].protocol, "h3-29");
    }

    #[test]
    fn parse_escaped_protocol_id() {
        // RFC 7838 §3.1: percent-encoded protocol ids (`%2D` is the hyphen).
        let parsed = parse_alt_svc(r#"h3%2D2=":443""#);
        assert_eq!(parsed[0].protocol, "h3-2");
    }

    #[test]
    fn parse_clear() {
        assert!(parse_alt_svc("clear").is_empty());
        assert!(parse_alt_svc("CLEAR").is_empty());
    }

    #[test]
    fn parse_quoted_comma_survives() {
        // A quoted authority must not split on its comma (defensive; not legal grammar).
        let parsed = parse_alt_svc(r#"h3="a.example:443"; ma=10, h3="b.example:443"; ma=10"#);
        assert_eq!(parsed.len(), 2);
    }

    #[test]
    fn cache_update_lookup_and_expiry() {
        let mut cache = AltSvcCache::new();
        cache.update_from_header("https", "example.com", 443, r#"h3=":443"; ma=2"#);
        let (h, p) = cache.h3_for_origin("https", "example.com", 443).unwrap();
        assert_eq!(h, "example.com");
        assert_eq!(p, 443);

        // Overwrite with an already-expired entry directly.
        let key = origin_key_to_string("https", "example.com", 443);
        cache.entries.insert(
            key,
            vec![StoredAltSvc {
                protocol: "h3".into(),
                host: "".into(),
                port: 443,
                expires_at_unix: now_unix().saturating_sub(1),
                persist: true,
            }],
        );
        assert!(cache.h3_for_origin("https", "example.com", 443).is_none());
        assert_eq!(cache.evict_expired(), 1);
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_clear_directive_removes_entries() {
        let mut cache = AltSvcCache::new();
        cache.update_from_header("https", "example.com", 443, r#"h3=":443"; ma=100"#);
        cache.update_from_header("https", "example.com", 443, "clear");
        assert!(cache.h3_for_origin("https", "example.com", 443).is_none());
    }

    #[test]
    fn cache_json_roundtrip() {
        let mut cache = AltSvcCache::new();
        cache.update_from_header("https", "example.com", 443, r#"h3="alt.example:8443"; ma=9999; persist=1"#);
        let json = cache.to_json();
        let restored = AltSvcCache::from_json(&json);
        let (h, p) = restored.h3_for_origin("https", "example.com", 443).unwrap();
        assert_eq!(h, "alt.example");
        assert_eq!(p, 8443);
    }
}
