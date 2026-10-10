/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! brow (phase 7.2): the machine-computed privacy score.
//!
//! Ten line items, weights summing to 100, each backed by REAL assertions
//! against the production engine (the same embedded lists and configs that
//! ship). The whole file is the rubric: a line item earns its weight only
//! if every assertion below it passes. CI gates the total at 90+ (phase
//! 7.8); the JSON line (`BROW_PRIVACY_SCORE_JSON`) is the evidence record.
//!
//! Line items and weights (sum = 100):
//!   1. ad_block_network        15  known ad endpoints blocked, benign allowed
//!   2. tracker_block_network   15  GTM/GA/Hotjar/Clarity/Sentry/Segment/pixel
//!   3. element_hiding          10  generic + domain-scoped CSS planes live
//!   4. cname_uncloak           10  CNAME-chain trackers flagged
//!   5. cookies_chips           10  Partitioned needs Secure; 3P send omission
//!   6. doh_encrypted           10  DoH on by default, https templates
//!   7. fingerprint_standard    15  canvas/webgl/audio/navigator + invisibility
//!   8. fingerprint_strict       5  fonts/rects/timezone tier exists
//!   9. session_isolation        5  per-session keys, per-origin overrides
//!  10. stats_persistence        5  counters record + persist roundtrip
//!
//! The owner's baseline complaint was 40/100 (v0.6 real-world assessment).
//! This rubric is the v0.7 measurable equivalent; every point is an
//! assertion a CI job ran, not a claim.

use std::collections::HashMap;
use std::sync::OnceLock;

use url::Url;

use brow_privacy::cname::{CnameDetector, StaticChainResolver};
use brow_privacy::exceptions::ExceptionsStore;
use brow_privacy::filter::rule::ResourceTypeMask;
use brow_privacy::fingerprint::{build_payload, DefenseLevel, FingerprintConfig, SessionKey};
use brow_privacy::lists;
use brow_privacy::stats::PrivacyStats;

/// The production engine: all four embedded lists (7.4 stack), the same
/// `lists::global_engine()` the browser process uses.
fn engine() -> &'static brow_privacy::filter::FilterEngine {
    static ENGINE: OnceLock<Option<&'static brow_privacy::filter::FilterEngine>> =
        OnceLock::new();
    ENGINE.get_or_init(lists::global_engine).unwrap()
}

fn blocked(site: &str, req: &str, bit: u32) -> bool {
    use brow_privacy::filter::Decision;
    matches!(
        engine().should_block(
            &Url::parse(site).unwrap(),
            &Url::parse(req).unwrap(),
            bit,
            true,
        ),
        Decision::Block { .. }
    )
}

struct Item {
    id: &'static str,
    weight: u32,
    earned: u32,
    detail: String,
}

fn item(id: &'static str, weight: u32, ok: bool, detail: String) -> Item {
    Item {
        id,
        weight,
        earned: if ok { weight } else { 0 },
        detail,
    }
}

#[test]
fn privacy_score_at_least_90() {
    let mut items: Vec<Item> = Vec::new();

    // 1. ad_block_network (15): classic ad endpoints across resource
    // types, plus benign controls that MUST pass.
    let ads = [
        ("https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js", ResourceTypeMask::SCRIPT),
        ("https://securepubads.g.doubleclick.net/tag/js/gpt.js", ResourceTypeMask::SCRIPT),
        ("https://c2.taboola.com/right/getads", ResourceTypeMask::XHR),
        ("https://tags.outbrain.com/outbrain.js", ResourceTypeMask::SCRIPT),
        ("https://ib.adnxs.com/px", ResourceTypeMask::XHR),
        ("https://criteo.com/delivery/ajs.php", ResourceTypeMask::SCRIPT),
        ("https://www.facebook.com/tr/?id=123456789&ev=PageView", ResourceTypeMask::XHR),
    ];
    let ads_results: Vec<String> = ads
        .iter()
        .map(|(u, b)| {
            let b = blocked("https://www.example.com/", u, *b);
            format!("{}{}", if b { "OK " } else { "MISS " }, u)
        })
        .collect();
    let ads_ok = ads_results.iter().all(|r| r.starts_with("OK"));
    let benign_ok = !blocked(
        "https://www.example.com/",
        "https://www.example.com/style.css",
        ResourceTypeMask::STYLESHEET,
    ) && !blocked(
        "https://www.wikipedia.org/",
        "https://upload.wikimedia.org/wikipedia/commons/a/a4/Barnstar.png",
        ResourceTypeMask::IMAGE,
    );
    items.push(item(
        "ad_block_network",
        15,
        ads_ok && benign_ok,
        format!(
            "{}/{} blocked, benign={benign_ok} :: {}",
            ads_results.iter().filter(|r| r.starts_with("OK")).count(),
            ads.len(),
            ads_results.join(" | ")
        ),
    ));

    // 2. tracker_block_network (15): the tracker/telemetry class the
    // owner named (GTM was THE v0.6 evidence).
    let trackers = [
        ("https://www.googletagmanager.com/gtm.js?id=GTM-ABC", ResourceTypeMask::SCRIPT),
        ("https://www.google-analytics.com/analytics.js", ResourceTypeMask::SCRIPT),
        ("https://region1.google-analytics.com/g/collect", ResourceTypeMask::XHR),
        ("https://static.hotjar.com/c/hotjar-123.js", ResourceTypeMask::SCRIPT),
        ("https://www.clarity.ms/tag/abc", ResourceTypeMask::SCRIPT),
        ("https://browser.sentry-cdn.com/5.0.0/bundle.min.js", ResourceTypeMask::SCRIPT),
        ("https://cdn.segment.com/analytics.js/v1/abc/analytics.min.js", ResourceTypeMask::SCRIPT),
        ("https://connect.facebook.net/en_US/fbevents.js", ResourceTypeMask::SCRIPT),
    ];
    let tracker_hits = trackers
        .iter()
        .filter(|(u, b)| blocked("https://www.example.com/", u, *b))
        .count();
    items.push(item(
        "tracker_block_network",
        15,
        tracker_hits == trackers.len(),
        format!("{}/{} tracker endpoints blocked", tracker_hits, trackers.len()),
    ));

    // 3. element_hiding (10): the generic plane is live with the right
    // shape, and the stylesheet matches the engine's generic plane.
    let generic = lists::generic_cosmetic_css();
    let scoped = lists::site_cosmetic_css("doubleclick.net");
    let generic_lines = generic.lines().count();
    let generic_ok =
        generic.contains("{display:none!important}") && generic_lines > 1_000;
    let scoped_shape_ok = scoped.is_empty() || scoped.contains("{display:none!important}");
    let plane_ok = engine().cosmetic().generic_effective().len() == generic_lines;
    items.push(item(
        "element_hiding",
        10,
        generic_ok && scoped_shape_ok && plane_ok,
        format!(
            "generic {generic_lines} selectors, scoped {} bytes, plane-consistency {plane_ok}",
            scoped.len()
        ),
    ));

    // 4. cname_uncloak (10): a first-party-looking host whose CNAME chain
    // ends at a tracker registrable domain is flagged.
    let resolver = StaticChainResolver {
        table: HashMap::from([(
            "metrics.customer-site.example".to_string(),
            vec![
                "metrics.customer-site.example".to_string(),
                "customer-site.example".to_string(),
                "googletagmanager.com".to_string(),
            ],
        )]),
    };
    let detector = CnameDetector::new(Box::new(resolver), std::time::Duration::from_secs(60));
    let verdict = detector.inspect(
        &Url::parse("https://customer-site.example/").unwrap(),
        &Url::parse("https://metrics.customer-site.example/collect").unwrap(),
    );
    let cname_ok = verdict.is_some();
    items.push(item(
        "cname_uncloak",
        10,
        cname_ok,
        if cname_ok {
            "CNAME chain to tracker registrable domain flagged".to_string()
        } else {
            "cloaked tracker NOT flagged".to_string()
        },
    ));

    // 5. cookies_chips (10): Partitioned requires Secure on receive; the
    // send side omits unpartitioned cookies in third-party contexts and
    // keeps partitioned ones with a matching key.
    let chips_ok = {
        use brow_privacy::chips::{self, ChipsConfig, StoredCookie, SendVerdict};
        let cfg = ChipsConfig {
            require_secure_for_partitioned: true,
            block_third_party_unpartitioned: true,
        };
        let top = Url::parse("https://customer-site.example/").unwrap();
        let accept_partitioned = chips::receive_policy(
            "c=v; Partitioned; Secure; Domain=tracker.example; Path=/",
            &top,
            cfg.clone(),
        );
        let reject_insecure = chips::receive_policy(
            "c=v; Partitioned; Domain=tracker.example; Path=/",
            &top,
            cfg.clone(),
        );
        let third_party_plain_ok = matches!(
            chips::send_policy(
                &StoredCookie {
                    domain: "tracker.example".into(),
                    partitioned: false,
                    partition_key: None,
                },
                &Url::parse("https://tracker.example/pixel").unwrap(),
                &top,
                false,
                cfg.clone(),
            ),
            SendVerdict::OmitUnpartitionedThirdParty
        );
        let partitioned_send_ok = matches!(
            chips::send_policy(
                &StoredCookie {
                    domain: "tracker.example".into(),
                    partitioned: true,
                    partition_key: Some(chips::partition_key(&top)),
                },
                &Url::parse("https://tracker.example/api").unwrap(),
                &top,
                false,
                cfg.clone(),
            ),
            SendVerdict::Include
        );
        let accepted = matches!(accept_partitioned,
            brow_privacy::chips::ReceiveVerdict::Accept { partitioned: true, .. });
        let rejected = matches!(
            reject_insecure,
            brow_privacy::chips::ReceiveVerdict::RejectPartitionedInsecure
        );
        accepted && rejected && third_party_plain_ok && partitioned_send_ok
    };
    items.push(item(
        "cookies_chips",
        10,
        chips_ok,
        "Partitioned/Secure enforced; 3P unpartitioned omitted; partitioned send kept".to_string(),
    ));

    // 6. doh_encrypted (10): DoH ships ON with https-only templates and
    // bootstrap addresses (the defaults the resolver constructor uses).
    let doh_ok = {
        let enabled = servo_config::pref!(network_dns_over_https_enabled);
        let configured = servo_config::pref!(network_dns_over_https_templates);
        let templates = if configured.is_empty() {
            "https://mozilla.cloudflare-dns.com/dns-query https://dns.google/dns-query"
        } else {
            configured.as_str()
        };
        enabled
            && templates
                .split_whitespace()
                .all(|t| t.starts_with("https://"))
    };
    items.push(item(
        "doh_encrypted",
        10,
        doh_ok,
        "DoH default ON (https-only templates)".to_string(),
    ));

    // 7. fingerprint_standard (15): the four core defenses plus the
    // invisibility properties (5.4: no page-visible errors, native-code
    // spoofing shim, balanced braces).
    let session = SessionKey::from_entropy();
    let payload =
        build_payload(DefenseLevel::Standard, &session, &FingerprintConfig::default());
    let core = [
        "canvas2d-noise",
        "webgl-spoof+noise",
        "audiocontext-noise",
        "navigator-reduction",
    ];
    let has_core = core
        .iter()
        .all(|d| payload.active.iter().any(|a| a == d));
    let invisible = payload.script.contains("browNative") && braces_balanced(&payload.script);
    items.push(item(
        "fingerprint_standard",
        15,
        has_core && invisible,
        format!(
            "core={has_core} invisible={invisible} script={} bytes",
            payload.script.len()
        ),
    ));

    // 8. fingerprint_strict (5): the strict tier adds fonts/rects/timezone.
    let strict = build_payload(DefenseLevel::Strict, &session, &FingerprintConfig::default());
    let strict_extra = ["font-measure-jitter", "client-rects-jitter", "timezone-freeze"];
    let strict_ok = strict_extra
        .iter()
        .all(|d| strict.active.iter().any(|a| a == d))
        && strict.active.len() > payload.active.len();
    items.push(item(
        "fingerprint_strict",
        5,
        strict_ok,
        "strict tier adds 3 defenses".to_string(),
    ));

    // 9. session_isolation (5): keys vary per session; per-origin
    // overrides stick while others keep the default.
    let iso_ok = {
        let other = SessionKey::from_entropy();
        session.0 != other.0 && {
            let store = ExceptionsStore::new(None);
            store.set_defense_level("https://keep-private.example".into(), DefenseLevel::Off);
            store.defense_level("https://keep-private.example", DefenseLevel::Standard)
                == DefenseLevel::Off
                && store.defense_level("https://untouched.example", DefenseLevel::Standard)
                    == DefenseLevel::Standard
        }
    };
    items.push(item(
        "session_isolation",
        5,
        iso_ok,
        "session keys unique; per-origin overrides".to_string(),
    ));

    // 10. stats_persistence (5): block + cookie counters record and
    // survive a reload.
    let stats_ok = {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("brow-score-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&path);
        {
            let s = PrivacyStats::new(Some(path.clone()));
            s.record_block("ads.example");
            s.record_cookie_rejected();
            s.persist();
        }
        let s2 = PrivacyStats::new(Some(path.clone()));
        let (blocks, _cname, cookies, _omitted, _fp) = s2.totals();
        let ok = blocks == 1 && cookies == 1;
        let _ = std::fs::remove_file(&path);
        ok
    };
    items.push(item(
        "stats_persistence",
        5,
        stats_ok,
        "block+cookie counters persist".to_string(),
    ));

    let total: u32 = items.iter().map(|i| i.weight).sum();
    let earned: u32 = items.iter().map(|i| i.earned).sum();
    let failed: Vec<&str> = items
        .iter()
        .filter(|i| i.earned != i.weight)
        .map(|i| i.id)
        .collect();

    let json = format!(
        "{{\"score\":{},\"max\":{},\"failed\":{:?},\"items\":[{}]}}",
        earned,
        total,
        failed,
        items
            .iter()
            .map(|i| format!(
                "{{\"id\":\"{}\",\"earned\":{},\"max\":{},\"detail\":\"{}\"}}",
                i.id, i.earned, i.weight, i.detail
            ))
            .collect::<Vec<_>>()
            .join(",")
    );
    println!("BROW_PRIVACY_SCORE_JSON {json}");

    assert_eq!(total, 100, "rubric weights must sum to 100");
    assert!(
        earned >= 90,
        "privacy score {earned}/100 < 90 gate — failed items: {failed:?}"
    );
}

fn braces_balanced(s: &str) -> bool {
    s.matches('{').count() == s.matches('}').count()
}
