/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Blocking statistics — per-host counters + category totals, lock-free
//! increments, persisted on exit in the same JSON style as phase 2/3 stores.

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

// brow (D-011): process-global mirrors of the cumulative counters. brow is a
// single-process embedder, so the product shell reads these lock-free (no
// embedder/IPC plumbing). They are seeded from the persisted snapshot with
// `fetch_max` (idempotent under multiple `PrivacyStats` instances) and
// incremented alongside the per-instance counters in the `record_*` methods.
static GLOBAL_NETWORK_BLOCKED: AtomicU64 = AtomicU64::new(0);
static GLOBAL_CNAME_FLAGGED: AtomicU64 = AtomicU64::new(0);
static GLOBAL_COOKIES_REJECTED: AtomicU64 = AtomicU64::new(0);
static GLOBAL_COOKIES_OMITTED: AtomicU64 = AtomicU64::new(0);
static GLOBAL_FINGERPRINT_PAYLOADS: AtomicU64 = AtomicU64::new(0);

/// A lock-free snapshot of the process-global cumulative privacy counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PrivacyTotals {
    pub network_blocked: u64,
    pub cname_flagged: u64,
    pub cookies_rejected: u64,
    pub cookies_omitted: u64,
    pub fingerprint_payloads: u64,
}

/// Read the process-global cumulative counters (lock-free snapshot).
pub fn global_totals() -> PrivacyTotals {
    PrivacyTotals {
        network_blocked: GLOBAL_NETWORK_BLOCKED.load(Ordering::Relaxed),
        cname_flagged: GLOBAL_CNAME_FLAGGED.load(Ordering::Relaxed),
        cookies_rejected: GLOBAL_COOKIES_REJECTED.load(Ordering::Relaxed),
        cookies_omitted: GLOBAL_COOKIES_OMITTED.load(Ordering::Relaxed),
        fingerprint_payloads: GLOBAL_FINGERPRINT_PAYLOADS.load(Ordering::Relaxed),
    }
}

#[derive(Debug, Default)]
pub struct PrivacyStats {
    network_blocked: AtomicU64,
    cname_flagged: AtomicU64,
    cookies_rejected: AtomicU64,
    cookies_omitted: AtomicU64,
    fingerprint_payloads: AtomicU64,
    per_host: Mutex<BTreeMap<String, u64>>,
    path: Option<PathBuf>,
}

impl PrivacyStats {
    pub fn new(path: Option<PathBuf>) -> PrivacyStats {
        // load previous totals if present
        let stats = PrivacyStats {
            network_blocked: AtomicU64::new(0),
            cname_flagged: AtomicU64::new(0),
            cookies_rejected: AtomicU64::new(0),
            cookies_omitted: AtomicU64::new(0),
            fingerprint_payloads: AtomicU64::new(0),
            per_host: Mutex::new(BTreeMap::new()),
            path,
        };
        if let Some(p) = &stats.path {
            if let Ok(bytes) = std::fs::read(p) {
                if let Ok(prev) = serde_json::from_slice::<StatsSnapshot>(&bytes) {
                    stats
                        .network_blocked
                        .store(prev.network_blocked, Ordering::Relaxed);
                    stats
                        .cname_flagged
                        .store(prev.cname_flagged, Ordering::Relaxed);
                    stats
                        .cookies_rejected
                        .store(prev.cookies_rejected, Ordering::Relaxed);
                    stats
                        .cookies_omitted
                        .store(prev.cookies_omitted, Ordering::Relaxed);
                    stats
                        .fingerprint_payloads
                        .store(prev.fingerprint_payloads, Ordering::Relaxed);
                    *stats.per_host.lock() = prev.per_host;
                    // Seed the process-global mirrors (D-011); fetch_max keeps
                    // this idempotent if several instances load the same file.
                    GLOBAL_NETWORK_BLOCKED.fetch_max(prev.network_blocked, Ordering::Relaxed);
                    GLOBAL_CNAME_FLAGGED.fetch_max(prev.cname_flagged, Ordering::Relaxed);
                    GLOBAL_COOKIES_REJECTED.fetch_max(prev.cookies_rejected, Ordering::Relaxed);
                    GLOBAL_COOKIES_OMITTED.fetch_max(prev.cookies_omitted, Ordering::Relaxed);
                    GLOBAL_FINGERPRINT_PAYLOADS
                        .fetch_max(prev.fingerprint_payloads, Ordering::Relaxed);
                }
            }
        }
        stats
    }

    pub fn record_block(&self, host: &str) {
        self.network_blocked.fetch_add(1, Ordering::Relaxed);
        GLOBAL_NETWORK_BLOCKED.fetch_add(1, Ordering::Relaxed);
        self.bump_host(host);
    }

    pub fn record_cname(&self, host: &str) {
        self.cname_flagged.fetch_add(1, Ordering::Relaxed);
        GLOBAL_CNAME_FLAGGED.fetch_add(1, Ordering::Relaxed);
        self.bump_host(host);
    }

    pub fn record_cookie_rejected(&self) {
        self.cookies_rejected.fetch_add(1, Ordering::Relaxed);
        GLOBAL_COOKIES_REJECTED.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_cookie_omitted(&self) {
        self.cookies_omitted.fetch_add(1, Ordering::Relaxed);
        GLOBAL_COOKIES_OMITTED.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_fingerprint_payload(&self) {
        self.fingerprint_payloads.fetch_add(1, Ordering::Relaxed);
        GLOBAL_FINGERPRINT_PAYLOADS.fetch_add(1, Ordering::Relaxed);
    }

    fn bump_host(&self, host: &str) {
        if host.is_empty() {
            return;
        }
        let mut map = self.per_host.lock();
        *map.entry(host.to_ascii_lowercase()).or_insert(0) += 1;
    }

    pub fn totals(&self) -> (u64, u64, u64, u64, u64) {
        (
            self.network_blocked.load(Ordering::Relaxed),
            self.cname_flagged.load(Ordering::Relaxed),
            self.cookies_rejected.load(Ordering::Relaxed),
            self.cookies_omitted.load(Ordering::Relaxed),
            self.fingerprint_payloads.load(Ordering::Relaxed),
        )
    }

    /// Top-N blocked hosts.
    pub fn top_hosts(&self, n: usize) -> Vec<(String, u64)> {
        let map = self.per_host.lock();
        let mut v: Vec<(String, u64)> = map.iter().map(|(k, c)| (k.clone(), *c)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v.truncate(n);
        v
    }

    pub fn persist(&self) {
        if let Some(p) = &self.path {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let snap = StatsSnapshot {
                network_blocked: self.network_blocked.load(Ordering::Relaxed),
                cname_flagged: self.cname_flagged.load(Ordering::Relaxed),
                cookies_rejected: self.cookies_rejected.load(Ordering::Relaxed),
                cookies_omitted: self.cookies_omitted.load(Ordering::Relaxed),
                fingerprint_payloads: self.fingerprint_payloads.load(Ordering::Relaxed),
                per_host: self.per_host.lock().clone(),
            };
            let tmp = p.with_extension("json.tmp");
            if serde_json::to_vec(&snap)
                .ok()
                .and_then(|bytes| std::fs::write(&tmp, bytes).ok())
                .is_some()
            {
                let _ = std::fs::rename(&tmp, p);
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StatsSnapshot {
    pub network_blocked: u64,
    pub cname_flagged: u64,
    pub cookies_rejected: u64,
    pub cookies_omitted: u64,
    pub fingerprint_payloads: u64,
    pub per_host: BTreeMap<String, u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_and_top_hosts() {
        let s = PrivacyStats::new(None);
        s.record_block("ads.example");
        s.record_block("ads.example");
        s.record_block("track.example");
        s.record_cname("cloak.example");
        assert_eq!(s.totals(), (3, 1, 0, 0, 0));
        let top = s.top_hosts(2);
        assert_eq!(top[0], ("ads.example".to_string(), 2));
        assert_eq!(top.len(), 2);
    }

    #[test]
    fn persists_and_reloads() {
        let dir = std::env::temp_dir();
        let p = dir.join(format!("brow-priv-stats-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&p);
        {
            let s = PrivacyStats::new(Some(p.clone()));
            s.record_block("a.example");
            s.persist();
        }
        let s2 = PrivacyStats::new(Some(p.clone()));
        assert_eq!(s2.totals().0, 1);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn global_totals_mirror_instance_increments() {
        // D-011: the global mirrors must never lag an instance's cumulative
        // count (monotonic assertion — other tests in this process also bump
        // the globals, so exact values would be flaky).
        let before = global_totals().network_blocked;
        let s = PrivacyStats::new(None);
        s.record_block("mirror.example");
        s.record_block("mirror.example");
        let after = global_totals().network_blocked;
        assert!(after >= before + 2, "global mirror must count increments");
    }
}
