/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Cross-module integration tests: CNAME cloaking feeding the filter
//! engine, CHIPS verdicts in storage/send flows, fingerprint payload
//! generation with per-site exceptions.

use std::collections::HashMap;
use std::time::Duration;

use url::Url;

use brow_privacy::chips::{self, ChipsConfig, ReceiveVerdict, SendVerdict, StoredCookie};
use brow_privacy::cname::{ChainResolver, CnameDetector, StaticChainResolver};
use brow_privacy::exceptions::ExceptionsStore;
use brow_privacy::fingerprint::{build_payload, DefenseLevel, FingerprintConfig, SessionKey};
use brow_privacy::stats::PrivacyStats;

fn cloak_detector() -> CnameDetector {
    let mut table = HashMap::new();
    // Classic cloaking: first-party-looking host aliases to tracker infra.
    table.insert(
        "metrics.news.example".to_string(),
        vec![
            "metrics.news.example".to_string(),
            "collector.trackad.example".to_string(),
        ],
    );
    CnameDetector::new(Box::new(StaticChainResolver { table }), Duration::from_secs(60))
}

#[test]
fn cname_verdict_feeds_blocking_stats() {
    let detector = cloak_detector();
    let site = Url::parse("https://news.example/").unwrap();
    let req = Url::parse("https://metrics.news.example/px.js").unwrap();
    let verdict = detector
        .inspect(&site, &req)
        .expect("cloaked hostname must be flagged");
    let canonical = match &verdict.reason {
        brow_privacy::cname::CloakReason::CrossDomainAlias { canonical, .. } => canonical.clone(),
    };

    let stats = PrivacyStats::new(None);
    stats.record_cname(&canonical);
    assert_eq!(stats.totals().1, 1);
    assert!(stats.top_hosts(1)[0].0.contains("trackad"));
}

#[test]
fn chips_receive_then_send_partitioned_roundtrip() {
    let cfg = ChipsConfig { block_third_party_unpartitioned: true, ..Default::default() };
    let top = Url::parse("https://shop.example/cart").unwrap();
    let set_cookie = "__Host-aid=xyz; Secure; Path=/; Partitioned";
    let v = chips::receive_policy(set_cookie, &top, cfg);
    let key = match v {
        ReceiveVerdict::Accept { partitioned: true, partition_key } => partition_key.unwrap(),
        other => panic!("expected accept-partitioned, got {other:?}"),
    };
    let stored = StoredCookie {
        domain: "ads.example".into(),
        partitioned: true,
        partition_key: Some(key),
    };
    // same top-level site later -> included
    let top2 = Url::parse("https://shop.example/checkout").unwrap();
    assert_eq!(
        chips::send_policy(&stored, &Url::parse("https://ads.example/i").unwrap(), &top2, false, cfg),
        SendVerdict::Include
    );
    // different top-level site -> omitted
    let top3 = Url::parse("https://news.example/").unwrap();
    assert_eq!(
        chips::send_policy(&stored, &Url::parse("https://ads.example/i").unwrap(), &top3, false, cfg),
        SendVerdict::OmitPartitionMismatch
    );
    // unpartitioned cookie in third-party context with blocking on -> omitted
    let plain = StoredCookie { domain: "ads.example".into(), partitioned: false, partition_key: None };
    assert_eq!(
        chips::send_policy(&plain, &Url::parse("https://ads.example/i").unwrap(), &top3, false, cfg),
        SendVerdict::OmitUnpartitionedThirdParty
    );
}

#[test]
fn exceptions_drive_engine_bypass_and_defense_level() {
    let store = ExceptionsStore::new(None);
    store.add_filter_allow("trusted.example".into());
    assert!(store.filter_allowed("trusted.example"));
    assert!(store.filter_allowed("sub.trusted.example"));
    assert!(!store.filter_allowed("nottrusted.example"));
    store.set_defense_level("https://trusted.example".into(), DefenseLevel::Off);
    assert_eq!(
        store.defense_level("https://trusted.example", DefenseLevel::Standard),
        DefenseLevel::Off
    );
}

#[test]
fn fingerprint_payload_respects_exception_level() {
    let store = ExceptionsStore::new(None);
    let session = SessionKey::from_entropy();
    let default_level = store.defense_level("https://mixed.example", DefenseLevel::Standard);
    let p_default = build_payload(default_level, &session, &FingerprintConfig::default());
    assert!(!p_default.script.is_empty());

    store.set_defense_level("https://mixed.example".into(), DefenseLevel::Off);
    let level = store.defense_level("https://mixed.example", DefenseLevel::Standard);
    let p_off = build_payload(level, &session, &FingerprintConfig::default());
    assert!(p_off.script.is_empty());
    assert!(p_off.active.is_empty());
}

#[test]
fn stats_persist_across_reload() {
    let dir = std::env::temp_dir();
    let path = dir.join(format!("brow-it-stats-{}.json", std::process::id()));
    let _ = std::fs::remove_file(&path);
    {
        let s = PrivacyStats::new(Some(path.clone()));
        s.record_block("ads.example");
        s.record_cookie_rejected();
        s.record_fingerprint_payload();
        s.persist();
    }
    let s2 = PrivacyStats::new(Some(path.clone()));
    let (blocks, cname, cookies_rej, _omitted, fp) = s2.totals();
    assert_eq!((blocks, cname, cookies_rej, fp), (1, 0, 1, 1));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn chain_resolver_trait_contract() {
    struct NoopResolver;
    impl ChainResolver for NoopResolver {
        fn resolve_chain(&self, host: &str) -> Vec<String> {
            vec![host.to_string()]
        }
    }
    let d = CnameDetector::new(Box::new(NoopResolver), Duration::from_secs(1));
    let v = d.inspect(
        &Url::parse("https://a.example/").unwrap(),
        &Url::parse("https://b.example/x").unwrap(),
    );
    assert!(v.is_none(), "single-entry chain (no CNAME) must not be flagged");
}
