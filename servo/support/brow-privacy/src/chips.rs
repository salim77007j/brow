/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! CHIPS — Cookies Having Independent Partitioned State
//! (draft-ietf-httpbis-rfc6265bis, `Partitioned` attribute).
//!
//! This module owns the *policy*:
//! 1. **Receive** — `Partitioned` requires `Secure` (bis §5.6.3): a
//!    `Set-Cookie` with `Partitioned` but without `Secure` is rejected.
//! 2. **Storage** — a partitioned cookie is stored under its partition key,
//!    derived from the top-level site (`scheme://registrable-domain`).
//! 3. **Send** — in third-party contexts, unpartitioned cookies are omitted
//!    when the policy is on; partitioned cookies are only sent when their
//!    stored partition key equals the current top-level site key.
//!
//! Parsing here is independent of `cookie-rs` so the policy engine is
//! testable against raw header text and can be wired wherever the engine
//! processes cookies (the in-tree glue uses `cookie-rs`'s `partitioned()`
//! plus this module's verdicts).

use serde::{Deserialize, Serialize};
use url::Url;

/// Runtime policy switches (wired to prefs in the engine).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChipsConfig {
    /// Reject `Partitioned` cookies that lack `Secure` (spec MUST).
    pub require_secure_for_partitioned: bool,
    /// Omit third-party cookies that are not `Partitioned` (blocking mode).
    pub block_third_party_unpartitioned: bool,
}

impl Default for ChipsConfig {
    fn default() -> Self {
        ChipsConfig {
            require_secure_for_partitioned: true,
            block_third_party_unpartitioned: false,
        }
    }
}

/// Outcome of receiving a `Set-Cookie` in some context.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ReceiveVerdict {
    /// Store it (with the derived partition key when partitioned).
    Accept {
        partitioned: bool,
        partition_key: Option<String>,
    },
    /// `Partitioned` without `Secure` — MUST be ignored (bis §5.6.3).
    RejectPartitionedInsecure,
}

/// Outcome of the send-side check for one stored cookie.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SendVerdict {
    /// Include the cookie in the request.
    Include,
    /// Omit: third-party context and the cookie is not partitioned.
    OmitUnpartitionedThirdParty,
    /// Omit: partition key of the stored cookie does not match the current
    /// top-level site.
    OmitPartitionMismatch,
}

/// Raw attribute facts extracted from a `Set-Cookie` value.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CookieAttrs {
    pub partitioned: bool,
    pub secure: bool,
    pub httponly: bool,
    pub domain: Option<String>,
    pub path: Option<String>,
    pub same_site: Option<String>,
}

/// Parse the attribute segment (everything after the first `;`-separated
/// name=value pair) of a Set-Cookie header, case-insensitively.
pub fn parse_attrs(set_cookie: &str) -> CookieAttrs {
    let mut attrs = CookieAttrs::default();
    let mut first = true;
    for raw_pair in set_cookie.split(';').skip(1) {
        let _ = first;
        first = false;
        let pair = raw_pair.trim();
        if pair.is_empty() {
            continue;
        }
        let (name, value) = match pair.split_once('=') {
            Some((n, v)) => (n.trim(), v.trim()),
            None => (pair, ""),
        };
        // Attribute names are case-insensitive (bis §5.5).
        if name.eq_ignore_ascii_case("partitioned") {
            attrs.partitioned = true;
        } else if name.eq_ignore_ascii_case("secure") {
            attrs.secure = true;
        } else if name.eq_ignore_ascii_case("httponly") {
            attrs.httponly = true;
        } else if name.eq_ignore_ascii_case("domain") {
            attrs.domain = Some(value.to_ascii_lowercase());
        } else if name.eq_ignore_ascii_case("path") {
            attrs.path = Some(value.to_string());
        } else if name.eq_ignore_ascii_case("samesite") {
            attrs.same_site = Some(value.to_ascii_lowercase());
        }
    }
    attrs
}

/// Partition key derivation: `scheme://registrable-domain` of the top-level
/// site (bis §5.6.3 "partition key"). IP hosts are used verbatim.
pub fn partition_key(top_site: &Url) -> String {
    let host = top_site
        .host_str()
        .unwrap_or("")
        .to_ascii_lowercase();
    format!("{}://{}", top_site.scheme(), host)
}

/// Receive-side policy (step 1 + derived partition key).
pub fn receive_policy(
    set_cookie: &str,
    top_site: &Url,
    cfg: ChipsConfig,
) -> ReceiveVerdict {
    let attrs = parse_attrs(set_cookie);
    if attrs.partitioned {
        if cfg.require_secure_for_partitioned && !attrs.secure {
            return ReceiveVerdict::RejectPartitionedInsecure;
        }
        return ReceiveVerdict::Accept {
            partitioned: true,
            partition_key: Some(partition_key(top_site)),
        };
    }
    ReceiveVerdict::Accept { partitioned: false, partition_key: None }
}

/// One stored cookie as the policy engine sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StoredCookie {
    pub domain: String,
    pub partitioned: bool,
    /// `Some(key)` iff `partitioned`.
    pub partition_key: Option<String>,
}

/// Send-side policy: given the request context, may this cookie be attached?
pub fn send_policy(
    cookie: &StoredCookie,
    _request_url: &Url,
    top_site: &Url,
    first_party: bool,
    cfg: ChipsConfig,
) -> SendVerdict {
    if first_party {
        return SendVerdict::Include;
    }
    if cookie.partitioned {
        let current = partition_key(top_site);
        if cookie.partition_key.as_deref() == Some(current.as_str()) {
            SendVerdict::Include
        } else {
            SendVerdict::OmitPartitionMismatch
        }
    } else if cfg.block_third_party_unpartitioned {
        SendVerdict::OmitUnpartitionedThirdParty
    } else {
        SendVerdict::Include
    }
}

/// Third-party determination shared by the pipeline: hosts differ at the
/// registrable-domain level (approximation without full PSL, consistent
/// with `cname::registrable_suffix`).
pub fn is_third_party(request_host: &str, top_site: &Url) -> bool {
    use crate::cname::registrable_suffix;
    let top_host = top_site.host_str().unwrap_or("").to_ascii_lowercase();
    if top_host.is_empty() || request_host.is_empty() {
        return true;
    }
    registrable_suffix(request_host) != registrable_suffix(&top_host)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn parses_partitioned_and_secure() {
        let a = parse_attrs(
            "value; Path=/; Secure; HttpOnly; Partitioned; SameSite=None; Domain=.ads.example",
        );
        assert!(a.partitioned);
        assert!(a.secure);
        assert!(a.httponly);
        assert_eq!(a.domain.as_deref(), Some(".ads.example"));
        assert_eq!(a.same_site.as_deref(), Some("none"));
    }

    #[test]
    fn attribute_names_case_insensitive() {
        let a = parse_attrs("v; secure; PARTITIONED");
        assert!(a.secure && a.partitioned);
    }

    #[test]
    fn partitioned_without_secure_rejected() {
        let v = receive_policy(
            "sid=xyz; Partitioned; SameSite=None",
            &url("https://shop.example"),
            ChipsConfig::default(),
        );
        assert_eq!(v, ReceiveVerdict::RejectPartitionedInsecure);
    }

    #[test]
    fn partitioned_secure_accepted_with_key() {
        let v = receive_policy(
            "__Host-sid=xyz; Secure; Path=/; Partitioned",
            &url("https://shop.example/cart"),
            ChipsConfig::default(),
        );
        assert_eq!(
            v,
            ReceiveVerdict::Accept {
                partitioned: true,
                partition_key: Some("https://shop.example".into())
            }
        );
    }

    #[test]
    fn unpartitioned_accepted() {
        let v = receive_policy(
            "sid=abc; Secure",
            &url("https://shop.example"),
            ChipsConfig::default(),
        );
        assert_eq!(
            v,
            ReceiveVerdict::Accept { partitioned: false, partition_key: None }
        );
    }

    #[test]
    fn send_first_party_always_included() {
        let c = StoredCookie {
            domain: "ads.example".into(),
            partitioned: true,
            partition_key: Some("https://other.example".into()),
        };
        assert_eq!(
            send_policy(&c, &url("https://ads.example/x"), &url("https://ads.example/"), true, ChipsConfig::default()),
            SendVerdict::Include
        );
    }

    #[test]
    fn send_partition_match() {
        let c = StoredCookie {
            domain: "ads.example".into(),
            partitioned: true,
            partition_key: Some("https://shop.example".into()),
        };
        assert_eq!(
            send_policy(&c, &url("https://ads.example/x"), &url("https://shop.example/p"), false, ChipsConfig::default()),
            SendVerdict::Include
        );
        assert_eq!(
            send_policy(&c, &url("https://ads.example/x"), &url("https://news.example/p"), false, ChipsConfig::default()),
            SendVerdict::OmitPartitionMismatch
        );
    }

    #[test]
    fn send_unpartitioned_blocking_mode() {
        let c = StoredCookie { domain: "ads.example".into(), partitioned: false, partition_key: None };
        let cfg = ChipsConfig { block_third_party_unpartitioned: true, ..Default::default() };
        let top = url("https://shop.example/p");
        // third-party unpartitioned -> omitted
        assert_eq!(
            send_policy(&c, &url("https://ads.example/x"), &top, false, cfg),
            SendVerdict::OmitUnpartitionedThirdParty
        );
        // same registrable domain (a.shop.example) is first-party — include
        let first_party = !is_third_party("a.shop.example", &top);
        assert!(first_party);
        assert_eq!(
            send_policy(&c, &url("https://a.shop.example/x"), &top, first_party, cfg),
            SendVerdict::Include
        );
    }

    #[test]
    fn third_party_by_registrable_domain() {
        assert!(!is_third_party("a.shop.example", &url("https://shop.example/")));
        assert!(is_third_party("shop.example.evil", &url("https://shop.example/")));
        assert!(is_third_party("other.example", &url("https://shop.example/")));
    }

    #[test]
    fn partition_key_uses_scheme_and_host() {
        assert_eq!(partition_key(&url("https://Shop.Example/c")), "https://shop.example");
        assert_eq!(partition_key(&url("http://a.example/x")), "http://a.example");
    }
}
