/* Headless UI smoke test: builds the real Slint chrome component against a
 * buffer-backed surface, feeds it realistic state (including the Arabic RTL
 * locale), and renders frames. Requires no display, no engine, no GPU.
 *
 * Verifies the entire Slint compile + platform + software-renderer path that
 * the product chrome uses, plus the chrome<->core model mapping. */

use std::collections::HashMap;
use std::rc::Rc;

use brow_shell::chrome::{sync_panels, sync_tabs, ChromeSource};
use brow_shell::generated::BrowserChrome;
use brow_shell::platform::{queue_global, ChromeSurface};
use brow_shell_core::{
    BookmarkStore, DownloadManager, HistoryStore, L10n, Lang, Settings, TabId, TabManager,
};

/// Minimal state stub implementing the chrome's data source trait.
struct TestSource {
    tabs: TabManager,
    tab_loading: HashMap<u64, bool>,
    bookmarks: BookmarkStore,
    history: HistoryStore,
    downloads: DownloadManager,
    settings: Settings,
    lang: Lang,
}

impl ChromeSource for TestSource {
    fn tabs(&self) -> &TabManager {
        &self.tabs
    }
    fn tab_loading(&self) -> &HashMap<u64, bool> {
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
    fn lang(&self) -> Lang {
        self.lang
    }
    fn active_tab_bookmarked(&self) -> bool {
        false
    }
}

#[test]
fn chrome_ui_renders_headless() {
    use slint::Model;
    use slint::platform::Platform;
    use slint::platform::WindowAdapter;

    slint::platform::set_platform(Box::new(brow_shell::platform::BrowPlatform))
        .expect("platform must only be set once per process");

    let surface = Rc::new(ChromeSurface::new_headless());
    queue_global(surface.slint_window.clone());

    // Instantiate the real chrome component.
    let chrome = BrowserChrome::new().expect("chrome component must build");

    // Feed it state: two tabs (one active, one sleeping), Arabic RTL locale.
    let mut tabs = TabManager::default();
    let now = std::time::Instant::now();
    tabs.create_tab("https://example.com/".parse().unwrap(), now, true);
    tabs.create_tab("https://servo.org/".parse().unwrap(), now, false);
    let _ = tabs.sleep_tab(TabId(2));
    let _ = tabs.set_title(TabId(1), "Example Domain".to_string());

    let source = TestSource {
        tabs,
        tab_loading: HashMap::new(),
        bookmarks: BookmarkStore::new(),
        history: HistoryStore::new(100),
        downloads: DownloadManager::new(),
        settings: Settings::default(),
        lang: Lang::Ar,
    };

    sync_tabs(&chrome, &source);
    chrome.set_panel_kind(1);
    sync_panels(&chrome, &source);

    // Render one frame into the CPU buffer.
    let drew = surface.draw_if_needed();
    assert!(drew, "first frame must render");
    let buffer = surface.buffer_snapshot();
    assert_eq!(buffer.len(), 1240 * 88, "buffer must match surface size");
    assert!(
        buffer.iter().any(|px| px.0 != 0),
        "frame must have content"
    );

    // A property change must trigger a second frame.
    chrome.set_panel_kind(4);
    surface.slint_window.window().request_redraw();
    assert!(surface.draw_if_needed(), "second frame must render");

    // State must have propagated into the component.
    assert!(chrome.get_rtl(), "Arabic locale implies RTL");
    assert_eq!(chrome.get_sleeping_count(), 1);
    assert_eq!(chrome.get_tabs().row_count(), 2);
    assert_eq!(chrome.get_bookmarks().row_count(), 0);
    // Arabic strings must be present in the i18n bundle.
    assert_eq!(chrome.get_i18n().new_tab, "تبويب جديد");
}
