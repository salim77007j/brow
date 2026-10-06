/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Load stress test: repeated sequential rounds over the full 10 × 10 matrix,
//! with self-RSS sampling from /proc during the run.

use std::time::{Duration, Instant};

use crate::http::{current_rss_kib, fetch_and_parse, Fetcher};
use crate::metrics::{percentile, summarize, FetchRecord, StressSummary};
use crate::sites::CATEGORIES;

#[derive(Clone, Debug)]
pub struct StressConfig {
    pub rounds: u32,
    pub timeout: Duration,
    /// Pause between rounds so steady-state (post-warmup) behavior dominates.
    pub inter_round_pause: Duration,
}

impl Default for StressConfig {
    fn default() -> Self {
        StressConfig {
            rounds: 3,
            timeout: Duration::from_secs(20),
            inter_round_pause: Duration::from_millis(500),
        }
    }
}

/// Run the stress test. Sequential rounds (each URL once per round) — this
/// measures sustained behavior over the whole matrix, not concurrency.
pub async fn run_stress(cfg: StressConfig) -> (StressSummary, Vec<FetchRecord>) {
    let fetcher = Fetcher::new();
    let start = Instant::now();
    let mut records: Vec<FetchRecord> = Vec::new();
    let mut rss_samples: Vec<u64> = Vec::new();

    for round in 0..cfg.rounds {
        for category in CATEGORIES {
            for url in category.urls {
                let rec = fetch_and_parse(&fetcher.client, category.id, url, cfg.timeout, false).await;
                records.push(rec);
                if let Some(kib) = current_rss_kib() {
                    rss_samples.push(kib);
                }
            }
        }
        if round + 1 < cfg.rounds {
            tokio::time::sleep(cfg.inter_round_pause).await;
        }
    }

    let wall = start.elapsed();
    let ttfb: Vec<u64> = records.iter().map(|r| r.ttfb_ms).collect();
    let total: Vec<u64> = records.iter().map(|r| r.total_ms).collect();
    let ok = records.iter().filter(|r| r.outcome.is_success()).count();
    let requests = records.len() as u32;
    let wall_s = wall.as_secs_f64();

    let mut per_category = Vec::new();
    for category in CATEGORIES {
        let cat_records: Vec<FetchRecord> = records
            .iter()
            .filter(|r| r.category == category.id)
            .cloned()
            .collect();
        per_category.push(summarize(category.id, &cat_records));
    }

    let summary = StressSummary {
        rounds: cfg.rounds,
        requests,
        ok: ok as u32,
        failed: requests - ok as u32,
        success_rate_pct: if requests > 0 {
            100.0 * ok as f64 / requests as f64
        } else {
            0.0
        },
        ttfb_p50_ms: percentile(&ttfb, 50.0),
        ttfb_p90_ms: percentile(&ttfb, 90.0),
        ttfb_p99_ms: percentile(&ttfb, 99.0),
        total_p50_ms: percentile(&total, 50.0),
        total_p99_ms: percentile(&total, 99.0),
        wall_seconds: wall_s,
        requests_per_sec: if wall_s > 0.0 {
            f64::from(requests) / wall_s
        } else {
            0.0
        },
        rss_kib_p50: percentile(&rss_samples, 50.0),
        rss_kib_max: rss_samples.iter().copied().max(),
        per_category,
    };
    (summary, records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn stress_shape_on_unreachable_matrix() {
        // Hermetic: .invalid hosts fail fast; validates the aggregation math.
        let cfg = StressConfig {
            rounds: 1,
            timeout: Duration::from_millis(300),
            inter_round_pause: Duration::ZERO,
        };
        let (summary, records) = stress_with_invalid_hosts(cfg).await;
        assert_eq!(records.len(), 4);
        assert_eq!(summary.requests, 4);
        assert_eq!(summary.ok, 0);
        assert_eq!(summary.success_rate_pct, 0.0);
        assert!(summary.wall_seconds > 0.0);
        assert_eq!(summary.per_category.len(), 2, "c1 + c2");
    }

    /// Same as `run_stress` but against a 2-category × 2-url local matrix of
    /// unresolvable hosts (hermetic CI fast path).
    use crate::metrics::FetchOutcome;

    async fn stress_with_invalid_hosts(cfg: StressConfig) -> (StressSummary, Vec<FetchRecord>) {
        let fetcher = Fetcher::new();
        let start = Instant::now();
        let mut records = Vec::new();
        let mut rss_samples: Vec<u64> = Vec::new();
        for _ in 0..cfg.rounds {
            for (cat, url) in [
                ("c1", "https://stress-invalid-1.invalid/"),
                ("c1", "https://stress-invalid-2.invalid/"),
                ("c2", "https://stress-invalid-3.invalid/"),
                ("c2", "https://stress-invalid-4.invalid/"),
            ] {
                let rec = fetch_and_parse(&fetcher.client, cat, url, cfg.timeout, false).await;
                records.push(rec);
                if let Some(kib) = current_rss_kib() {
                    rss_samples.push(kib);
                }
            }
        }
        let wall = start.elapsed();
        let ttfb: Vec<u64> = records.iter().map(|r| r.ttfb_ms).collect();
        let total: Vec<u64> = records.iter().map(|r| r.total_ms).collect();
        let ok = records.iter().filter(|r| r.outcome.is_success()).count() as u32;
        let requests = records.len() as u32;
        let mut per_category = Vec::new();
        for cat in ["c1", "c2"] {
            let cat_records: Vec<FetchRecord> = records
                .iter()
                .filter(|r| r.category == cat)
                .cloned()
                .collect();
            per_category.push(summarize(cat, &cat_records));
        }
        let summary = StressSummary {
            rounds: cfg.rounds,
            requests,
            ok,
            failed: requests - ok,
            success_rate_pct: 100.0 * f64::from(ok) / f64::from(requests),
            ttfb_p50_ms: percentile(&ttfb, 50.0),
            ttfb_p90_ms: percentile(&ttfb, 90.0),
            ttfb_p99_ms: percentile(&ttfb, 99.0),
            total_p50_ms: percentile(&total, 50.0),
            total_p99_ms: percentile(&total, 99.0),
            wall_seconds: wall.as_secs_f64(),
            requests_per_sec: f64::from(requests) / wall.as_secs_f64(),
            rss_kib_p50: percentile(&rss_samples, 50.0),
            rss_kib_max: rss_samples.iter().copied().max(),
            per_category,
        };
        (summary, records)
    }

    #[test]
    fn outcomes_partition_completely() {
        // Every outcome is counted exactly once by the summary buckets.
        let recs: Vec<FetchRecord> = ["ok", "http_error", "timeout", "transport_error"]
            .iter()
            .map(|o| FetchRecord {
                category: "x".into(),
                url: "https://x/".into(),
                http_version: String::new(),
                status: 0,
                tcp_connect_ms: None,
                ttfb_ms: 1,
                total_ms: 1,
                body_bytes: 0,
                outcome: match *o {
                    "ok" => FetchOutcome::Ok,
                    "http_error" => FetchOutcome::HttpError,
                    "timeout" => FetchOutcome::Timeout,
                    _ => FetchOutcome::TransportError,
                },
                error: None,
                parse: None,
            })
            .collect();
        let s = summarize("x", &recs);
        assert_eq!(
            s.attempts as u32,
            s.ok + s.http_error + s.timeout + s.transport_error + s.parse_error
        );
    }
}
