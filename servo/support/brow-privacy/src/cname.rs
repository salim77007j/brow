/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! CNAME-cloaking detection.
//!
//! Third-party trackers alias first-party-looking hostnames to their real
//! infrastructure via DNS CNAME records (e.g. `metrics.site.com` →
//! `collector.tracker.example`). Because cookies and filter matching often
//! key on the *client-visible* hostname, cloaking evades both.
//!
//! The detector consumes a resolved CNAME chain (produced by the DNS stack —
//! DoH resolver in brow-net-core, or injected statically in tests) and
//! yields a verdict when the chain crosses from the site's own domain into
//! a third-party one. The resulting *effective* host is what the filter
//! engine must evaluate.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use url::Url;

/// Resolver of DNS CNAME chains: returns the ordered chain, first entry =
/// queried name, last entry = canonical name (without trailing dot).
pub trait ChainResolver: Send + Sync {
    fn resolve_chain(&self, host: &str) -> Vec<String>;
}

/// A resolver backed by a static table (tests, and the embedder if it
/// resolves through its own DNS layer).
#[derive(Default)]
pub struct StaticChainResolver {
    pub table: HashMap<String, Vec<String>>,
}

impl ChainResolver for StaticChainResolver {
    fn resolve_chain(&self, host: &str) -> Vec<String> {
        self.table.get(host).cloned().unwrap_or_else(|| vec![host.to_string()])
    }
}

/// Why a request's hostname was flagged.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CloakReason {
    /// Canonical name belongs to a different registrable domain than the
    /// client-visible host — the classic cloaking signature.
    CrossDomainAlias { visible: String, canonical: String },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CloakVerdict {
    pub reason: CloakReason,
    /// Chain length (hop count); longer chains are reported for diagnostics.
    pub hops: usize,
}

/// Detects and caches cloaking verdicts per hostname.
pub struct CnameDetector {
    resolver: Box<dyn ChainResolver>,
    cache: Mutex<HashMap<String, (Option<CloakVerdict>, Instant)>>,
    ttl: Duration,
}

impl CnameDetector {
    pub fn new(resolver: Box<dyn ChainResolver>, ttl: Duration) -> Self {
        CnameDetector { resolver, cache: Mutex::new(HashMap::new()), ttl }
    }

    /// Inspect a request URL's host against the top-level site.
    /// Returns `Some(verdict)` when the host is a cloaked alias.
    pub fn inspect(&self, site: &Url, request: &Url) -> Option<CloakVerdict> {
        let host = request.host_str()?.to_ascii_lowercase();
        if host.is_empty() || is_ip_literal(&host) {
            return None;
        }
        if let Some((v, at)) = self.cache.lock().get(&host) {
            if at.elapsed() < self.ttl {
                return v.clone();
            }
        }

        let verdict = self.evaluate(site, &host);
        self.cache.lock().insert(host, (verdict.clone(), Instant::now()));
        verdict
    }

    fn evaluate(&self, site: &Url, host: &str) -> Option<CloakVerdict> {
        let chain = self.resolver.resolve_chain(host);
        if chain.is_empty() {
            return None;
        }
        let canonical = chain
            .last()
            .expect("chain checked non-empty")
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if canonical == host || canonical.is_empty() {
            return None;
        }
        let site_host = site.host_str().unwrap_or("").to_ascii_lowercase();
        // The visible host is first-party-relative; if the canonical name
        // lands on a different site domain AND different registrable domain
        // than the visible host, this is cloaking.
        let visible_reg = registrable_suffix(host);
        let canonical_reg = registrable_suffix(&canonical);
        if canonical_reg == visible_reg {
            return None;
        }
        // Requests to hosts already at the site's own registrable domain are
        // first-party even if their canonical name is remote (CDN case is the
        // inverse and is NOT cloaking).
        let _ = site_host;
        Some(CloakVerdict {
            reason: CloakReason::CrossDomainAlias { visible: host.to_string(), canonical },
            hops: chain.len(),
        })
    }

    pub fn cache_len(&self) -> usize {
        self.cache.lock().len()
    }

    pub fn clear_cache(&self) {
        self.cache.lock().clear();
    }
}

fn is_ip_literal(host: &str) -> bool {
    host.parse::<std::net::IpAddr>().is_ok() || (host.starts_with('[') && host.ends_with(']'))
}

/// Registrable-domain approximation without a full PSL: the last two labels,
/// with a small table of common two-level public suffixes. Matches the
/// approximation used by `chips::partition_key`.
pub fn registrable_suffix(host: &str) -> String {
    let labels: Vec<&str> = host.split('.').filter(|l| !l.is_empty()).collect();
    if labels.len() <= 2 {
        return host.to_string();
    }
    let last2: Vec<&str> = labels[labels.len() - 2..].to_vec();
    let second_level_tld = matches!(
        last2[0],
        "co" | "com" | "org" | "net" | "gov" | "edu" | "ac" | "or" | "ne"
    );
    if second_level_tld {
        let taken: Vec<&str> = labels[labels.len() - 3..].to_vec();
        return taken.join(".");
    }
    last2.join(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detector(table: &[(&str, &[&str])]) -> CnameDetector {
        let mut map = HashMap::new();
        for (host, chain) in table {
            map.insert(
                host.to_string(),
                chain.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            );
        }
        CnameDetector::new(Box::new(StaticChainResolver { table: map }), Duration::from_secs(60))
    }

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn detects_classic_cloaking() {
        let d = detector(&[(
            "metrics.news.example",
            &["metrics.news.example", "collector.trackad.example"],
        )]);
        let v = d
            .inspect(&url("https://news.example/"), &url("https://metrics.news.example/px.js"))
            .expect("cloaked");
        match v.reason {
            CloakReason::CrossDomainAlias { visible, canonical } => {
                assert_eq!(visible, "metrics.news.example");
                assert_eq!(canonical, "collector.trackad.example");
            }
        }
        assert_eq!(v.hops, 2);
    }

    #[test]
    fn cross_registrable_alias_is_flagged() {
        let d = detector(&[(
            "cdn.news.example",
            &["cdn.news.example", "edge.news-cdn.example"],
        )]);
        // different registrable (news.example vs news-cdn.example) — cloaking
        let v = d.inspect(&url("https://news.example/"), &url("https://cdn.news.example/a.js"));
        assert!(v.is_some());
    }

    #[test]
    fn same_registrable_alias_is_not_cloaking() {
        let d = detector(&[(
            "cdn.news.example",
            &["cdn.news.example", "static.news.example"],
        )]);
        let v = d.inspect(&url("https://news.example/"), &url("https://cdn.news.example/a.js"));
        assert!(v.is_none());
    }

    #[test]
    fn plain_hostname_is_clean() {
        let d = detector(&[]);
        let v = d.inspect(&url("https://news.example/"), &url("https://news.example/a.js"));
        assert!(v.is_none());
    }

    #[test]
    fn ip_literal_never_cloaks() {
        let d = detector(&[]);
        let v = d.inspect(&url("https://news.example/"), &url("https://93.184.216.34/x"));
        assert!(v.is_none());
    }

    #[test]
    fn cache_hits() {
        let d = detector(&[("a.example", &["a.example", "b.example"])]);
        let u = url("https://a.example/x");
        let _ = d.inspect(&url("https://site.example/"), &u);
        assert_eq!(d.cache_len(), 1);
        let _ = d.inspect(&url("https://site.example/"), &u);
        assert_eq!(d.cache_len(), 1);
    }

    #[test]
    fn registrable_suffix_table() {
        assert_eq!(registrable_suffix("a.b.co.uk"), "b.co.uk");
        assert_eq!(registrable_suffix("news.example"), "news.example");
        assert_eq!(registrable_suffix("x.y.example"), "y.example");
    }
}
