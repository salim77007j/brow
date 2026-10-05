/* Memory budget tracking: self-RSS sampling from /proc + pressure derivation.
 *
 * Linux reads `/proc/self/status` (VmRSS) — no libc dependency, no sysinfo
 * crate, ~1 µs per sample. Other platforms report `None` and the tracker
 * degrades to tab-side accounting only.
 */

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::lifecycle::DiscardPolicy;

/// RSS + swap-backed resident memory of the current process, in bytes.
/// Parsed from `/proc/self/status` on Linux; `None` elsewhere.
pub fn current_process_rss_bytes() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmRSS:") {
                // e.g. "  123456 kB"
                let mut fields = rest.split_whitespace();
                let value_kb: u64 = fields.next()?.parse().ok()?;
                return Some(value_kb * 1024);
            }
        }
        None
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

/// A memory sample with a timestamp, for trend analysis.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MemorySample {
    pub rss_bytes: u64,
    /// Seconds since tracker start (monotonic).
    pub t_secs: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BudgetVerdict {
    WithinBudget,
    OverBudget { excess_bytes: u64 },
}

/// Watches the browser's RSS against the total budget and maintains a short
/// sample history so the UI dashboard can draw a trend line.
#[derive(Debug)]
pub struct MemoryBudgetTracker {
    pub policy: DiscardPolicy,
    started: Instant,
    history: Vec<MemorySample>,
    /// Keep at most this many samples (ring-buffer semantics).
    history_cap: usize,
    /// Minimum interval between accepted samples.
    sample_interval: Duration,
    last_sample_t: Option<Instant>,
    peak_rss: u64,
}

impl Default for MemoryBudgetTracker {
    fn default() -> Self {
        Self::new(DiscardPolicy::default(), 240, Duration::from_millis(500))
    }
}

impl MemoryBudgetTracker {
    pub fn new(policy: DiscardPolicy, history_cap: usize, sample_interval: Duration) -> Self {
        Self {
            policy,
            started: Instant::now(),
            history: Vec::with_capacity(history_cap.min(64)),
            history_cap: history_cap.max(8),
            sample_interval,
            last_sample_t: None,
            peak_rss: 0,
        }
    }

    /// Take a sample now (if the interval allows) and return the derived
    /// pressure. `rss_override` lets callers feed measured totals that include
    /// child processes.
    pub fn sample(
        &mut self,
        rss_override: Option<u64>,
    ) -> Option<crate::lifecycle::MemoryPressure> {
        let now = Instant::now();
        if let Some(last) = self.last_sample_t {
            if now.duration_since(last) < self.sample_interval {
                return None;
            }
        }
        let rss = rss_override.or_else(current_process_rss_bytes)?;
        self.last_sample_t = Some(now);
        self.peak_rss = self.peak_rss.max(rss);
        let t = now.duration_since(self.started).as_secs_f64();
        self.history.push(MemorySample { rss_bytes: rss, t_secs: t });
        if self.history.len() > self.history_cap {
            let over = self.history.len() - self.history_cap;
            self.history.drain(0..over);
        }
        Some(self.policy.pressure_for_rss(rss))
    }

    pub fn history(&self) -> &[MemorySample] {
        &self.history
    }

    pub fn peak_rss(&self) -> u64 {
        self.peak_rss
    }

    /// Latest sample, if any.
    pub fn latest(&self) -> Option<&MemorySample> {
        self.history.last()
    }

    /// Is the per-tab RSS target respected?
    pub fn check_tab_budget(&self, tab_rss_bytes: u64, per_tab_budget_bytes: u64) -> BudgetVerdict {
        if tab_rss_bytes <= per_tab_budget_bytes {
            BudgetVerdict::WithinBudget
        } else {
            BudgetVerdict::OverBudget {
                excess_bytes: tab_rss_bytes - per_tab_budget_bytes,
            }
        }
    }

    /// Fraction of the total budget currently consumed (0.0..=1.0+).
    pub fn budget_usage_fraction(&self, rss_bytes: u64) -> f64 {
        (rss_bytes as f64 / self.policy.total_memory_budget.max(1) as f64).clamp(0.0, 8.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_parsing_or_graceful_none() {
        // On Linux CI this returns a real number; elsewhere None. Either way
        // it must not panic.
        let rss = current_process_rss_bytes();
        if cfg!(target_os = "linux") {
            let rss = rss.expect("Linux must expose VmRSS");
            assert!(rss > 0);
            assert!(rss < 64 * 1024 * 1024 * 1024, "implausible RSS: {rss}");
        } else {
            assert!(rss.is_none());
        }
    }

    #[test]
    fn sampling_interval_and_history_cap() {
        let mut t = MemoryBudgetTracker::new(
            DiscardPolicy::default(),
            8,
            Duration::from_millis(0),
        );
        // Feed synthetic samples via override.
        let mut last = None;
        for i in 0..20u64 {
            let p = t.sample(Some(100 * (i + 1)));
            if i == 0 {
                assert!(p.is_some());
            }
            last = p;
        }
        assert_eq!(t.history().len(), 8, "history ring must cap at 8");
        assert_eq!(t.peak_rss(), 100 * 20);
        assert!(t.latest().is_some());
        let _ = last;
    }

    #[test]
    fn tab_budget_check() {
        let t = MemoryBudgetTracker::default();
        let budget = 100 * 1024 * 1024;
        assert_eq!(
            t.check_tab_budget(90 * 1024 * 1024, budget),
            BudgetVerdict::WithinBudget
        );
        assert_eq!(
            t.check_tab_budget(120 * 1024 * 1024, budget),
            BudgetVerdict::OverBudget { excess_bytes: 20 * 1024 * 1024 }
        );
    }

    #[test]
    fn usage_fraction_clamped() {
        let mut t = MemoryBudgetTracker::default();
        t.policy.total_memory_budget = 1000;
        assert!((t.budget_usage_fraction(500) - 0.5).abs() < 1e-9);
        assert_eq!(t.budget_usage_fraction(0), 0.0);
        assert!((t.budget_usage_fraction(4000) - 4.0).abs() < 1e-9);
    }
}
