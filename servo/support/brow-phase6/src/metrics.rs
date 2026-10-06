/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Metric types, percentile math and aggregation for the Phase 6 harness.

use serde::{Deserialize, Serialize};

/// Result of one fetch + parse pipeline run against one URL.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchRecord {
    pub category: String,
    pub url: String,
    /// HTTP version negotiated ("HTTP/1.1", "HTTP/2", …), empty on transport error.
    pub http_version: String,
    /// HTTP status code (0 = transport-level failure before any response).
    pub status: u16,
    /// Separate raw TCP connect probe to host:443 (own socket; informative,
    /// labelled as probe in reports — not part of the request timings).
    pub tcp_connect_ms: Option<u64>,
    /// Request issued → response head received.
    pub ttfb_ms: u64,
    /// Request issued → full body received.
    pub total_ms: u64,
    /// Body bytes received (after content encoding, i.e. on-wire size).
    pub body_bytes: u64,
    /// Outcome classification.
    pub outcome: FetchOutcome,
    /// Human-readable transport/TLS error detail (None on success).
    pub error: Option<String>,
    /// HTML5 parse statistics (present when a parseable text/html body was received).
    pub parse: Option<ParseStats>,
}

/// Coarse classification of a fetch result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FetchOutcome {
    /// 2xx and parsed.
    Ok,
    /// Got a response but it was a redirect, error, or non-HTML body.
    HttpError,
    /// Timed out.
    Timeout,
    /// TLS/DNS/connection failure.
    TransportError,
    /// Body received but not parseable as HTML (empty, binary…).
    ParseError,
}

impl FetchOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, FetchOutcome::Ok)
    }
}

/// html5ever parse statistics for one document.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ParseStats {
    /// Total nodes produced by tree construction.
    pub nodes: u64,
    /// Maximum tree depth.
    pub max_depth: u32,
    /// Elements seen.
    pub elements: u64,
    /// Text characters accumulated.
    pub text_chars: u64,
    /// Parse wall time (µs).
    pub parse_us: u64,
    /// Parse throughput (nodes / second).
    pub nodes_per_sec: u64,
}

/// Aggregated statistics for one category (or the whole run).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CategorySummary {
    pub category: String,
    pub attempts: u32,
    pub ok: u32,
    pub http_error: u32,
    pub timeout: u32,
    pub transport_error: u32,
    pub parse_error: u32,
    pub ttfb_p50_ms: Option<u64>,
    pub ttfb_p90_ms: Option<u64>,
    pub total_p50_ms: Option<u64>,
    pub total_p90_ms: Option<u64>,
    pub body_bytes_p50: Option<u64>,
    pub parse_ms_p50: Option<u64>,
    pub nodes_p50: Option<u64>,
}

/// Stress-test summary: repeated rounds over the whole matrix.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StressSummary {
    pub rounds: u32,
    pub requests: u32,
    pub ok: u32,
    pub failed: u32,
    pub success_rate_pct: f64,
    pub ttfb_p50_ms: Option<u64>,
    pub ttfb_p90_ms: Option<u64>,
    pub ttfb_p99_ms: Option<u64>,
    pub total_p50_ms: Option<u64>,
    pub total_p99_ms: Option<u64>,
    /// Wall time of the whole stress run.
    pub wall_seconds: f64,
    /// Requests completed per second (wall-clock).
    pub requests_per_sec: f64,
    /// Self process RSS samples (KiB) taken during the run.
    pub rss_kib_p50: Option<u64>,
    pub rss_kib_max: Option<u64>,
    /// Per-category breakdown.
    pub per_category: Vec<CategorySummary>,
}

/// Nearest-rank percentile of a sample list (input need not be sorted;
/// the input slice is not modified).
pub fn percentile(samples: &[u64], p: f64) -> Option<u64> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted: Vec<u64> = samples.to_vec();
    sorted.sort_unstable();
    let rank = ((p / 100.0) * (sorted.len() as f64)).ceil() as usize;
    let idx = rank.clamp(1, sorted.len()) - 1;
    Some(sorted[idx])
}

/// Aggregate a list of records into a summary for one bucket.
pub fn summarize(category: &str, records: &[FetchRecord]) -> CategorySummary {
    let mut s = CategorySummary {
        category: category.to_string(),
        attempts: records.len() as u32,
        ..Default::default()
    };
    let mut ttfb: Vec<u64> = Vec::new();
    let mut total: Vec<u64> = Vec::new();
    let mut bytes: Vec<u64> = Vec::new();
    let mut parse_ms: Vec<u64> = Vec::new();
    let mut nodes: Vec<u64> = Vec::new();
    for r in records {
        match r.outcome {
            FetchOutcome::Ok => s.ok += 1,
            FetchOutcome::HttpError => s.http_error += 1,
            FetchOutcome::Timeout => s.timeout += 1,
            FetchOutcome::TransportError => s.transport_error += 1,
            FetchOutcome::ParseError => s.parse_error += 1,
        }
        ttfb.push(r.ttfb_ms);
        total.push(r.total_ms);
        bytes.push(r.body_bytes);
        if let Some(p) = &r.parse {
            parse_ms.push(p.parse_us / 1000);
            nodes.push(p.nodes);
        }
    }
    s.ttfb_p50_ms = percentile(&ttfb, 50.0);
    s.ttfb_p90_ms = percentile(&ttfb, 90.0);
    s.total_p50_ms = percentile(&total, 50.0);
    s.total_p90_ms = percentile(&total, 90.0);
    s.body_bytes_p50 = percentile(&bytes, 50.0);
    s.parse_ms_p50 = percentile(&parse_ms, 50.0);
    s.nodes_p50 = percentile(&nodes, 50.0);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_nearest_rank() {
        let v: Vec<u64> = (1..=100).collect();
        assert_eq!(percentile(&v, 50.0), Some(50));
        assert_eq!(percentile(&v, 90.0), Some(90));
        assert_eq!(percentile(&v, 99.0), Some(99));
        assert_eq!(percentile(&v, 100.0), Some(100));
        assert_eq!(percentile(&v, 0.1), Some(1));
        assert_eq!(percentile(&[], 50.0), None);
    }

    #[test]
    fn percentile_small_input() {
        assert_eq!(percentile(&[7], 50.0), Some(7));
        assert_eq!(percentile(&[3, 1, 2], 50.0), Some(2));
        assert_eq!(percentile(&[3, 1, 2], 90.0), Some(3));
    }

    #[test]
    fn summarize_buckets_correctly() {
        let mk = |status: u16, ttfb: u64, out: FetchOutcome| FetchRecord {
            category: "t".into(),
            url: "https://x/".into(),
            http_version: "HTTP/2".into(),
            status,
            tcp_connect_ms: Some(10),
            ttfb_ms: ttfb,
            total_ms: ttfb * 2,
            body_bytes: 100,
            outcome: out,
            error: None,
            parse: None,
        };
        let recs = vec![
            mk(200, 100, FetchOutcome::Ok),
            mk(403, 200, FetchOutcome::HttpError),
            mk(0, 1500, FetchOutcome::Timeout),
            mk(200, 300, FetchOutcome::Ok),
        ];
        let s = summarize("t", &recs);
        assert_eq!((s.attempts, s.ok, s.http_error, s.timeout), (4, 2, 1, 1));
        assert_eq!(s.ttfb_p50_ms, Some(200));
        assert_eq!(s.ttfb_p90_ms, Some(1500));
    }

    #[test]
    fn outcome_serializes_snake_case() {
        let j = serde_json::to_string(&FetchOutcome::TransportError).unwrap();
        assert_eq!(j, "\"transport_error\"");
    }
}
