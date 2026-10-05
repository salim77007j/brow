/* Tab lifecycle policies: sleeping, discarding, memory-pressure response.
 *
 * brow's resource strategy (Phase 3 target: <100 MB per tab, near-zero idle
 * CPU) rests on three tiers of tab state:
 *
 *   Active      — the webview the user is interacting with. Full resources.
 *   Background  — visible in the tab strip but not focused. Timers clamped,
 *                 animation ticks stopped (engine `set_throttled(true)`),
 *                 background frame cap applies (engine-side 1 FPS limiter).
 *   Sleeping    — moved off the hot path: the webview is throttled hard by
 *                 the shell and its render pipeline is considered idle.
 *                 Restoration is instant (state is still resident).
 *   Discarded   — the webview object is destroyed; only a small
 *                 `DiscardedTabPayload` (URL, title, scroll offset, zoom,
 *                 history index) survives. Memory cost drops to ~KBs.
 *                 Re-activation reloads from the payload (what Chrome calls
 *                 "tab discard").
 *
 * The policies here are pure functions over tab snapshots so they are
 * trivially unit-testable and can be driven from any event source
 * (idle timers, RSS watchers, user action).
 */

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::tabs::{Tab, TabId, TabState};

/// Memory pressure levels derived from RSS versus budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoryPressure {
    /// RSS comfortably below budget. No action.
    Normal,
    /// RSS above the warning threshold. Sleep idle background tabs.
    Warning,
    /// RSS at/above the critical threshold. Sleep everything eligible, then
    /// discard the least-recently-used background tabs until back under.
    Critical,
}

/// Policy controlling when background tabs go to sleep.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepPolicy {
    /// Background tabs idle longer than this are put to sleep.
    pub idle_timeout: Duration,
    /// Pinned tabs never sleep automatically.
    pub exempt_pinned: bool,
    /// Tabs known to play audio never sleep automatically.
    pub exempt_audible: bool,
    /// Always keep at least this many tabs fully alive (never auto-sleep
    /// them), so the browser does not feel "cold".
    pub min_alive_tabs: usize,
}

impl Default for SleepPolicy {
    fn default() -> Self {
        // 10 minutes idle → sleep. Deliberately aggressive: sleeping is
        // instant to resume, and the win is a flat memory floor.
        Self {
            idle_timeout: Duration::from_secs(10 * 60),
            exempt_pinned: true,
            exempt_audible: true,
            min_alive_tabs: 2,
        }
    }
}

/// Policy controlling discarding under memory pressure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscardPolicy {
    /// More alive tabs than this → discard LRU background tabs proactively.
    pub max_alive_tabs: usize,
    /// Total memory budget across tabs; the tracker maps RSS/this ratio to
    /// [`MemoryPressure`].
    pub total_memory_budget: u64,
    /// Fraction of the budget at which [`MemoryPressure::Warning`] fires.
    pub warning_ratio: f64,
    /// Fraction of the budget at which [`MemoryPressure::Critical`] fires.
    pub critical_ratio: f64,
}

impl Default for DiscardPolicy {
    fn default() -> Self {
        Self {
            max_alive_tabs: 12,
            // 2 GiB total budget by default; per-tab target remains <100 MB.
            total_memory_budget: 2 * 1024 * 1024 * 1024,
            warning_ratio: 0.80,
            critical_ratio: 0.95,
        }
    }
}

impl DiscardPolicy {
    /// Map an RSS sample to a pressure level using this policy.
    pub fn pressure_for_rss(&self, rss_bytes: u64) -> MemoryPressure {
        let budget = self.total_memory_budget.max(1);
        let ratio = rss_bytes as f64 / budget as f64;
        if ratio >= self.critical_ratio {
            MemoryPressure::Critical
        } else if ratio >= self.warning_ratio {
            MemoryPressure::Warning
        } else {
            MemoryPressure::Normal
        }
    }
}

/// Select tabs that should be put to sleep right now.
///
/// Eligible: `Background` (not Active/Sleeping/Discarded), idle longer than
/// the policy timeout, not exempt, and not needed to satisfy
/// `min_alive_tabs` (most recently used alive tabs are kept).
pub fn select_tabs_to_sleep(
    tabs: &[Tab],
    policy: &SleepPolicy,
    now: Instant,
) -> Vec<TabId> {
    let alive: Vec<&Tab> = tabs
        .iter()
        .filter(|t| matches!(t.state, TabState::Active | TabState::Background))
        .collect();

    // Slots that must stay alive: the min_alive_tabs most recently activated
    // alive tabs (MRU slice). Everyone else is a sleep candidate.
    let mut keep = alive.clone();
    keep.sort_by_key(|t| std::cmp::Reverse(t.last_activated));
    let keep_count = policy.min_alive_tabs.max(1);
    let keep_ids: Vec<TabId> = keep.iter().take(keep_count).map(|t| t.id).collect();

    tabs.iter()
        .filter(|t| matches!(t.state, TabState::Background))
        .filter(|t| {
            now.duration_since(t.last_activity) >= policy.idle_timeout
                && !(policy.exempt_pinned && t.pinned)
                && !(policy.exempt_audible && t.audible)
        })
        .filter(|t| !keep_ids.contains(&t.id))
        .map(|t| t.id)
        .collect()
}

/// Select tabs to discard right now, in the order they should be discarded
/// (LRU background first). Never selects Active or pinned tabs.
pub fn select_tabs_to_discard(
    tabs: &[Tab],
    policy: &DiscardPolicy,
    pressure: MemoryPressure,
) -> Vec<TabId> {
    if pressure == MemoryPressure::Normal {
        return Vec::new();
    }

    let mut alive_background: Vec<&Tab> = tabs
        .iter()
        .filter(|t| matches!(t.state, TabState::Background | TabState::Sleeping))
        .filter(|t| !t.pinned)
        .collect();
    // LRU by last activation, then by id for determinism.
    alive_background.sort_by_key(|t| (t.last_activated, t.id));

    let alive_count = tabs
        .iter()
        .filter(|t| matches!(t.state, TabState::Active | TabState::Background | TabState::Sleeping))
        .count();

    let mut victims = Vec::new();
    match pressure {
        MemoryPressure::Normal => {}
        MemoryPressure::Warning => {
            // Proactively keep the alive set bounded.
            let mut over = alive_count.saturating_sub(policy.max_alive_tabs);
            for t in &alive_background {
                if over == 0 {
                    break;
                }
                victims.push(t.id);
                over -= 1;
            }
        }
        MemoryPressure::Critical => {
            // Discard everything except the active tab (and pinned), LRU first.
            for t in &alive_background {
                victims.push(t.id);
            }
        }
    }
    victims
}

/// How much memory a discarded tab costs (bytes, approximate). The payload is
/// a handful of strings + numbers; measured at ~2 KiB on 64-bit.
pub const DISCARDED_TAB_MEMORY_ESTIMATE: u64 = 2 * 1024;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tabs::{Tab, TabState};
    use std::time::Instant;

    fn tab(id: u64, state: TabState, idle_secs: u64) -> Tab {
        let now = Instant::now();
        Tab::new(
            TabId(id),
            format!("https://example.com/{id}").parse().unwrap(),
            now,
        )
        .with_state_for_test(state)
        .with_last_activity_for_test(now - Duration::from_secs(idle_secs))
        .with_last_activated_for_test(now - Duration::from_secs(idle_secs))
    }

    #[test]
    fn idle_background_tabs_sleep() {
        let tabs = vec![tab(1, TabState::Background, 700), tab(2, TabState::Active, 0)];
        let policy = SleepPolicy {
            idle_timeout: Duration::from_secs(600),
            min_alive_tabs: 1,
            ..Default::default()
        };
        let victims = select_tabs_to_sleep(&tabs, &policy, Instant::now());
        assert_eq!(victims, vec![TabId(1)]);
    }

    #[test]
    fn fresh_tabs_do_not_sleep() {
        let tabs = vec![tab(1, TabState::Background, 30)];
        let policy = SleepPolicy::default();
        assert!(select_tabs_to_sleep(&tabs, &policy, Instant::now()).is_empty());
    }

    #[test]
    fn pinned_and_audible_are_exempt() {
        let mut pinned = tab(1, TabState::Background, 700);
        pinned.pinned = true;
        let mut audible = tab(2, TabState::Background, 700);
        audible.audible = true;
        let policy = SleepPolicy::default();
        let victims = select_tabs_to_sleep(&[pinned, audible], &policy, Instant::now());
        assert!(victims.is_empty());
    }

    #[test]
    fn min_alive_tabs_protects_mru() {
        let mut tabs = vec![
            tab(1, TabState::Active, 0),
            tab(2, TabState::Background, 900),
            tab(3, TabState::Background, 800),
        ];
        let policy = SleepPolicy { idle_timeout: Duration::from_secs(600), min_alive_tabs: 3, ..Default::default() };
        // min_alive_tabs=3 and only 3 alive tabs → nobody sleeps.
        tabs[1].pinned = false;
        assert!(select_tabs_to_sleep(&tabs, &policy, Instant::now()).is_empty());
        let policy2 = SleepPolicy { idle_timeout: Duration::from_secs(600), min_alive_tabs: 2, ..Default::default() };
        let victims = select_tabs_to_sleep(&tabs, &policy2, Instant::now());
        assert_eq!(victims.len(), 1);
    }

    #[test]
    fn pressure_mapping() {
        let policy = DiscardPolicy::default();
        assert_eq!(policy.pressure_for_rss(1024), MemoryPressure::Normal);
        assert_eq!(
            policy.pressure_for_rss((policy.total_memory_budget as f64 * 0.85) as u64),
            MemoryPressure::Warning
        );
        assert_eq!(
            policy.pressure_for_rss(policy.total_memory_budget),
            MemoryPressure::Critical
        );
    }

    #[test]
    fn warning_discards_only_over_limit() {
        let tabs = vec![
            tab(1, TabState::Active, 0),
            tab(2, TabState::Background, 5),
            tab(3, TabState::Background, 10),
            tab(4, TabState::Background, 20),
        ];
        let policy = DiscardPolicy { max_alive_tabs: 3, ..Default::default() };
        let victims = select_tabs_to_discard(&tabs, &policy, MemoryPressure::Warning);
        // alive = 4 > max 3 → discard exactly 1, the LRU background tab.
        assert_eq!(victims, vec![TabId(4)]);
    }

    #[test]
    fn critical_discards_all_background() {
        let mut tabs = vec![
            tab(1, TabState::Active, 0),
            tab(2, TabState::Sleeping, 50),
            tab(3, TabState::Background, 10),
        ];
        tabs[1].pinned = false;
        let policy = DiscardPolicy::default();
        let victims = select_tabs_to_discard(&tabs, &policy, MemoryPressure::Critical);
        assert_eq!(victims.len(), 2);
        assert!(!victims.contains(&TabId(1))); // active survives
    }

    #[test]
    fn normal_pressure_never_discards() {
        let tabs = vec![tab(1, TabState::Background, 999)];
        let policy = DiscardPolicy::default();
        assert!(select_tabs_to_discard(&tabs, &policy, MemoryPressure::Normal).is_empty());
    }
}
