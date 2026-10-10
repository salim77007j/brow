/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Element-hiding (cosmetic) engine.
//!
//! Produces, per top-level site:
//! - `hide`: selectors hidden on the site (generic + domain-scoped)
//! - `unhide`: `#@#` exceptions that cancel matching hides
//!
//! Procedural filters (`#?#`) are recorded but not applied (documented
//! limitation); selectors are sanity-checked at parse time to keep
//! malformed or hostile lists from injecting CSS.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use super::rule::CosmeticRule;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CosmeticResult {
    /// Selectors to hide — applied as `display: none` CSS / JS.
    pub hide: Vec<String>,
    /// `#@#` exceptions matching this site (cancel entries of `hide`).
    pub unhide: Vec<String>,
    /// Number of procedural rules recorded for this site (not applied).
    pub procedural_skipped: usize,
}

impl CosmeticResult {
    /// Selectors that survive unhide exceptions, deduplicated and sorted.
    pub fn effective(&self) -> Vec<String> {
        let un: std::collections::BTreeSet<&str> =
            self.unhide.iter().map(|s| s.as_str()).collect();
        self.hide
            .iter()
            .filter(|s| !un.contains(&s.as_str()))
            .cloned()
            .collect()
    }
}

#[derive(Debug, Default)]
struct DomainBucket {
    hide: Vec<u32>,
    unhide: Vec<u32>,
}

#[derive(Debug)]
struct CosmeticSelector {
    text: String,
    unhide: bool,
    include: Vec<String>,
    exclude: Vec<String>,
}

#[derive(Debug)]
pub struct CosmeticEngine {
    generic_hide: Vec<String>,
    /// brow (7.3): generic `#@#` exceptions (no domain part). The parser
    /// routes them to `DomainUnhide { include: [] }`, which used to be
    /// unreachable by `site_result` (no bucket to file them under) — a
    /// generic exception could never cancel a generic hide. They now live
    /// here and are subtracted by `generic_effective`.
    generic_unhide: Vec<String>,
    /// domain entry (or entity `example.*`) -> bucket of rule ids
    by_domain: HashMap<String, DomainBucket>,
    /// entity-style keys (with `.*`) for wildcard-TLD matching
    entity_keys: Vec<String>,
    selectors: Vec<CosmeticSelector>,
    procedural_count: usize,
}

impl CosmeticEngine {
    pub fn new(rules: Vec<CosmeticRule>) -> CosmeticEngine {
        let mut generic_hide = Vec::new();
        let mut generic_unhide: Vec<String> = Vec::new();
        let mut selectors: Vec<CosmeticSelector> = Vec::new();
        let mut by_domain: HashMap<String, DomainBucket> = HashMap::new();
        let mut entity_keys: Vec<String> = Vec::new();
        let mut procedural_count = 0usize;

        for rule in rules {
            let unhide = matches!(&rule, CosmeticRule::DomainUnhide { .. });
            match rule {
                CosmeticRule::GenericHide { selector, .. } => {
                    generic_hide.push(selector);
                }
                CosmeticRule::DomainUnhide {
                    include,
                    selector,
                    ..
                } if include.is_empty() => {
                    // brow (7.3): generic `#@#` — no domain to bucket under;
                    // cancels generic hides globally (see generic_unhide).
                    generic_unhide.push(selector);
                }
                CosmeticRule::DomainHide {
                    include,
                    exclude,
                    selector,
                    ..
                }
                | CosmeticRule::DomainUnhide {
                    include,
                    exclude,
                    selector,
                    ..
                } => {
                    let id = selectors.len() as u32;
                    selectors.push(CosmeticSelector {
                        text: selector,
                        unhide,
                        include: include.clone(),
                        exclude,
                    });
                    for domain in include {
                        let bucket = by_domain.entry(domain.clone()).or_default();
                        if unhide {
                            bucket.unhide.push(id);
                        } else {
                            bucket.hide.push(id);
                        }
                        if domain.ends_with(".*") && !entity_keys.contains(&domain) {
                            entity_keys.push(domain.clone());
                        }
                    }
                }
                CosmeticRule::Procedural { .. } => {
                    procedural_count += 1;
                }
            }
        }

        CosmeticEngine {
            generic_hide,
            generic_unhide,
            by_domain,
            entity_keys,
            selectors,
            procedural_count,
        }
    }

    /// Compute the cosmetic result for a site hostname (lowercase).
    pub fn site_result(&self, site_host: &str) -> CosmeticResult {
        let mut hide: BTreeMap<String, ()> = BTreeMap::new();
        let mut unhide: BTreeMap<String, ()> = BTreeMap::new();

        // Generic hides apply on every site.
        for s in &self.generic_hide {
            hide.insert(s.clone(), ());
        }

        // Candidate buckets: every label suffix of the site host
        // (www.foo.com -> www.foo.com, foo.com, com).
        let labels: Vec<&str> = site_host.split('.').collect();
        for i in 0..labels.len() {
            let candidate = labels[i..].join(".");
            if let Some(bucket) = self.by_domain.get(&candidate) {
                self.apply_bucket(site_host, bucket, &mut hide, &mut unhide);
            }
        }
        // Entity keys (`example.*`).
        for key in &self.entity_keys {
            if super::domain_matches(site_host, key) {
                if let Some(bucket) = self.by_domain.get(key) {
                    self.apply_bucket(site_host, bucket, &mut hide, &mut unhide);
                }
            }
        }

        CosmeticResult {
            hide: hide.into_keys().collect(),
            unhide: unhide.into_keys().collect(),
            procedural_skipped: self.procedural_count,
        }
    }

    fn apply_bucket(
        &self,
        site_host: &str,
        bucket: &DomainBucket,
        hide: &mut BTreeMap<String, ()>,
        unhide: &mut BTreeMap<String, ()>,
    ) {
        for id in bucket.hide.iter().chain(bucket.unhide.iter()) {
            let sel = &self.selectors[*id as usize];
            let applies = sel.include.iter().any(|d| super::domain_matches(site_host, d))
                && !sel.exclude.iter().any(|d| super::domain_matches(site_host, d));
            if !applies {
                continue;
            }
            if sel.unhide {
                unhide.insert(sel.text.clone(), ());
            } else {
                hide.insert(sel.text.clone(), ());
            }
        }
    }

    pub fn generic_count(&self) -> usize {
        self.generic_hide.len()
    }

    pub fn domain_rule_count(&self) -> usize {
        self.selectors.len()
    }

    /// brow (7.3): the generic element-hiding plane — selectors that apply
    /// on EVERY site, minus generic exceptions. Ships once as a global user
    /// stylesheet (parsed once, Origin::User, shared by every document via
    /// ScriptThreadUserContents); NOT repeated per document.
    pub fn generic_effective(&self) -> Vec<String> {
        let un: std::collections::BTreeSet<&str> =
            self.generic_unhide.iter().map(|s| s.as_str()).collect();
        let mut out: Vec<String> = self
            .generic_hide
            .iter()
            .filter(|s| !un.contains(&s.as_str()))
            .cloned()
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// brow (7.3): the domain-scoped plane ONLY — generic hides excluded
    /// (they ship as the global user stylesheet; re-injecting them per
    /// document would duplicate the CSS). Applied per document by the
    /// engine at head-bind time.
    pub fn site_scoped_result(&self, site_host: &str) -> CosmeticResult {
        let mut hide: BTreeMap<String, ()> = BTreeMap::new();
        let mut unhide: BTreeMap<String, ()> = BTreeMap::new();

        // Candidate buckets: every label suffix of the site host
        // (www.foo.com -> www.foo.com, foo.com, com).
        let labels: Vec<&str> = site_host.split('.').collect();
        for i in 0..labels.len() {
            let candidate = labels[i..].join(".");
            if let Some(bucket) = self.by_domain.get(&candidate) {
                self.apply_bucket(site_host, bucket, &mut hide, &mut unhide);
            }
        }
        // Entity keys (`example.*`).
        for key in &self.entity_keys {
            if super::domain_matches(site_host, key) {
                if let Some(bucket) = self.by_domain.get(key) {
                    self.apply_bucket(site_host, bucket, &mut hide, &mut unhide);
                }
            }
        }

        CosmeticResult {
            hide: hide.into_keys().collect(),
            unhide: unhide.into_keys().collect(),
            procedural_skipped: self.procedural_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::parser::parse_line;
    use crate::filter::rule::{ParseStats, ParsedFilter};

    fn engine(lines: &[&str]) -> (Vec<CosmeticRule>, usize) {
        let mut st = ParseStats::default();
        let mut out = Vec::new();
        for l in lines {
            if let Ok(Some(ParsedFilter::Cosmetic(c))) = parse_line(l, &mut st) {
                out.push(c);
            }
        }
        (out, st.invalid)
    }

    #[test]
    fn generic_and_domain_hides() {
        let (rules, invalid) = engine(&["##.ad", "foo.com##.promo", "bar.foo.com##.local"]);
        assert_eq!(invalid, 0);
        let e = CosmeticEngine::new(rules);
        let r = e.site_result("www.foo.com");
        let eff = r.effective();
        assert!(eff.contains(&".ad".to_string()));
        assert!(eff.contains(&".promo".to_string()));
        assert!(!eff.contains(&".local".to_string()));
    }

    #[test]
    fn unhide_cancels() {
        let (rules, _) = engine(&["foo.com##.ad", "foo.com#@#.ad"]);
        let e = CosmeticEngine::new(rules);
        let r = e.site_result("foo.com");
        assert!(r.hide.contains(&".ad".to_string()));
        assert!(r.unhide.contains(&".ad".to_string()));
        assert!(r.effective().is_empty());
    }

    #[test]
    fn entity_wildcard_domain() {
        let (rules, _) = engine(&["example.*##.ent"]);
        let e = CosmeticEngine::new(rules);
        assert!(e
            .site_result("example.com")
            .effective()
            .contains(&".ent".to_string()));
        assert!(e
            .site_result("example.co.uk")
            .effective()
            .contains(&".ent".to_string()));
        assert!(!e
            .site_result("notexample.com")
            .effective()
            .contains(&".ent".to_string()));
    }

    #[test]
    fn exclude_domain_limits_scope() {
        let (rules, _) = engine(&["foo.com,~bar.foo.com##.scoped"]);
        let e = CosmeticEngine::new(rules);
        assert!(e.site_result("www.foo.com").effective().contains(&".scoped".to_string()));
        assert!(!e.site_result("bar.foo.com").effective().contains(&".scoped".to_string()));
    }

    #[test]
    fn generic_unhide_cancels_generic_hide() {
        // brow (7.3): generic #@# used to be silently dropped (empty
        // include never bucketed) — the exception could never cancel.
        let (rules, _) = engine(&["##.ad", "#@#.ad", "##.banner"]);
        let e = CosmeticEngine::new(rules);
        let gen = e.generic_effective();
        assert!(gen.contains(&".banner".to_string()));
        assert!(!gen.contains(&".ad".to_string()));
    }

    #[test]
    fn site_scoped_excludes_generics() {
        let (rules, _) = engine(&["##.ad", "foo.com##.promo", "foo.com#@#.promo"]);
        let e = CosmeticEngine::new(rules);
        // site_scoped_result: only the domain plane.
        let r = e.site_scoped_result("www.foo.com");
        assert!(!r.hide.contains(&".ad".to_string()));
        assert!(r.hide.contains(&".promo".to_string()));
        assert!(r.unhide.contains(&".promo".to_string()));
        assert!(r.effective().is_empty());
        // site_result (full, unchanged semantics) still includes generics.
        let full = e.site_result("www.foo.com");
        assert!(full.hide.contains(&".ad".to_string()));
    }
}
