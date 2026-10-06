/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Integration tests against the **real** vendored EasyList snapshot
//! (79,855 lines, 2026-10-06, commit cd705aa5 — see
//! assets/EASYLIST_LICENCE.md). These are the acceptance tests for the
//! filtering engine: real rules must produce the decisions a real
//! blocking stack would produce.
//!
//! Note on scope: EasyList blocks *adverts*. Tracker scripts
//! (google-analytics.com, googletagmanager.com, most fingerprinting) live
//! in the sister list **EasyPrivacy** — brow loads both in production, and
//! the engine treats them identically (concatenated text).

use brow_privacy::filter::rule::ResourceTypeMask;
use brow_privacy::filter::{Decision, FilterEngine};
use url::Url;

const EASYLIST: &str = include_str!("../assets/easylist-snapshot.txt");

fn engine() -> FilterEngine {
    FilterEngine::from_lists(&[EASYLIST])
}

fn decide(e: &FilterEngine, site: &str, req: &str, bit: u32) -> Decision {
    e.should_block(&Url::parse(site).unwrap(), &Url::parse(req).unwrap(), bit, true)
}

fn assert_blocked(e: &FilterEngine, site: &str, req: &str, bit: u32) {
    let d = decide(e, site, req, bit);
    assert!(matches!(d, Decision::Block { .. }), "expected Block for {req} on {site}, got {d:?}");
}

fn assert_allowed(e: &FilterEngine, site: &str, req: &str, bit: u32) {
    let d = decide(e, site, req, bit);
    assert!(
        matches!(d, Decision::Allow | Decision::AllowExcepted { .. }),
        "expected Allow for {req} on {site}, got {d:?}"
    );
}

#[test]
fn real_easylist_parses_cleanly() {
    let e = engine();
    let s = e.parse_stats();
    // corpus sanity
    assert!(s.lines_total > 70_000, "expected ~80k lines, got {}", s.lines_total);
    assert!(s.network_block > 50_000, "network blocks: {}", s.network_block);
    assert!(s.network_exception > 500, "exceptions: {}", s.network_exception);
    assert!(s.cosmetic_generic > 10_000, "generic cosmetic: {}", s.cosmetic_generic);
    assert!(s.cosmetic_domain > 5_000, "domain cosmetic: {}", s.cosmetic_domain);
    assert!(e.network_rule_count() > 50_000);
    // Upstream EasyList ships a handful of genuinely malformed element-hide
    // lines (`##ref^="..."` — broken selector bodies). The parser rejects
    // exactly those and counts them; nothing else in this corpus is invalid.
    assert!(s.invalid < 100, "invalid lines: {}", s.invalid);
}

#[test]
fn blocks_well_known_ad_hosts() {
    let e = engine();
    let site = "https://www.nytimes.com/";
    let cases: &[(&str, u32)] = &[
        // Google ad delivery
        ("https://securepubads.g.doubleclick.net/tag/js/gpt.js", ResourceTypeMask::SCRIPT),
        ("https://googleads.g.doubleclick.net/pagead/id", ResourceTypeMask::XHR),
        ("https://ad.doubleclick.net/ddm/adj/x", ResourceTypeMask::SCRIPT),
        ("https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js", ResourceTypeMask::SCRIPT),
        ("https://tpc.googlesyndication.com/pagead/js/loader.js", ResourceTypeMask::SCRIPT),
        // Twitter ads
        ("https://static.ads-twitter.com/uwt.js", ResourceTypeMask::SCRIPT),
    ];
    for (req, bit) in cases {
        assert_blocked(&e, site, req, *bit);
    }
}

#[test]
fn benign_subresources_flow_through() {
    let e = engine();
    let site = "https://www.nytimes.com/";
    let cases: &[(&str, u32)] = &[
        ("https://www.nytimes.com/vi-assets/static-assets/main.css", ResourceTypeMask::STYLESHEET),
        ("https://en.wikipedia.org/w/load.php?modules=startup", ResourceTypeMask::SCRIPT),
        ("https://www.wikipedia.org/portal/wikipedia.org/assets/img/Wikipedia-logo-v2.png", ResourceTypeMask::IMAGE),
        ("https://fonts.googleapis.com/css2?family=Roboto&display=swap", ResourceTypeMask::STYLESHEET),
        ("https://fonts.gstatic.com/s/roboto/v30/KFOm.woff2", ResourceTypeMask::FONT),
    ];
    for (req, bit) in cases {
        assert_allowed(&e, site, req, *bit);
    }
}

#[test]
fn out_of_scope_trackers_need_a_privacy_list() {
    // Documented engine behavior, not a limitation: EasyList is an AD list.
    // googletagmanager.com has zero rules in this corpus; the privacy list
    // (EasyPrivacy) supplies those rules in production and the engine
    // handles them identically — proved below by appending the rule.
    let e = engine();
    let s = e.parse_stats();
    let gtm_rules = EASYLIST
        .lines()
        .filter(|l| l.contains("googletagmanager") && !l.starts_with('!') && !l.starts_with('['))
        .count();
    assert_eq!(gtm_rules, 0, "test premise: no GTM rules in EasyList");
    assert_allowed(&e, "https://www.nytimes.com/", "https://www.googletagmanager.com/gtm.js?id=GTM-X", ResourceTypeMask::SCRIPT);

    let mut with_privacy = EASYLIST.to_string();
    with_privacy.push_str("\n||googletagmanager.com^\n||google-analytics.com^\n");
    let e2 = FilterEngine::from_lists(&[with_privacy.as_str()]);
    assert_blocked(
        &e2,
        "https://www.nytimes.com/",
        "https://www.googletagmanager.com/gtm.js?id=GTM-X",
        ResourceTypeMask::SCRIPT,
    );
    assert_blocked(
        &e2,
        "https://www.nytimes.com/",
        "https://www.google-analytics.com/analytics.js",
        ResourceTypeMask::SCRIPT,
    );
    let _ = s;
}

#[test]
fn exception_list_respected() {
    let e = engine();
    assert!(e.parse_stats().network_exception > 0);
    // Append a real-world-shaped exception and verify it wins over blocks.
    let mut with_exception = EASYLIST.to_string();
    with_exception.push_str(
        "\n@@||securepubads.g.doubleclick.net/tag/js/gpt.js$domain=trusted.example\n",
    );
    let e2 = FilterEngine::from_lists(&[with_exception.as_str()]);
    let d = decide(
        &e2,
        "https://trusted.example/",
        "https://securepubads.g.doubleclick.net/tag/js/gpt.js",
        ResourceTypeMask::SCRIPT,
    );
    assert!(matches!(d, Decision::AllowExcepted { .. }), "{d:?}");
    // ...but not for other sites
    let d2 = decide(
        &e2,
        "https://www.nytimes.com/",
        "https://securepubads.g.doubleclick.net/tag/js/gpt.js",
        ResourceTypeMask::SCRIPT,
    );
    assert!(matches!(d2, Decision::Block { .. }));
}

#[test]
fn important_block_beats_exception() {
    let mut list = EASYLIST.to_string();
    list.push_str("\n||important-test.example^$important\n@@||important-test.example^\n");
    let e = FilterEngine::from_lists(&[list.as_str()]);
    let d = decide(
        &e,
        "https://www.nytimes.com/",
        "https://important-test.example/px.js",
        ResourceTypeMask::SCRIPT,
    );
    assert!(matches!(d, Decision::Block { important: true, .. }), "{d:?}");
}

#[test]
fn engine_memory_budget() {
    // Measures the resident-set delta of building the full real-list engine.
    fn rss_kb() -> u64 {
        std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| {
                s.lines().find(|l| l.starts_with("VmRSS:")).and_then(|l| {
                    l.split_whitespace().nth(1).and_then(|v| v.parse().ok())
                })
            })
            .expect("VmRSS available on Linux")
    }
    let before = rss_kb();
    let e = engine();
    let after = rss_kb();
    let delta = after.saturating_sub(before);
    let n = e.network_rule_count();
    println!(
        "EasyList engine: {n} network rules, RSS delta = {} KiB ({} MiB)",
        delta,
        delta / 1024
    );
    // Generous CI guardrail; the release-mode figure belongs in the report.
    assert!(delta < 300 * 1024, "engine RSS delta {} KiB exceeds budget", delta);
}

#[test]
fn throughput_smoke() {
    // Not a benchmark — a smoke check that decisions are comfortably
    // fast even in debug/test builds. Criterion measures the real numbers.
    let e = engine();
    let site = Url::parse("https://www.news.example/x").unwrap();
    let start = std::time::Instant::now();
    let mut hits = 0;
    for i in 0..1000 {
        let req = Url::parse(&format!(
            "https://securepubads.g.doubleclick.net/tag/js/gpt.js?slot={}",
            i
        ))
        .unwrap();
        if matches!(
            e.should_block(&site, &req, ResourceTypeMask::SCRIPT, true),
            Decision::Block { .. }
        ) {
            hits += 1;
        }
    }
    let elapsed = start.elapsed();
    assert_eq!(hits, 1000, "GPT urls must all block");
    assert!(
        elapsed < std::time::Duration::from_secs(20),
        "1000 decisions took {:?} — pathological slowdown",
        elapsed
    );
    println!("1000 decisions in {:?} (debug build)", elapsed);
}
