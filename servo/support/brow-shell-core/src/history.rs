/* History store: merged visits with frecency-ranked search. */

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::bookmarks::write_atomic;
use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Normalized URL (the store's merge key).
    pub url: String,
    pub title: String,
    pub visit_count: u32,
    pub first_visit_unix: u64,
    pub last_visit_unix: u64,
}

impl HistoryEntry {
    /// Frecency score: visit frequency decaying with staleness.
    /// score = visits / (1 + age_days / half_life_days)
    pub fn frecency(&self, now_unix: u64, half_life_days: f64) -> f64 {
        let age_secs = now_unix.saturating_sub(self.last_visit_unix) as f64;
        let age_days = age_secs / 86_400.0;
        let decay = 1.0 + age_days / half_life_days.max(0.01);
        self.visit_count as f64 / decay
    }
}

/// Browser history. Entries merge per URL; search ranks by frecency.
#[derive(Debug, Default)]
pub struct HistoryStore {
    entries: Vec<HistoryEntry>,
    max_entries: usize,
    path: Option<PathBuf>,
}

use std::path::PathBuf;

impl HistoryStore {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::new(),
            max_entries: max_entries.max(1),
            path: None,
        }
    }

    pub fn open(path: &Path, max_entries: usize) -> Result<Self, StoreError> {
        let entries: Vec<HistoryEntry> = if path.exists() {
            serde_json::from_str(&std::fs::read_to_string(path)?)?
        } else {
            Vec::new()
        };
        Ok(Self {
            entries,
            max_entries: max_entries.max(1),
            path: Some(path.to_path_buf()),
        })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    fn now_unix() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    /// Record a visit. Re-visits of the same URL merge and bump the count.
    pub fn record_visit(&mut self, url: &str, title: &str, at_unix: u64) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.url == url) {
            e.visit_count += 1;
            e.last_visit_unix = at_unix;
            if !title.is_empty() {
                e.title = title.to_string();
            }
        } else {
            self.entries.push(HistoryEntry {
                url: url.to_string(),
                title: title.to_string(),
                visit_count: 1,
                first_visit_unix: at_unix,
                last_visit_unix: at_unix,
            });
        }
        self.trim_and_persist();
    }

    pub fn record_visit_now(&mut self, url: &str, title: &str) {
        self.record_visit(url, title, Self::now_unix());
    }

    /// Frecency-ranked search over URL and title.
    pub fn search(&self, query: &str, limit: usize) -> Vec<&HistoryEntry> {
        let q = query.trim().to_lowercase();
        let now = Self::now_unix();
        let mut scored: Vec<(f64, &HistoryEntry)> = self
            .entries
            .iter()
            .filter(|e| q.is_empty() || e.url.to_lowercase().contains(&q) || e.title.to_lowercase().contains(&q))
            .map(|e| (e.frecency(now, 14.0), e))
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored.into_iter().map(|(_, e)| e).take(limit).collect()
    }

    /// Most recent visits (newest first), for the history panel.
    pub fn recent(&self, limit: usize) -> Vec<&HistoryEntry> {
        let mut sorted: Vec<&HistoryEntry> = self.entries.iter().collect();
        sorted.sort_by_key(|e| std::cmp::Reverse(e.last_visit_unix));
        sorted.into_iter().take(limit).collect()
    }

    pub fn remove_url(&mut self, url: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.url != url);
        let removed = self.entries.len() != before;
        if removed {
            self.persist();
        }
        removed
    }

    /// Forget everything visited before `before_unix` (inclusive).
    pub fn clear_before(&mut self, before_unix: u64) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| e.last_visit_unix > before_unix);
        let removed = before - self.entries.len();
        if removed > 0 {
            self.persist();
        }
        removed
    }

    pub fn clear_all(&mut self) {
        self.entries.clear();
        self.persist();
    }

    fn trim_and_persist(&mut self) {
        if self.entries.len() > self.max_entries {
            // Evict the stalest entries first.
            self.entries
                .sort_by_key(|e| std::cmp::Reverse(e.last_visit_unix));
            self.entries.truncate(self.max_entries);
        }
        self.persist();
    }

    fn persist(&self) {
        if let Some(path) = &self.path {
            if let Ok(json) = serde_json::to_string_pretty(&self.entries) {
                if let Err(e) = write_atomic(path, json.as_bytes()) {
                    log::warn!("history persist failed: {e}");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> HistoryStore {
        HistoryStore::new(100)
    }

    #[test]
    fn visits_merge_per_url() {
        let mut h = store();
        h.record_visit("https://a.example/", "A", 1000);
        h.record_visit("https://a.example/", "A — updated", 2000);
        h.record_visit("https://b.example/", "B", 2500);
        assert_eq!(h.len(), 2);
        let a = h.entries().iter().find(|e| e.url.starts_with("https://a")).unwrap();
        assert_eq!(a.visit_count, 2);
        assert_eq!(a.title, "A — updated");
        assert_eq!(a.first_visit_unix, 1000);
        assert_eq!(a.last_visit_unix, 2000);
    }

    #[test]
    fn search_ranks_by_frecency() {
        let mut h = store();
        // Frequently visited but old.
        for day in 0..5u64 {
            h.record_visit("https://docs.example/", "Docs", 1000 + day * 86_400);
        }
        // Visited once but just now.
        h.record_visit("https://news.example/", "News", 5_000_000);

        let hits = h.search("example", 10);
        assert_eq!(hits.len(), 2);
        // Docs has 5 visits; even with decay it outranks a single old-ish visit
        // only if decay is gentle — assert docs first (5 visits / small decay).
        assert_eq!(hits[0].url, "https://docs.example/");
    }

    #[test]
    fn recent_orders_newest_first() {
        let mut h = store();
        h.record_visit("https://old.example/", "O", 100);
        h.record_visit("https://new.example/", "N", 900);
        let rec = h.recent(10);
        assert_eq!(rec[0].url, "https://new.example/");
    }

    #[test]
    fn trim_evicts_stale() {
        let mut h = HistoryStore::new(3);
        h.record_visit("https://1.example/", "1", 100);
        h.record_visit("https://2.example/", "2", 200);
        h.record_visit("https://3.example/", "3", 300);
        h.record_visit("https://4.example/", "4", 400);
        assert_eq!(h.len(), 3);
        assert!(h.entries().iter().all(|e| !e.url.contains("1.example")));
    }

    #[test]
    fn clear_before_and_remove_url() {
        let mut h = store();
        h.record_visit("https://a.example/", "A", 1000);
        h.record_visit("https://b.example/", "B", 5000);
        assert_eq!(h.clear_before(2000), 1);
        assert_eq!(h.len(), 1);
        assert!(h.remove_url("https://b.example/"));
        assert!(h.is_empty());
    }

    #[test]
    fn persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        {
            let mut h = HistoryStore::open(&path, 100).unwrap();
            h.record_visit("https://keep.example/", "K", 1234);
        }
        let h = HistoryStore::open(&path, 100).unwrap();
        assert_eq!(h.len(), 1);
        assert_eq!(h.entries()[0].visit_count, 1);
    }
}
