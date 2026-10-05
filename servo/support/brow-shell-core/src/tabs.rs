/* Tab model + manager: the chrome-side source of truth for open tabs.
 *
 * The manager never touches the engine. Every mutating operation returns the
 * list of [`TabEvent`]s the shell must replay against real WebViews
 * (show/hide/throttle/destroy/reload). This inversion keeps the entire
 * lifecycle testable without a compositor.
 */

use std::time::Instant;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::lifecycle::{
    select_tabs_to_discard, select_tabs_to_sleep, DiscardPolicy, MemoryPressure, SleepPolicy,
    DISCARDED_TAB_MEMORY_ESTIMATE,
};

/// Identifier of a tab within a browser session. `u64` counter, never reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TabId(pub u64);

/// Lifecycle state of a tab; see `lifecycle.rs` for the resource model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TabState {
    Active,
    Background,
    Sleeping,
    Discarded,
}

/// State preserved when a tab is discarded so it can be restored exactly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscardedTabPayload {
    pub url: Url,
    pub title: String,
    /// Position within the tab's session history to restore to.
    pub history_index: usize,
    /// Scroll offset in CSS pixels.
    pub scroll_y: f64,
    /// Page zoom factor (1.0 = 100%).
    pub zoom: f32,
    pub discarded_at: u64,
}

/// A single tab. Memory footprint is intentionally tiny.
/// Not persisted directly: sessions are rebuilt from URLs + payloads.
#[derive(Debug, Clone)]
pub struct Tab {
    pub id: TabId,
    pub url: Url,
    pub title: String,
    pub favicon_url: Option<Url>,
    pub state: TabState,
    pub pinned: bool,
    pub audible: bool,
    /// Milliseconds of CPU this tab consumed, as reported by the shell.
    pub cpu_millis: u64,
    /// RSS bytes attributed to this tab's webview, as measured by the shell.
    pub rss_bytes: u64,
    pub last_activity: Instant,
    pub last_activated: Instant,
    /// Back/forward session history (URLs only, capped).
    pub history: Vec<Url>,
    pub history_index: usize,
    pub scroll_y: f64,
    pub zoom: f32,
    pub discarded: Option<DiscardedTabPayload>,
}

impl Tab {
    pub fn new(id: TabId, url: Url, now: Instant) -> Self {
        Self {
            id,
            url,
            title: String::new(),
            favicon_url: None,
            state: TabState::Background,
            pinned: false,
            audible: false,
            cpu_millis: 0,
            rss_bytes: 0,
            last_activity: now,
            last_activated: now,
            history: Vec::new(),
            history_index: 0,
            scroll_y: 0.0,
            zoom: 1.0,
            discarded: None,
        }
    }

    /// Estimated resident cost of this tab right now.
    pub fn estimated_memory(&self) -> u64 {
        match self.state {
            TabState::Discarded => DISCARDED_TAB_MEMORY_ESTIMATE,
            // Sleeping tabs keep their engine state but the compositor frame
            // buffers are freed by the shell on sleep; measured overhead is
            // applied by the shell, this is the bookkeeping estimate.
            TabState::Sleeping => self.rss_bytes * 3 / 5,
            TabState::Active | TabState::Background => self.rss_bytes,
        }
    }

    // ---- test-only builders (kept private to the crate) -------------------
    pub fn with_state_for_test(mut self, state: TabState) -> Self {
        self.state = state;
        self
    }

    pub fn with_last_activity_for_test(mut self, at: Instant) -> Self {
        self.last_activity = at;
        self
    }

    pub fn with_last_activated_for_test(mut self, at: Instant) -> Self {
        self.last_activated = at;
        self
    }
}

/// Events the shell must replay against real WebViews.
#[derive(Debug, Clone, PartialEq)]
pub enum TabEvent {
    Created(TabId),
    Activated(TabId),
    /// Tab must be visually hidden and engine-throttled.
    Backgrounded(TabId),
    /// Tab entered the sleeping tier (already backgrounded).
    Slept(TabId),
    Woken(TabId),
    /// Tab must be destroyed; `payload` describes how to recreate it later.
    Discarded(TabId, DiscardedTabPayload),
    /// Tab must be recreated from the payload.
    Restored(TabId, Url, f64, f32),
    Closed(TabId),
    TitleChanged(TabId, String),
    UrlChanged(TabId, Url),
    /// Activation request was rejected (e.g. tab already active).
    Noop(TabId),
}

/// Errors surfaced by the manager.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TabError {
    #[error("unknown tab {0:?}")]
    Unknown(TabId),
    #[error("tab {0:?} is discarded and must be restored first")]
    Discarded(TabId),
}

/// Owns the tab list and drives lifecycle decisions.
#[derive(Debug)]
pub struct TabManager {
    tabs: Vec<Tab>,
    next_id: u64,
    active: Option<TabId>,
    sleep_policy: SleepPolicy,
    discard_policy: DiscardPolicy,
    /// Upper bound for per-tab stored session history entries.
    history_cap: usize,
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new(SleepPolicy::default(), DiscardPolicy::default())
    }
}

impl TabManager {
    pub fn new(sleep_policy: SleepPolicy, discard_policy: DiscardPolicy) -> Self {
        Self {
            tabs: Vec::new(),
            next_id: 1,
            active: None,
            sleep_policy,
            discard_policy,
            history_cap: 50,
        }
    }

    pub fn with_policies(mut self, sleep: SleepPolicy, discard: DiscardPolicy) -> Self {
        self.sleep_policy = sleep;
        self.discard_policy = discard;
        self
    }

    // ---- introspection ----------------------------------------------------

    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    pub fn tab(&self, id: TabId) -> Result<&Tab, TabError> {
        self.tabs.iter().find(|t| t.id == id).ok_or(TabError::Unknown(id))
    }

    pub fn tab_mut(&mut self, id: TabId) -> Result<&mut Tab, TabError> {
        self.tabs.iter_mut().find(|t| t.id == id).ok_or(TabError::Unknown(id))
    }

    pub fn active_id(&self) -> Option<TabId> {
        self.active
    }

    pub fn active_tab(&self) -> Option<&Tab> {
        self.active.and_then(|id| self.tab(id).ok())
    }

    /// Total estimated memory of all tabs.
    pub fn estimated_memory_bytes(&self) -> u64 {
        self.tabs.iter().map(Tab::estimated_memory).sum()
    }

    /// Number of tabs in `Active | Background | Sleeping`.
    pub fn alive_count(&self) -> usize {
        self.tabs
            .iter()
            .filter(|t| matches!(t.state, TabState::Active | TabState::Background | TabState::Sleeping))
            .count()
    }

    // ---- lifecycle operations ---------------------------------------------

    /// Open a new tab. If `activate` is true the previous active tab is
    /// backgrounded. Returns the resulting events.
    pub fn create_tab(&mut self, url: Url, now: Instant, activate: bool) -> Vec<TabEvent> {
        let id = TabId(self.next_id);
        self.next_id += 1;
        let mut tab = Tab::new(id, url.clone(), now);
        tab.history.push(url);
        self.tabs.push(tab);

        let mut events = vec![TabEvent::Created(id)];
        if activate {
            events.extend(self.activate_tab(id, now));
        } else {
            events.push(TabEvent::Backgrounded(id));
        }
        events
    }

    /// Activate a tab: discard-tabs are restored first, the previous active
    /// tab is backgrounded.
    pub fn activate_tab(&mut self, id: TabId, now: Instant) -> Vec<TabEvent> {
        if self.active == Some(id) {
            return vec![TabEvent::Noop(id)];
        }
        let Some(tab) = self.tabs.iter_mut().find(|t| t.id == id) else {
            return vec![TabEvent::Noop(id)];
        };

        let mut events = Vec::new();
        match tab.state {
            TabState::Discarded => {
                let payload_url = tab.discarded.as_ref().map(|p| p.url.clone());
                let scroll = tab.discarded.as_ref().map(|p| p.scroll_y).unwrap_or(0.0);
                let zoom = tab.discarded.as_ref().map(|p| p.zoom).unwrap_or(1.0);
                let url = payload_url.unwrap_or_else(|| tab.url.clone());
                tab.state = TabState::Background;
                tab.discarded = None;
                tab.last_activated = now;
                tab.last_activity = now;
                events.push(TabEvent::Restored(id, url, scroll, zoom));
            }
            TabState::Sleeping => {
                tab.state = TabState::Background;
                tab.last_activated = now;
                tab.last_activity = now;
                events.push(TabEvent::Woken(id));
            }
            TabState::Active | TabState::Background => {
                tab.last_activated = now;
                tab.state = TabState::Background; // fixed below if it becomes active
            }
        }

        // Background the previously active tab.
        if let Some(prev) = self.active {
            if prev != id {
                if let Ok(prev_tab) = self.tab_mut(prev) {
                    prev_tab.state = TabState::Background;
                    events.push(TabEvent::Backgrounded(prev));
                }
            }
        }

        if let Ok(t) = self.tab_mut(id) {
            t.state = TabState::Active;
        }
        self.active = Some(id);
        events.push(TabEvent::Activated(id));
        events
    }

    /// Close a tab. Closing the active tab promotes its neighbor.
    pub fn close_tab(&mut self, id: TabId, now: Instant) -> Result<Vec<TabEvent>, TabError> {
        let index = self
            .tabs
            .iter()
            .position(|t| t.id == id)
            .ok_or(TabError::Unknown(id))?;

        self.tabs.remove(index);
        let mut events = vec![TabEvent::Closed(id)];

        if self.active == Some(id) {
            self.active = None;
            // Promote the nearest neighbor (prefer the tab to the left, like
            // every mainstream browser).
            if let Some(neighbor) = index
                .checked_sub(1)
                .and_then(|i| self.tabs.get(i))
                .map(|t| t.id)
                .or_else(|| self.tabs.get(index).map(|t| t.id))
            {
                events.extend(self.activate_tab(neighbor, now));
            }
        }
        Ok(events)
    }

    /// Record a URL navigation within a tab (pushes onto session history).
    pub fn record_navigation(
        &mut self,
        id: TabId,
        url: Url,
    ) -> Result<Vec<TabEvent>, TabError> {
        let cap = self.history_cap;
        let tab = self.tab_mut(id)?;
        tab.url = url.clone();
        tab.history.truncate(tab.history_index + 1);
        tab.history.push(url.clone());
        if tab.history.len() > cap {
            let over = tab.history.len() - cap;
            tab.history.drain(0..over);
            tab.history_index = tab.history_index.saturating_sub(over);
        }
        tab.history_index = tab.history.len() - 1;
        Ok(vec![TabEvent::UrlChanged(id, url)])
    }

    /// Move back one entry in the tab's session history. Returns the URL to
    /// load, or None when at the oldest entry.
    pub fn go_back(&mut self, id: TabId) -> Result<Option<Url>, TabError> {
        let tab = self.tab_mut(id)?;
        if tab.history_index == 0 {
            return Ok(None);
        }
        tab.history_index -= 1;
        Ok(Some(tab.history[tab.history_index].clone()))
    }

    /// Move forward one entry in the tab's session history.
    pub fn go_forward(&mut self, id: TabId) -> Result<Option<Url>, TabError> {
        let tab = self.tab_mut(id)?;
        if tab.history_index + 1 >= tab.history.len() {
            return Ok(None);
        }
        tab.history_index += 1;
        Ok(Some(tab.history[tab.history_index].clone()))
    }

    /// Jump to an absolute session-history index (clamped).
    pub fn history_goto(&mut self, id: TabId, index: usize) -> Result<Option<Url>, TabError> {
        let tab = self.tab_mut(id)?;
        let index = index.min(tab.history.len() - 1);
        tab.history_index = index;
        Ok(Some(tab.history[index].clone()))
    }

    pub fn set_title(&mut self, id: TabId, title: String) -> Result<Vec<TabEvent>, TabError> {
        let tab = self.tab_mut(id)?;
        tab.title = title.clone();
        Ok(vec![TabEvent::TitleChanged(id, title)])
    }

    pub fn set_activity(&mut self, id: TabId, now: Instant) -> Result<(), TabError> {
        self.tab_mut(id)?.last_activity = now;
        Ok(())
    }

    pub fn set_resource_usage(
        &mut self,
        id: TabId,
        rss_bytes: u64,
        cpu_millis: u64,
    ) -> Result<(), TabError> {
        let tab = self.tab_mut(id)?;
        tab.rss_bytes = rss_bytes;
        tab.cpu_millis = cpu_millis;
        Ok(())
    }

    pub fn set_pinned(&mut self, id: TabId, pinned: bool) -> Result<(), TabError> {
        self.tab_mut(id)?.pinned = pinned;
        Ok(())
    }

    pub fn set_audible(&mut self, id: TabId, audible: bool) -> Result<(), TabError> {
        self.tab_mut(id)?.audible = audible;
        Ok(())
    }

    pub fn set_scroll(&mut self, id: TabId, scroll_y: f64) -> Result<(), TabError> {
        self.tab_mut(id)?.scroll_y = scroll_y;
        Ok(())
    }

    /// Reorder a tab (drag & drop). No-op if positions are invalid.
    pub fn move_tab(&mut self, id: TabId, to: usize) -> Result<(), TabError> {
        let from = self
            .tabs
            .iter()
            .position(|t| t.id == id)
            .ok_or(TabError::Unknown(id))?;
        let to = to.min(self.tabs.len() - 1);
        if from != to {
            let tab = self.tabs.remove(from);
            self.tabs.insert(to, tab);
        }
        Ok(())
    }

    /// Run the automatic sleep pass; returns events for tabs that fell asleep.
    pub fn run_sleep_pass(&mut self, now: Instant) -> Vec<TabEvent> {
        let snapshot: Vec<Tab> = self.tabs.clone();
        let victims = select_tabs_to_sleep(&snapshot, &self.sleep_policy, now);
        let mut events = Vec::new();
        for id in victims {
            if let Ok(tab) = self.tab_mut(id) {
                if matches!(tab.state, TabState::Background) {
                    tab.state = TabState::Sleeping;
                    events.push(TabEvent::Slept(id));
                }
            }
        }
        events
    }

    /// Manually put a tab to sleep (user action / extension).
    pub fn sleep_tab(&mut self, id: TabId) -> Result<Vec<TabEvent>, TabError> {
        let tab = self.tab_mut(id)?;
        match tab.state {
            TabState::Background => {
                tab.state = TabState::Sleeping;
                Ok(vec![TabEvent::Slept(id)])
            }
            TabState::Sleeping | TabState::Discarded => Ok(vec![]),
            TabState::Active => Ok(vec![TabEvent::Noop(id)]),
        }
    }

    /// Wake a sleeping tab back to background tier.
    pub fn wake_tab(&mut self, id: TabId, now: Instant) -> Result<Vec<TabEvent>, TabError> {
        let tab = self.tab_mut(id)?;
        match tab.state {
            TabState::Sleeping => {
                tab.state = TabState::Background;
                tab.last_activity = now;
                Ok(vec![TabEvent::Woken(id)])
            }
            _ => Ok(vec![]),
        }
    }

    /// Discard a tab: engine-side the shell destroys the WebView and keeps
    /// only the payload.
    pub fn discard_tab(&mut self, id: TabId, now: Instant) -> Result<Vec<TabEvent>, TabError> {
        let tab = self.tab_mut(id)?;
        match tab.state {
            TabState::Active => Ok(vec![TabEvent::Noop(id)]),
            TabState::Discarded => Ok(vec![]),
            TabState::Background | TabState::Sleeping => {
                tab.state = TabState::Discarded;
                tab.discarded = Some(DiscardedTabPayload {
                    url: tab.url.clone(),
                    title: tab.title.clone(),
                    history_index: tab.history_index,
                    scroll_y: tab.scroll_y,
                    zoom: tab.zoom,
                    discarded_at: 0,
                });
                let _ = now;
                Ok(vec![TabEvent::Discarded(
                    id,
                    tab.discarded.as_ref().unwrap().clone(),
                )])
            }
        }
    }

    /// React to a memory sample: sleep + discard as the pressure demands.
    /// Returns the events the shell must perform, in order.
    pub fn handle_memory_sample(
        &mut self,
        rss_bytes: u64,
        now: Instant,
    ) -> Vec<TabEvent> {
        let pressure = self.discard_policy.pressure_for_rss(rss_bytes);
        let mut events = self.run_sleep_pass(now);

        if pressure != MemoryPressure::Normal {
            let snapshot: Vec<Tab> = self.tabs.clone();
            let victims = select_tabs_to_discard(&snapshot, &self.discard_policy, pressure);
            for id in victims {
                if let Ok(ev) = self.discard_tab(id, now) {
                    events.extend(ev);
                }
            }
        }
        events
    }

    /// Per-tab memory budget as configured (used by the dashboard UI).
    pub fn per_tab_budget(&self) -> u64 {
        // brow target: <100 MB per tab.
        100 * 1024 * 1024
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn url(n: u8) -> Url {
        format!("https://example.com/{n}").parse().unwrap()
    }

    fn now() -> Instant {
        Instant::now()
    }

    #[test]
    fn create_and_activate_flow() {
        let mut mgr = TabManager::default();
        let t = now();
        let ev1 = mgr.create_tab(url(1), t, true);
        assert_eq!(ev1, vec![TabEvent::Created(TabId(1)), TabEvent::Activated(TabId(1))]);

        let ev2 = mgr.create_tab(url(2), t, true);
        assert!(ev2.contains(&TabEvent::Created(TabId(2))));
        assert!(ev2.contains(&TabEvent::Backgrounded(TabId(1))));
        assert!(ev2.contains(&TabEvent::Activated(TabId(2))));
        assert_eq!(mgr.active_id(), Some(TabId(2)));
    }

    #[test]
    fn sleeping_tab_wakes_on_activation() {
        let mut mgr = TabManager::default();
        let t = now();
        mgr.create_tab(url(1), t, true);
        mgr.create_tab(url(2), t, true);
        let ev = mgr.sleep_tab(TabId(1)).unwrap();
        assert_eq!(ev, vec![TabEvent::Slept(TabId(1))]);

        let ev = mgr.activate_tab(TabId(1), t);
        assert!(ev.contains(&TabEvent::Woken(TabId(1))));
        assert!(ev.contains(&TabEvent::Activated(TabId(1))));
        assert_eq!(mgr.active_id(), Some(TabId(1)));
        assert_eq!(mgr.tab(TabId(1)).unwrap().state, TabState::Active);
    }

    #[test]
    fn discard_then_restore_preserves_scroll_and_zoom() {
        let mut mgr = TabManager::default();
        let t = now();
        mgr.create_tab(url(1), t, true);
        mgr.create_tab(url(2), t, true);
        mgr.set_scroll(TabId(1), 320.5).unwrap();
        mgr.tab_mut(TabId(1)).unwrap().zoom = 1.25;

        let ev = mgr.discard_tab(TabId(1), t).unwrap();
        assert!(matches!(ev.first(), Some(TabEvent::Discarded(_, _))));
        assert_eq!(mgr.tab(TabId(1)).unwrap().state, TabState::Discarded);

        // Restoring drops memory to the payload estimate.
        assert!(mgr.tab(TabId(1)).unwrap().estimated_memory() <= DISCARDED_TAB_MEMORY_ESTIMATE);

        let ev = mgr.activate_tab(TabId(1), t);
        assert!(ev.contains(&TabEvent::Restored(TabId(1), url(1), 320.5, 1.25)));
        assert_eq!(mgr.tab(TabId(1)).unwrap().state, TabState::Active);
        assert!(mgr.tab(TabId(1)).unwrap().discarded.is_none());
    }

    #[test]
    fn active_tab_cannot_be_discarded_or_slept() {
        let mut mgr = TabManager::default();
        let t = now();
        mgr.create_tab(url(1), t, true);
        assert_eq!(mgr.sleep_tab(TabId(1)).unwrap(), vec![TabEvent::Noop(TabId(1))]);
        assert_eq!(mgr.discard_tab(TabId(1), t).unwrap(), vec![TabEvent::Noop(TabId(1))]);
    }

    #[test]
    fn close_active_promotes_neighbor() {
        let mut mgr = TabManager::default();
        let t = now();
        mgr.create_tab(url(1), t, true);
        mgr.create_tab(url(2), t, true);
        mgr.create_tab(url(3), t, true);
        let ev = mgr.close_tab(TabId(3), t).unwrap();
        assert!(ev.contains(&TabEvent::Closed(TabId(3))));
        assert!(ev.contains(&TabEvent::Activated(TabId(2))));
        assert_eq!(mgr.active_id(), Some(TabId(2)));
    }

    #[test]
    fn navigation_builds_session_history() {
        let mut mgr = TabManager::default();
        let t = now();
        mgr.create_tab(url(1), t, true);
        mgr.record_navigation(TabId(1), url(2)).unwrap();
        mgr.record_navigation(TabId(1), url(3)).unwrap();
        let tab = mgr.tab(TabId(1)).unwrap();
        assert_eq!(tab.history.len(), 3);
        assert_eq!(tab.history_index, 2);
        assert_eq!(tab.url, url(3));
    }

    #[test]
    fn history_cap_trims_oldest() {
        let mut mgr = TabManager::default();
        let t = now();
        mgr.create_tab(url(0), t, true);
        for i in 1..70u8 {
            mgr.record_navigation(TabId(1), url(i)).unwrap();
        }
        let tab = mgr.tab(TabId(1)).unwrap();
        assert!(tab.history.len() <= 50);
        assert_eq!(tab.history.last().unwrap(), &url(69));
    }

    #[test]
    fn sleep_pass_uses_policy() {
        let sleep_policy = SleepPolicy {
            idle_timeout: Duration::from_secs(600),
            min_alive_tabs: 1,
            ..Default::default()
        };
        let mut mgr = TabManager::default().with_policies(sleep_policy, DiscardPolicy::default());
        let t = now();
        mgr.create_tab(url(1), t, true);
        mgr.create_tab(url(2), t, false);
        // Tab 2 was created 700s in the past.
        mgr.tab_mut(TabId(2)).unwrap().last_activity = t - Duration::from_secs(700);
        mgr.tab_mut(TabId(2)).unwrap().last_activated = t - Duration::from_secs(700);

        let ev = mgr.run_sleep_pass(t);
        assert_eq!(ev, vec![TabEvent::Slept(TabId(2))]);
        // Second pass is a no-op.
        assert!(mgr.run_sleep_pass(t).is_empty());
    }

    #[test]
    fn memory_sample_triggers_sleep_and_discard() {
        let sleep_policy = SleepPolicy {
            idle_timeout: Duration::from_secs(1),
            min_alive_tabs: 1,
            exempt_pinned: true,
            exempt_audible: true,
        };
        // Tiny budget → any real RSS is Critical.
        let discard_policy = DiscardPolicy {
            max_alive_tabs: 10,
            total_memory_budget: 1_000_000, // ~1 MB
            warning_ratio: 0.8,
            critical_ratio: 0.95,
        };
        let mut mgr = TabManager::default().with_policies(sleep_policy, discard_policy);
        let t = now();
        mgr.create_tab(url(1), t, true);
        mgr.create_tab(url(2), t, false);
        mgr.create_tab(url(3), t, false);
        for id in [TabId(2), TabId(3)] {
            mgr.tab_mut(id).unwrap().last_activity = t - Duration::from_secs(60);
            mgr.tab_mut(id).unwrap().last_activated = t - Duration::from_secs(60);
            mgr.set_resource_usage(id, 50 * 1024 * 1024, 0).unwrap();
        }

        let ev = mgr.handle_memory_sample(200 * 1024 * 1024, t);
        // Both background tabs must end up discarded (critical pressure).
        let discarded: Vec<_> = ev
            .iter()
            .filter_map(|e| match e {
                TabEvent::Discarded(id, _) => Some(*id),
                _ => None,
            })
            .collect();
        assert_eq!(discarded.len(), 2);
        assert_eq!(mgr.alive_count(), 1);
        // Memory estimate collapsed.
        assert!(mgr.estimated_memory_bytes() < 10 * 1024 * 1024);
    }

    #[test]
    fn move_tab_reorders() {
        let mut mgr = TabManager::default();
        let t = now();
        mgr.create_tab(url(1), t, false);
        mgr.create_tab(url(2), t, false);
        mgr.create_tab(url(3), t, false);
        mgr.move_tab(TabId(3), 0).unwrap();
        assert_eq!(mgr.tabs()[0].id, TabId(3));
        assert_eq!(mgr.tabs()[1].id, TabId(1));
        assert_eq!(mgr.tabs()[2].id, TabId(2));
    }

    #[test]
    fn pinned_tabs_survive_pressure() {
        let sleep_policy = SleepPolicy { idle_timeout: Duration::from_secs(1), min_alive_tabs: 1, ..Default::default() };
        let discard_policy = DiscardPolicy { total_memory_budget: 1000, ..Default::default() };
        let mut mgr = TabManager::default().with_policies(sleep_policy, discard_policy);
        let t = now();
        mgr.create_tab(url(1), t, true);
        mgr.create_tab(url(2), t, false);
        mgr.set_pinned(TabId(2), true).unwrap();
        mgr.tab_mut(TabId(2)).unwrap().last_activity = t - Duration::from_secs(60);
        mgr.tab_mut(TabId(2)).unwrap().last_activated = t - Duration::from_secs(60);

        let ev = mgr.handle_memory_sample(10_000_000, t);
        assert!(!ev.iter().any(|e| matches!(e, TabEvent::Discarded(TabId(2), _))));
        assert_eq!(mgr.tab(TabId(2)).unwrap().state, TabState::Background);
    }
}
