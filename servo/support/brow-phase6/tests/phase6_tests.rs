/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Hermetic integration tests for the Phase 6 harness (no network).

use brow_phase6::metrics::{percentile, summarize, FetchOutcome, FetchRecord};
use brow_phase6::parse::parse_html_bytes;
use brow_phase6::report;

fn record(category: &str, url: &str, outcome: FetchOutcome, ttfb: u64, total: u64, bytes: u64) -> FetchRecord {
    FetchRecord {
        category: category.into(),
        url: url.into(),
        http_version: "HTTP/2".into(),
        status: 200,
        tcp_connect_ms: Some(9),
        ttfb_ms: ttfb,
        total_ms: total,
        body_bytes: bytes,
        outcome,
        error: None,
        parse: None,
    }
}

#[test]
fn full_summary_pipeline_is_consistent() {
    let recs = vec![
        record("news", "https://a/", FetchOutcome::Ok, 80, 200, 50_000),
        record("news", "https://b/", FetchOutcome::Ok, 120, 300, 80_000),
        record("news", "https://c/", FetchOutcome::HttpError, 90, 150, 300),
        record("ecom", "https://d/", FetchOutcome::Timeout, 20_000, 20_000, 0),
    ];
    let news = summarize("news", &recs.iter().filter(|r| r.category == "news").cloned().collect::<Vec<_>>().as_slice());
    let ecom = summarize("ecom", &recs.iter().filter(|r| r.category == "ecom").cloned().collect::<Vec<_>>().as_slice());
    assert_eq!(news.attempts, 3);
    assert_eq!(news.ok, 2);
    assert_eq!(news.ttfb_p50_ms, Some(90));
    assert_eq!(news.ttfb_p90_ms, Some(120));
    assert_eq!(ecom.attempts, 1);
    assert_eq!(ecom.timeout, 1);
}

#[test]
fn markdown_artifacts_render_for_mixed_records() {
    let recs = vec![
        record("wiki", "https://w1/", FetchOutcome::Ok, 100, 250, 300_000),
        record("wiki", "https://w2/", FetchOutcome::TransportError, 0, 0, 0),
    ];
    let md = report::detail_markdown(&recs);
    assert!(md.contains("transport-error"));
    assert!(md.contains("293.0 KiB"), "byte format: {md}");
    let s = summarize("wiki", &recs.as_slice());
    let table = report::bench_markdown(std::slice::from_ref(&s));
    assert!(table.contains("1/0/0/1"));
}

#[test]
fn parse_pipeline_on_realistic_fixture() {
    // A realistic-ish page: doctype, metadata, nested structure, entities.
    let fixture = br#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>brow fixture</title></head>
<body>
  <header><h1>Hello &amp; welcome</h1></header>
  <main>
    <article><p>First paragraph.</p><p>Second one.</p></article>
    <ul><li>one</li><li>two</li><li>three</li></ul>
    <table><tr><td>cell</td></tr></table>
  </main>
  <footer>fin</footer>
</body>
</html>"#;
    let s = parse_html_bytes(fixture).unwrap();
    assert!(s.elements >= 15, "elements: {}", s.elements);
    assert!(s.text_chars >= 60, "text chars: {}", s.text_chars);
    assert!(s.max_depth >= 5, "depth: {}", s.max_depth);
    assert!(s.nodes >= s.elements);
}

#[test]
fn percentile_matches_sorted_expectation() {
    let mut v: Vec<u64> = (0..1000).map(|i| (i * 37) % 997).collect();
    v.sort_unstable();
    let p90 = percentile(&v, 90.0).unwrap();
    // nearest-rank on 1000 samples: ceil(0.9*1000)=900th sample (1-based)
    assert_eq!(p90, v[899]);
}
