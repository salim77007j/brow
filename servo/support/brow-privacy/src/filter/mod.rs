/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The network + cosmetic filtering engine.
//!
//! Architecture (standard two-stage design, cf. uBlock-style engines):
//! 1. **Prefilter** — two Aho-Corasick automata:
//!    - `host_ac`: needles are host-hints of `||`-anchored rules, matched
//!      against the full URL (label-boundary verification happens in stage 2)
//!    - `generic_ac`: needles are the first literal of every other rule
//! 2. **Verify** — candidate rules are checked exactly (anchors, `^`
//!    separators, `*` wildcards, `$type`, `$domain`, `$third-party`).
//!
//! Exceptions beat blocks unless the block is `$important`.

mod cosmetic;
pub(crate) mod matcher;
pub mod parser;
pub mod rule;

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use aho_corasick::AhoCorasick;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use url::Url;

pub use cosmetic::{CosmeticEngine, CosmeticResult};
pub use rule::{FilterKind, ParseStats};

use matcher::{first_literal, host_hint, lower_url, verify};
use rule::{NetworkRule, PartyScope, PatternPiece};
#[cfg(test)]
use rule::ResourceTypeMask;

/// The decision returned for one subresource request.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Decision {
    /// No rule matched.
    Allow,
    /// An `@@` exception matched (still allowed, but recorded).
    AllowExcepted { rule: String },
    /// Blocked by this rule (raw text).
    Block { rule: String, important: bool },
}

#[derive(Debug)]
struct NetworkEngine {
    rules: Vec<NetworkRule>,
    /// needle -> rule indices (host automaton, `||` rules)
    host_groups: Vec<Vec<u32>>,
    /// needle -> rule indices (generic automaton)
    generic_groups: Vec<Vec<u32>>,
    host_ac: AhoCorasick,
    generic_ac: AhoCorasick,
    /// rules with no literal at all (pure `*`/`^`) — verified directly
    fallback: Vec<u32>,
    /// `/regex/` rules: rule index -> compiled regex (case-sensitive)
    regex_rules: Vec<(u32, regex::Regex)>,
}

/// `/regex/` rule evaluation cap: beyond this many regex rules in a list,
/// further regex rules are parsed and counted but not evaluated (DoS guard).
const REGEX_EVAL_CAP: usize = 512;

/// Statistics about decisions, lock-free counters.
#[derive(Debug, Default)]
pub struct DecisionStats {
    pub requests_evaluated: AtomicU64,
    pub requests_blocked: AtomicU64,
    pub requests_excepted: AtomicU64,
}

/// Complete filter engine (network + cosmetic).
pub struct FilterEngine {
    network: NetworkEngine,
    cosmetic: CosmeticEngine,
    stats: ParseStats,
    decisions: DecisionStats,
}

impl FilterEngine {
    /// Build from raw list text (EasyList/ABP syntax, hosts-file lines
    /// supported). Multiple lists may be concatenated with newlines.
    pub fn from_lists(lists: &[&str]) -> FilterEngine {
        let mut stats = ParseStats::default();
        let mut network_rules: Vec<NetworkRule> = Vec::new();
        let mut cosmetic_rules: Vec<rule::CosmeticRule> = Vec::new();

        for list in lists {
            for line in list.lines() {
                match parser::parse_line(line, &mut stats) {
                    Ok(Some(rule::ParsedFilter::Network(r))) => network_rules.push(r),
                    Ok(Some(rule::ParsedFilter::Cosmetic(c))) => cosmetic_rules.push(c),
                    Ok(None) => {}
                    Err(_e) => {
                        // counted in stats.invalid by the parser
                    }
                }
            }
        }

        // badfilter cancellation: drop rules whose signature matches a
        // badfilter signature (ignoring the marker itself).
        let badfilter_sigs: Vec<String> = network_rules
            .iter()
            .filter(|r| r.include_domains.iter().any(|d| d == "\u{1}badfilter"))
            .map(badfilter_signature)
            .collect();
        network_rules.retain(|r| {
            if r.include_domains.iter().any(|d| d == "\u{1}badfilter") {
                return false;
            }
            !badfilter_sigs.contains(&badfilter_signature(r))
        });

        let engine = build_network_engine(network_rules);
        let cosmetic_engine = CosmeticEngine::new(cosmetic_rules);
        FilterEngine {
            network: engine,
            cosmetic: cosmetic_engine,
            stats,
            decisions: DecisionStats::default(),
        }
    }

    pub fn parse_stats(&self) -> &ParseStats {
        &self.stats
    }

    pub fn cosmetic(&self) -> &CosmeticEngine {
        &self.cosmetic
    }

    pub fn decision_counters(&self) -> &DecisionStats {
        &self.decisions
    }

    pub fn network_rule_count(&self) -> usize {
        self.network.rules.len()
    }

    /// Core decision: should this subresource request be blocked?
    ///
    /// * `site` — the top-level document URL (for `$domain=` and first/third-party)
    /// * `request` — the subresource URL
    /// * `dest_type_bit` — one `ResourceTypeMask` bit (0 = unknown type)
    /// * `third_party` — request host differs from site host
    pub fn should_block(
        &self,
        site: &Url,
        request: &Url,
        dest_type_bit: u32,
        third_party: bool,
    ) -> Decision {
        self.decisions.requests_evaluated.fetch_add(1, Ordering::Relaxed);
        let req_lower = lower_url(request.as_str());
        let req_raw = request.as_str();
        let site_host = site.host_str().unwrap_or("").to_ascii_lowercase();
        let req_host = request.host_str().unwrap_or("").to_ascii_lowercase();

        let mut best_block: Option<(&NetworkRule, bool)> = None; // (rule, important)
        let mut best_except: Option<&NetworkRule> = None;

        for idx in self.candidates(&req_lower) {
            let rule = &self.network.rules[idx as usize];

            // exact pattern verification
            if !self.verify_rule(rule, idx, &req_lower, req_raw) {
                #[cfg(feature = "brow-debug-decisions")]
                eprintln!("DBG verify-fail idx={idx} raw={}", rule.raw);
                continue;
            }
            // semantic gates
            if !self.rule_applies(rule, &site_host, &req_host, dest_type_bit, third_party) {
                #[cfg(feature = "brow-debug-decisions")]
                eprintln!("DBG applies-fail idx={idx} raw={}", rule.raw);
                continue;
            }
            // brow (7.2): document-level page flags ($generichide & co) on
            // exception rules never except a network request.
            if rule.hide_only {
                #[cfg(feature = "brow-debug-decisions")]
                eprintln!("DBG hide-only-skip idx={idx} raw={}", rule.raw);
                continue;
            }
            #[cfg(feature = "brow-debug-decisions")]
            eprintln!("DBG MATCH idx={idx} kind={:?} raw={}", rule.kind, rule.raw);
            match rule.kind {
                FilterKind::Block => {
                    let important = rule.important;
                    if best_block.map_or(true, |(_, imp)| important && !imp) {
                        best_block = Some((rule, important));
                    }
                }
                FilterKind::Exception => {
                    if best_except.is_none() {
                        best_except = Some(rule);
                    }
                }
            }
        }

        // Semantics: exception beats non-important block; important block
        // beats exception (EasyList `$important` semantics).
        match (best_except, best_block) {
            (Some(e), b) if b.map_or(true, |(_, imp)| !imp) => {
                self.decisions.requests_excepted.fetch_add(1, Ordering::Relaxed);
                Decision::AllowExcepted { rule: e.raw.clone() }
            }
            (_, Some((r, imp))) => {
                self.decisions.requests_blocked.fetch_add(1, Ordering::Relaxed);
                Decision::Block { rule: r.raw.clone(), important: imp }
            }
            (Some(e), None) => {
                self.decisions.requests_excepted.fetch_add(1, Ordering::Relaxed);
                Decision::AllowExcepted { rule: e.raw.clone() }
            }
            (None, None) => Decision::Allow,
        }
    }

    fn candidates(&self, url_lower: &str) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::with_capacity(8);
        // Overlapping iteration is REQUIRED: Standard/leftmost semantics
        // silently swallow matches contained inside other matches (e.g. the
        // needle `a.b^` shadows `a.b/c/d` at the same start position).
        for m in self.network.host_ac.find_overlapping_iter(url_lower) {
            if let Some(group) = self.network.host_groups.get(m.pattern().as_usize()) {
                out.extend_from_slice(group);
            }
        }
        for m in self.network.generic_ac.find_overlapping_iter(url_lower) {
            if let Some(group) = self.network.generic_groups.get(m.pattern().as_usize()) {
                out.extend_from_slice(group);
            }
        }
        out.extend_from_slice(&self.network.fallback);
        // `/regex/` rules are always candidates (their needles live inside
        // the regex source, not extractable as literals).
        for (idx, _) in &self.network.regex_rules {
            out.push(*idx);
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    fn verify_rule(&self, rule: &NetworkRule, idx: u32, url_lower: &str, url_raw: &str) -> bool {
        if rule.regex.is_some() {
            // compiled at build time (see build_network_engine); case-sensitive
            // per ABP regex semantics
            if let Ok(found) = self
                .network
                .regex_rules
                .binary_search_by_key(&idx, |(i, _)| *i)
            {
                return self.network.regex_rules[found].1.is_match(url_raw);
            }
            return false;
        }
        let hit = first_literal(&rule.pattern).and_then(|lit| memmem_first(url_lower, lit));
        verify(url_lower, rule, hit)
    }

    fn rule_applies(
        &self,
        rule: &NetworkRule,
        site_host: &str,
        req_host: &str,
        dest_type_bit: u32,
        third_party: bool,
    ) -> bool {
        // party scope
        match rule.party {
            PartyScope::Any => {}
            PartyScope::ThirdPartyOnly if third_party => {}
            PartyScope::FirstPartyOnly if !third_party => {}
            _ => return false,
        }
        // resource type: unknown destination (0) is conservative — only rules
        // without a type restriction apply. An empty positive mask
        // (`~image,~xhr` style) means "all types except the negated ones".
        if let Some(types) = &rule.types {
            if dest_type_bit == 0 {
                if types.0 != 0 {
                    return false;
                }
            } else if types.0 != 0 && !types.contains(dest_type_bit) {
                return false;
            }
        }
        if rule.not_types.contains(dest_type_bit) {
            return false;
        }
        // $domain include/exclude against the top-level site host
        if !rule.include_domains.is_empty() {
            let mut any = false;
            for d in &rule.include_domains {
                if d == "\u{1}badfilter" {
                    continue;
                }
                if domain_matches(site_host, d) {
                    any = true;
                    break;
                }
            }
            if !any {
                return false;
            }
        }
        for d in &rule.exclude_domains {
            if domain_matches(site_host, d) {
                return false;
            }
        }
        // first-party determination consistent with caller is fine; host
        // equality shortcut for rules that also check hosts (paranoia)
        let _ = (site_host, req_host);
        true
    }
}

fn memmem_first(hay: &str, needle: &str) -> Option<usize> {
    memchr::memmem::Finder::new(needle.as_bytes()).find(hay.as_bytes())
}

fn ends_with_label(host: &str, suffix: &str) -> bool {
    host == suffix || host.len() > suffix.len() && host.ends_with(suffix) && host.as_bytes()[host.len() - suffix.len() - 1] == b'.'
}

/// `$domain=` / entity matching with `example.*` TLD-wildcard support.
/// The wildcard covers exactly the TLD segment: a single label, or the
/// two-label public-suffix forms (`co.uk` & co) used by the same
/// approximation as `cname::registrable_suffix`.
pub fn domain_matches(host: &str, entry: &str) -> bool {
    if let Some(base) = entry.strip_suffix(".*") {
        if host == base {
            return true;
        }
        return match host.strip_prefix(base) {
            Some(rest) if rest.starts_with('.') => {
                let tld = &rest[1..];
                if tld.is_empty() || tld.contains('.') {
                    // multi-label TLD — only known two-level suffixes
                    let labels: Vec<&str> = tld.split('.').collect();
                    labels.len() == 2
                        && matches!(labels[0], "co" | "com" | "org" | "net" | "gov" | "edu" | "ac" | "or" | "ne")
                } else {
                    true
                }
            }
            _ => false,
        };
    }
    ends_with_label(host, entry)
}

fn badfilter_signature(rule: &NetworkRule) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = write!(s, "{:?}|{:?}", rule.anchor, rule.party);
    for p in &rule.pattern {
        match p {
            PatternPiece::Literal(l) => {
                let _ = write!(s, "|L:{l}");
            }
            PatternPiece::Wildcard => s.push_str("|W"),
            PatternPiece::Separator => s.push_str("|S"),
        }
    }
    let mut inc = rule
        .include_domains
        .iter()
        .filter(|d| d.as_str() != "\u{1}badfilter")
        .cloned()
        .collect::<Vec<_>>();
    inc.sort();
    let mut exc = rule.exclude_domains.clone();
    exc.sort();
    let _ = write!(s, "|inc:{inc:?}|exc:{exc:?}|types:{:?}|nt:{:?}|imp:{}|rx:{:?}", rule.types.map(|t| t.0), rule.not_types.0, rule.important, rule.regex);
    s
}

fn build_network_engine(mut rules: Vec<NetworkRule>) -> NetworkEngine {
    // Dedup identical raw rules (lists overlap heavily)
    rules.sort_by(|a, b| a.raw.cmp(&b.raw));
    rules.dedup_by(|a, b| a.raw == b.raw);

    let mut host_needles: Vec<String> = Vec::new();
    let mut host_groups: Vec<Vec<u32>> = Vec::new();
    let mut host_index: HashMap<String, u32> = HashMap::new();

    let mut generic_needles: Vec<String> = Vec::new();
    let mut generic_groups: Vec<Vec<u32>> = Vec::new();
    let mut generic_index: HashMap<String, u32> = HashMap::new();

    let mut fallback: Vec<u32> = Vec::new();
    let mut regex_rules: Vec<(u32, regex::Regex)> = Vec::new();

    for (idx, rule) in rules.iter().enumerate() {
        let idx = idx as u32;
        if let Some(src) = &rule.regex {
            // Cap regex evaluation to keep hostile lists DoS-safe; beyond
            // the cap rules are parsed and counted but not evaluated.
            if regex_rules.len() < REGEX_EVAL_CAP {
                if let Ok(re) = regex::Regex::new(src) {
                    regex_rules.push((idx, re));
                    continue;
                }
            }
            continue;
        }
        if rule.anchor == rule::Anchor::DoubleAnchor {
            if let Some(hint) = host_hint(&rule.pattern) {
                let gid = *host_index.entry(hint.clone()).or_insert_with(|| {
                    host_needles.push(hint.clone());
                    let gid = host_groups.len() as u32;
                    host_groups.push(Vec::new());
                    gid
                }) as usize;
                host_groups[gid].push(idx);
                continue;
            }
        }
        match first_literal(&rule.pattern) {
            Some(lit) => {
                let gid = *generic_index.entry(lit.to_string()).or_insert_with(|| {
                    generic_needles.push(lit.to_string());
                    let gid = generic_groups.len() as u32;
                    generic_groups.push(Vec::new());
                    gid
                }) as usize;
                generic_groups[gid].push(idx);
            }
            None => fallback.push(idx),
        }
    }

    let empty = vec!["\u{0}brow-no-needle".to_string()];
    let host_ac = matcher::build_ac(if host_needles.is_empty() { &empty } else { &host_needles });
    let generic_ac =
        matcher::build_ac(if generic_needles.is_empty() { &empty } else { &generic_needles });

    NetworkEngine {
        rules,
        host_groups,
        generic_groups,
        host_ac,
        generic_ac,
        fallback,
        regex_rules,
    }
}

/// Thread-safe shared handle.
pub type SharedFilterEngine = Arc<RwLock<FilterEngine>>;

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    const LISTS: &str = "\
[Adblock Plus 2.0]
! real-ish list
||doubleclick.net^
||ads.example.com^$script
||tracker.io^$third-party
/analytics/pixel-$image
@@||doubleclick.net^$image,domain=exception.com
banner*.gif
||example.com^$badfilter
||example.com^
0.0.0.0 hosts-tracker.example
##.generic-ad
adsite.example##.site-ad
adsite.example#@#.site-ad-exception
";

    fn engine() -> FilterEngine {
        FilterEngine::from_lists(&[LISTS])
    }

    #[test]
    fn blocks_doubleclick() {
        let e = engine();
        let site = url("https://news.com/article");
        let req = url("https://ad.doubleclick.net/ddm/adj/x");
        let d = e.should_block(&site, &req, ResourceTypeMask::XHR, true);
        assert!(matches!(d, Decision::Block { .. }), "{d:?}");
    }

    #[test]
    fn type_restriction_respected() {
        let e = engine();
        let site = url("https://news.com/article");
        // rule is $script — an image fetch must NOT match it
        let req = url("https://ads.example.com/pixel");
        let img = e.should_block(&site, &req, ResourceTypeMask::IMAGE, true);
        let script = e.should_block(&site, &req, ResourceTypeMask::SCRIPT, true);
        assert!(matches!(img, Decision::Allow));
        assert!(matches!(script, Decision::Block { .. }));
    }

    #[test]
    fn third_party_restriction() {
        let e = engine();
        let req = url("https://tracker.io/t?id=1");
        let third = e.should_block(&url("https://foo.com/"), &req, ResourceTypeMask::XHR, true);
        let first = e.should_block(&url("https://tracker.io/home"), &req.clone(), ResourceTypeMask::XHR, false);
        assert!(matches!(third, Decision::Block { .. }));
        assert!(matches!(first, Decision::Allow));
    }

    #[test]
    fn exception_wins_on_domain() {
        let e = engine();
        let site = url("https://exception.com/page");
        let req = url("https://ad.doubleclick.net/adx/img.gif");
        let d = e.should_block(&site, &req, ResourceTypeMask::IMAGE, true);
        assert!(matches!(d, Decision::AllowExcepted { .. }), "{d:?}");
        // other sites still blocked
        let d2 = e.should_block(
            &url("https://other.com/"),
            &url("https://ad.doubleclick.net/adx/img.gif"),
            ResourceTypeMask::IMAGE,
            true,
        );
        assert!(matches!(d2, Decision::Block { .. }));
    }

    #[test]
    fn badfilter_cancels_rule() {
        let e = engine();
        let site = url("https://news.com/");
        let req = url("https://sub.example.com/px");
        // `||example.com^` was cancelled by its badfilter twin — subdomain
        // sub.example.com must NOT be blocked.
        let d = e.should_block(&site, &req, ResourceTypeMask::OTHER, true);
        assert!(matches!(d, Decision::Allow), "{d:?}");
    }

    #[test]
    fn hosts_line_blocks() {
        let e = engine();
        let d = e.should_block(
            &url("https://foo.com/"),
            &url("https://hosts-tracker.example/px.gif"),
            ResourceTypeMask::IMAGE,
            true,
        );
        assert!(matches!(d, Decision::Block { .. }));
    }

    #[test]
    fn stats_are_counted() {
        let e = engine();
        let s = e.parse_stats();
        assert!(s.network_block > 5, "{s:?}");
        assert_eq!(s.cosmetic_generic, 1);
        assert_eq!(s.cosmetic_domain, 1);
        assert_eq!(s.cosmetic_unhide, 1);
        assert_eq!(s.comments, 2);
    }

    #[test]
    fn wildcard_pattern_matches() {
        let e = engine();
        let d = e.should_block(
            &url("https://foo.com/"),
            &url("https://cdn.foo.com/img/banner_728x90.gif"),
            ResourceTypeMask::IMAGE,
            false,
        );
        assert!(matches!(d, Decision::Block { .. }), "{d:?}");
    }
}
