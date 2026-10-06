/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Markdown + JSON report rendering for the Phase 6 artifacts.

use crate::metrics::{CategorySummary, FetchOutcome, FetchRecord, StressSummary};

/// Render the per-category benchmark table.
pub fn bench_markdown(summaries: &[CategorySummary]) -> String {
    let mut out = String::new();
    out.push_str("| Category | OK/Err/T/O | TTFB p50 | TTFB p90 | Total p50 | Body p50 | Parse p50 | Nodes p50 |\n");
    out.push_str("|---|---|---:|---:|---:|---:|---:|---:|\n");
    for s in summaries {
        out.push_str(&format!(
            "| {} | {}/{}/{}/{} | {} | {} | {} | {} | {} | {} |\n",
            s.category,
            s.ok,
            s.http_error,
            s.timeout,
            s.transport_error,
            fmt_opt(s.ttfb_p50_ms, "ms"),
            fmt_opt(s.ttfb_p90_ms, "ms"),
            fmt_opt(s.total_p50_ms, "ms"),
            fmt_bytes(s.body_bytes_p50),
            fmt_opt(s.parse_ms_p50, "ms"),
            fmt_opt(s.nodes_p50, ""),
        ));
    }
    out
}

/// Render the stress-test summary block.
pub fn stress_markdown(s: &StressSummary) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "- rounds: {} · requests: {} · ok: {} · failed: {} · success rate: {:.1}%\n",
        s.rounds, s.requests, s.ok, s.failed, s.success_rate_pct
    ));
    out.push_str(&format!(
        "- TTFB p50 / p90 / p99: {} / {} / {} ms\n",
        fmt_opt(s.ttfb_p50_ms, "ms"),
        fmt_opt(s.ttfb_p90_ms, "ms"),
        fmt_opt(s.ttfb_p99_ms, "ms")
    ));
    out.push_str(&format!(
        "- Total p50 / p99: {} / {} ms\n",
        fmt_opt(s.total_p50_ms, "ms"),
        fmt_opt(s.total_p99_ms, "ms")
    ));
    out.push_str(&format!(
        "- wall: {:.1} s · throughput: {:.2} req/s\n",
        s.wall_seconds, s.requests_per_sec
    ));
    out.push_str(&format!(
        "- harness RSS p50 / max: {} / {} KiB\n",
        fmt_opt(s.rss_kib_p50, ""),
        fmt_opt(s.rss_kib_max, "")
    ));
    out.push_str("\nPer-category (stress):\n\n");
    out.push_str(&bench_markdown(&s.per_category));
    out
}

/// Render the per-URL detail table (bench pipeline, one line per URL).
pub fn detail_markdown(records: &[FetchRecord]) -> String {
    let mut out = String::new();
    out.push_str("| Category | URL | Ver | Status | Connect | TTFB | Total | Bytes | Outcome | Nodes | Parse |\n");
    out.push_str("|---|---|---|---:|---:|---:|---:|---:|---|---:|---:|\n");
    for r in records {
        let (nodes, parse_ms) = match &r.parse {
            Some(p) => (p.nodes.to_string(), format!("{} ms", p.parse_us / 1000)),
            None => ("-".into(), "-".into()),
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} ms | {} ms | {} | {} | {} | {} |\n",
            r.category,
            r.url,
            r.http_version,
            r.status,
            fmt_opt(r.tcp_connect_ms, "ms"),
            r.ttfb_ms,
            r.total_ms,
            fmt_bytes(Some(r.body_bytes)),
            outcome_label(r.outcome),
            nodes,
            parse_ms,
        ));
    }
    out
}

fn outcome_label(o: FetchOutcome) -> &'static str {
    match o {
        FetchOutcome::Ok => "ok",
        FetchOutcome::HttpError => "http-error",
        FetchOutcome::Timeout => "timeout",
        FetchOutcome::TransportError => "transport-error",
        FetchOutcome::ParseError => "parse-error",
    }
}

fn fmt_opt(v: Option<u64>, unit: &str) -> String {
    match v {
        Some(x) if unit.is_empty() => x.to_string(),
        Some(x) => format!("{x} {unit}"),
        None => "-".into(),
    }
}

fn fmt_bytes(v: Option<u64>) -> String {
    match v {
        Some(b) if b >= 1_048_576 => format!("{:.1} MiB", b as f64 / 1_048_576.0),
        Some(b) if b >= 1024 => format!("{:.1} KiB", b as f64 / 1024.0),
        Some(b) => format!("{b} B"),
        None => "-".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::ParseStats;

    fn rec(category: &str, url: &str, outcome: FetchOutcome) -> FetchRecord {
        FetchRecord {
            category: category.into(),
            url: url.into(),
            http_version: "HTTP/2".into(),
            status: 200,
            tcp_connect_ms: Some(11),
            ttfb_ms: 100,
            total_ms: 200,
            body_bytes: 2048,
            outcome,
            error: None,
            parse: Some(ParseStats {
                nodes: 42,
                max_depth: 9,
                elements: 30,
                text_chars: 100,
                parse_us: 1500,
                nodes_per_sec: 28_000_000,
            }),
        }
    }

    #[test]
    fn bench_table_renders_all_rows() {
        let summaries = vec![CategorySummary {
            category: "news".into(),
            attempts: 2,
            ok: 1,
            http_error: 1,
            ttfb_p50_ms: Some(90),
            ttfb_p90_ms: Some(180),
            total_p50_ms: Some(150),
            body_bytes_p50: Some(2048),
            parse_ms_p50: Some(1),
            nodes_p50: Some(42),
            ..Default::default()
        }];
        let md = bench_markdown(&summaries);
        assert_eq!(md.matches('|').count() >= 9 * 2, true);
        assert!(md.contains("| news | 1/1/0/0 | 90 ms | 180 ms |"));
        assert!(md.contains("2.0 KiB"));
    }

    #[test]
    fn detail_table_labels_outcomes() {
        let recs = vec![rec("wiki", "https://w/", FetchOutcome::Ok)];
        let md = detail_markdown(&recs);
        assert!(md.contains("| ok |"));
        assert!(md.contains("https://w/"));
        assert!(md.contains("HTTP/2"));
    }

    #[test]
    fn stress_block_contains_key_numbers() {
        let s = StressSummary {
            rounds: 3,
            requests: 300,
            ok: 270,
            failed: 30,
            success_rate_pct: 90.0,
            ttfb_p50_ms: Some(120),
            ttfb_p90_ms: Some(400),
            ttfb_p99_ms: Some(900),
            total_p50_ms: Some(300),
            total_p99_ms: Some(1200),
            wall_seconds: 60.0,
            requests_per_sec: 5.0,
            rss_kib_p50: Some(20_000),
            rss_kib_max: Some(25_000),
            per_category: vec![],
        };
        let md = stress_markdown(&s);
        assert!(md.contains("90.0%"));
        assert!(md.contains("5.00 req/s"));
        assert!(md.contains("120 ms / 400 ms / 900 ms"));
        assert!(md.contains("20,000") || md.contains("20000"));
    }

    #[test]
    fn byte_formatting() {
        assert_eq!(fmt_bytes(Some(512)), "512 B");
        assert_eq!(fmt_bytes(Some(4096)), "4.0 KiB");
        assert_eq!(fmt_bytes(Some(2 * 1_048_576)), "2.0 MiB");
        assert_eq!(fmt_bytes(None), "-");
    }
}
