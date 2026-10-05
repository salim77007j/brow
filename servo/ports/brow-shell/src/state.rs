/* brow-shell shared state: core (servo-free) + engine handles.
 *
 * The `engine` feature (default) pulls in libservo and the winit content
 * windows. Without it (e.g. headless UI verification) only the chrome state
 * and the Slint stack compile. This is a compile-time feature split, not a
 * stub: the engine code paths are the same code CI builds.
 */

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use winit::event_loop::EventLoopProxy;

use brow_shell_core::{
    BookmarkStore, DiscardPolicy, DownloadManager, HistoryStore, L10n, MemoryBudgetTracker,
    Settings, SleepPolicy, TabId, TabManager,
};

/// Application event used to wake the winit loop from engine threads.
#[derive(Debug)]
pub enum BrowEvent {
    /// Generic pump request (servo work pending).
    Wake,
    /// The chrome asked for a refresh of UI models.
    ChromeSync,
}

/// UI intent produced by chrome callbacks; executed where ActiveEventLoop is
/// available (engine builds).
#[derive(Debug, Clone)]
pub enum Action {
    ActivateTab(TabId),
    CloseTab(TabId),
    NewTab(url::Url),
    Back,
    Forward,
    ReloadActive,
    AddressSubmitted(String),
    TogglePanel(i32),
    ToggleBookmark,
    RemoveBookmark(u64),
    ClearHistory,
    ToggleSetting(String),
    CycleSetting(String),
    SleepActiveTab,
}

pub const RESOURCE_SAMPLE_INTERVAL: Duration = Duration::from_secs(5);
pub const SLEEP_PASS_INTERVAL: Duration = Duration::from_secs(30);

/// One engine-backed tab: a winit window + surfman context + WebView.
#[cfg(feature = "engine")]
pub struct ContentWindow {
    pub window: winit::window::Window,
    pub rendering_context: std::rc::Rc<servo::WindowRenderingContext>,
    pub webview: servo::WebView,
    /// Last cursor position (physical px within this window).
    pub last_cursor: std::cell::Cell<(f64, f64)>,
}

pub struct BrowState {
    pub proxy: EventLoopProxy<BrowEvent>,
    pub data_dir: PathBuf,

    // ---- brow-shell-core: the chrome brain (always present) ----
    pub tabs: TabManager,
    pub bookmarks: BookmarkStore,
    pub history: HistoryStore,
    pub downloads: DownloadManager,
    pub settings: Settings,
    pub l10n: L10n,
    pub memwatch: MemoryBudgetTracker,

    // ---- engine + UI handles (engine feature) ----
    #[cfg(feature = "engine")]
    pub servo: Option<servo::Servo>,
    pub chrome: Option<crate::generated::BrowserChrome>,
    pub chrome_surface: Option<Rc<crate::platform::ChromeSurface>>,
    #[cfg(feature = "engine")]
    pub chrome_window_id: Option<winit::window::WindowId>,
    #[cfg(feature = "engine")]
    pub content: HashMap<TabId, ContentWindow>,
    #[cfg(feature = "engine")]
    pub webview_to_tab: HashMap<servo::WebViewId, TabId>,
    pub tab_loading: HashMap<u64, bool>,

    // ---- bookkeeping ----
    pub pending: Vec<Action>,
    pub quitting: bool,
    pub last_resource_sample: Instant,
    pub last_sleep_pass: Instant,
}

pub fn l10n_for(settings: &Settings) -> L10n {
    L10n::new(settings.locale)
}

impl BrowState {
    pub fn new(proxy: EventLoopProxy<BrowEvent>) -> Self {
        let data_dir = std::env::var("BROW_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("XDG_CONFIG_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| {
                        std::env::var("HOME")
                            .map(|h| PathBuf::from(h).join(".config"))
                            .unwrap_or_else(|_| PathBuf::from("."))
                    })
                    .join("brow")
            });
        let _ = std::fs::create_dir_all(&data_dir);

        let settings = Settings::load(&data_dir.join("settings.json")).unwrap_or_default();
        let bookmarks = BookmarkStore::open(&data_dir.join("bookmarks.json")).unwrap_or_default();
        let history =
            HistoryStore::open(&data_dir.join("history.json"), 10_000).unwrap_or_default();
        let downloads =
            DownloadManager::open(&data_dir.join("downloads.json")).unwrap_or_default();

        let sleep_policy = SleepPolicy {
            idle_timeout: Duration::from_secs(settings.sleep_after_idle_secs),
            ..SleepPolicy::default()
        };
        let discard_policy = DiscardPolicy {
            total_memory_budget: u64::from(settings.total_memory_budget_mb) * 1024 * 1024,
            ..DiscardPolicy::default()
        };
        let tabs = TabManager::default().with_policies(sleep_policy, discard_policy);
        let memwatch = MemoryBudgetTracker::default();
        let l10n = l10n_for(&settings);

        Self {
            proxy,
            data_dir,
            tabs,
            bookmarks,
            history,
            downloads,
            settings,
            l10n,
            memwatch,
            #[cfg(feature = "engine")]
            servo: None,
            chrome: None,
            chrome_surface: None,
            #[cfg(feature = "engine")]
            chrome_window_id: None,
            #[cfg(feature = "engine")]
            content: HashMap::new(),
            #[cfg(feature = "engine")]
            webview_to_tab: HashMap::new(),
            tab_loading: HashMap::new(),
            pending: Vec::new(),
            quitting: false,
            last_resource_sample: Instant::now(),
            last_sleep_pass: Instant::now(),
        }
    }

    /// Push chrome state into the Slint models.
    pub fn sync_chrome(&self) {
        let Some(chrome) = &self.chrome else { return };
        crate::chrome::sync_tabs(chrome, self);
        crate::chrome::sync_panels(chrome, self);
        chrome.set_rtl(self.l10n.is_rtl());
    }

    pub fn active_tab_bookmarked(&self) -> bool {
        self.tabs
            .active_id()
            .and_then(|id| self.tabs.tab(id).ok())
            .map(|t| self.bookmarks.contains_url(&t.url))
            .unwrap_or(false)
    }

    pub fn open_tab(&mut self, url: url::Url, activate: bool) {
        let now = Instant::now();
        self.tabs.create_tab(url, now, activate);
    }

    pub fn persist_session(&self) {
        let urls: Vec<String> = self
            .tabs
            .tabs()
            .iter()
            .map(|t| t.url.to_string())
            .collect();
        if let Ok(json) = serde_json::to_string(&urls) {
            let _ = crate::chrome::write_atomic(
                &self.data_dir.join("session.json"),
                json.as_bytes(),
            );
        }
        let _ = self.settings.save(&self.data_dir.join("settings.json"));
    }
}

/// Convenience alias matching the app's ownership model.
pub type SharedState = Rc<RefCell<BrowState>>;

impl crate::chrome::ChromeSource for BrowState {
    fn tabs(&self) -> &TabManager {
        &self.tabs
    }

    fn tab_loading(&self) -> &std::collections::HashMap<u64, bool> {
        &self.tab_loading
    }

    fn bookmarks(&self) -> &BookmarkStore {
        &self.bookmarks
    }

    fn history(&self) -> &HistoryStore {
        &self.history
    }

    fn downloads(&self) -> &DownloadManager {
        &self.downloads
    }

    fn settings(&self) -> &Settings {
        &self.settings
    }

    fn lang(&self) -> brow_shell_core::Lang {
        self.settings.locale
    }

    fn active_tab_bookmarked(&self) -> bool {
        self.active_tab_bookmarked()
    }
}
