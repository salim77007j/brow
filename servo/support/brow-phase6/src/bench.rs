/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The 10-category page benchmark: fetch + parse every URL in the matrix.

use std::time::Duration;

use tokio::sync::Semaphore;

use crate::http::{fetch_and_parse, Fetcher};
use crate::metrics::{summarize, CategorySummary, FetchRecord};
use crate::sites::CATEGORIES;

/// Configuration for one benchmark run.
#[derive(Clone, Debug)]
pub struct BenchConfig {
    /// Per-request timeout (connect + headers + body).
    pub timeout: Duration,
    /// Maximum concurrent in-flight requests.
    pub concurrency: usize,
    /// Whether to run the separate TCP connect probe per URL.
    pub with_connect_probe: bool,
    /// Restrict to one category (None = all ten).
    pub only_category: Option<String>,
}

impl Default for BenchConfig {
    fn default() -> Self {
        BenchConfig {
            timeout: Duration::from_secs(20),
            concurrency: 4,
            with_connect_probe: true,
            only_category: None,
        }
    }
}

/// Run the page benchmark. Returns per-category summaries and all raw records.
pub async fn run_bench(
    cfg: BenchConfig,
) -> (Vec<CategorySummary>, Vec<FetchRecord>) {
    let fetcher = Fetcher::new();
    let semaphore = std::sync::Arc::new(Semaphore::new(cfg.concurrency));
    let mut handles = Vec::new();

    for category in CATEGORIES {
        if let Some(only) = &cfg.only_category {
            if only != category.id {
                continue;
            }
        }
        for url in category.urls {
            let permit = semaphore.clone().acquire_owned().await.ok();
            let client = fetcher.client.clone();
            let cat = category.id.to_string();
            let timeout = cfg.timeout;
            let probe = cfg.with_connect_probe;
            handles.push(tokio::spawn(async move {
                let rec = fetch_and_parse(&client, &cat, url, timeout, probe).await;
                drop(permit);
                rec
            }));
        }
    }

    let mut records: Vec<FetchRecord> = Vec::with_capacity(handles.len());
    for h in handles {
        if let Ok(r) = h.await {
            records.push(r);
        }
    }

    let mut summaries = Vec::new();
    for category in CATEGORIES {
        if let Some(only) = &cfg.only_category {
            if only != category.id {
                continue;
            }
        }
        let cat_records: Vec<FetchRecord> = records
            .iter()
            .filter(|r| r.category == category.id)
            .cloned()
            .collect();
        summaries.push(summarize(category.id, &cat_records));
    }
    (summaries, records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bench_respects_category_filter_shape() {
        // Hermetic: hits only an unresolvable .invalid host — exercises the
        // aggregation path without network dependence.
        let cfg = BenchConfig {
            timeout: Duration::from_millis(500),
            concurrency: 2,
            with_connect_probe: false,
            only_category: Some("forums".into()),
        };
        let (summaries, records) = run_bench(cfg).await;
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].category, "forums");
        assert_eq!(records.len(), 10);
        assert!(records.iter().all(|r| r.category == "forums"));
    }
}
