/* Chrome glue: mirrors browser state into Slint models/properties.
 *
 * Decoupled from the app through the `ChromeSource` trait, so the full
 * mapping is exercised headlessly by the UI smoke test. */

use std::rc::Rc;

use slint::{ModelRc, VecModel};

use crate::generated::{
    BookmarkInfo, BrowserChrome, DownloadInfo, HistoryInfo, L10nStrings, SettingsInfo, TabInfo,
};
use brow_shell_core::settings::Theme;
use brow_shell_core::{
    DownloadState, HistoryStore, L10n, Lang, BookmarkStore, DownloadManager, Settings, Str,
    TabManager, TabState,
};

/// Read-only view of browser state needed to drive the chrome.
pub trait ChromeSource {
    fn tabs(&self) -> &TabManager;
    fn tab_loading(&self) -> &std::collections::HashMap<u64, bool>;
    fn bookmarks(&self) -> &BookmarkStore;
    fn history(&self) -> &HistoryStore;
    fn downloads(&self) -> &DownloadManager;
    fn settings(&self) -> &Settings;
    fn lang(&self) -> Lang;
    fn active_tab_bookmarked(&self) -> bool;
}

pub fn l10n_for(settings: &Settings) -> L10n {
    L10n::new(settings.locale)
}

pub fn l10n_strings(l: &L10n) -> L10nStrings {
    L10nStrings {
        new_tab: l.tr(Str::NewTab).into(),
        close_tab: l.tr(Str::CloseTab).into(),
        address_hint: l.tr(Str::AddressBarHint).into(),
        back: l.tr(Str::Back).into(),
        forward: l.tr(Str::Forward).into(),
        reload: l.tr(Str::Reload).into(),
        home: l.tr(Str::Home).into(),
        bookmarks: l.tr(Str::Bookmarks).into(),
        history: l.tr(Str::History).into(),
        downloads: l.tr(Str::Downloads).into(),
        settings: l.tr(Str::Settings).into(),
        search_bookmarks: l.tr(Str::SearchBookmarks).into(),
        search_history: l.tr(Str::SearchHistory).into(),
        clear_history: l.tr(Str::ClearHistory).into(),
        no_downloads: l.tr(Str::NoDownloads).into(),
        memory: l.tr(Str::MemoryDashboard).into(),
        memory_per_tab: l.tr(Str::MemoryPerTab).into(),
        memory_total: l.tr(Str::MemoryTotal).into(),
        tabs: l.tr(Str::TabCount).into(),
        tab_slept: l.tr(Str::TabSlept).into(),
        menu: l.tr(Str::Menu).into(),
        language: l.tr(Str::Language).into(),
        search_engine: l.tr(Str::SearchEngine).into(),
        block_ads: l.tr(Str::BlockAds).into(),
        dnt: l.tr(Str::Dnt).into(),
        restore_session: l.tr(Str::RestoreSession).into(),
        dns_over_https: l.tr(Str::DnsOverHttps).into(),
        min_tls: l.tr(Str::MinTlsVersion).into(),
        // brow (v0.6.1): new privacy settings strings.
        third_party_cookies: l.tr(Str::ThirdPartyCookies).into(),
        fingerprint_defense: l.tr(Str::FingerprintDefense).into(),
        cname_tracking: l.tr(Str::CnameTracking).into(),
    }
}

pub fn settings_info(l: &L10n, settings: &Settings) -> SettingsInfo {
    SettingsInfo {
        locale_label: settings.locale.name().into(),
        theme_label: match settings.theme {
            Theme::Light => l.tr(Str::ThemeLight).into(),
            Theme::Dark => l.tr(Str::ThemeDark).into(),
            Theme::System => l.tr(Str::ThemeSystem).into(),
        },
        engine_label: format!("{:?}", settings.search_engine).into(),
        sleep_after: format!("{}s", settings.sleep_after_idle_secs).into(),
        per_tab_budget: format!("{} MB", settings.per_tab_memory_budget_mb).into(),
        total_budget: format!("{} MB", settings.total_memory_budget_mb).into(),
        block_ads: settings.block_ads,
        dnt: settings.dnt,
        restore_session: settings.restore_session,
        hidden_fps: format!("{}", settings.hidden_webview_fps).into(),
        // brow (v0.6.1): new privacy settings state.
        block_third_party_cookies: settings.block_third_party_cookies,
        block_cname_tracking: settings.block_cname_tracking,
        fingerprint_label: match settings.fingerprint_defense.as_str() {
            "off" => "off".into(),
            "strict" => "strict".into(),
            _ => "standard".into(),
        },
    }
}

pub fn tabs_model(
    tabs: &TabManager,
    loading: &std::collections::HashMap<u64, bool>,
) -> Vec<TabInfo> {
    tabs.tabs()
        .iter()
        .map(|t| TabInfo {
            id: t.id.0 as i32,
            title: if t.title.is_empty() {
                t.url.host_str().unwrap_or("New tab").to_string().into()
            } else {
                t.title.clone().into()
            },
            active: t.state == TabState::Active,
            sleeping: t.state == TabState::Sleeping,
            discarded: t.state == TabState::Discarded,
            audible: t.audible,
            loading: loading.get(&t.id.0).copied().unwrap_or(false),
        })
        .collect()
}

/// Push fresh state into the chrome component.
pub fn sync_tabs(chrome: &BrowserChrome, source: &dyn ChromeSource) {
    let l = L10n::new(source.lang());
    let mut model = tabs_model(source.tabs(), source.tab_loading());
    // RTL mirroring: the tab strip reads right-to-left for Arabic locales.
    if l.is_rtl() {
        model.reverse();
    }
    chrome.set_tabs(ModelRc::from(Rc::new(VecModel::from(model))));
    chrome.set_sleeping_count(
        source
            .tabs()
            .tabs()
            .iter()
            .filter(|t| t.state == TabState::Sleeping)
            .count() as i32,
    );
    chrome.set_rtl(l.is_rtl());
    chrome.set_i18n(l10n_strings(&l));
    chrome.set_settings(settings_info(&l, source.settings()));
    chrome.set_bookmarked(source.active_tab_bookmarked());
    if let Some(active) = source.tabs().active_id().and_then(|id| source.tabs().tab(id).ok()) {
        chrome.set_address(active.url.to_string().into());
    }
}

pub fn sync_panels(chrome: &BrowserChrome, source: &dyn ChromeSource) {
    let l = L10n::new(source.lang());
    let bookmarks: Vec<BookmarkInfo> = source
        .bookmarks()
        .all()
        .iter()
        .map(|b| BookmarkInfo {
            id: b.id as i32,
            title: b.title.clone().into(),
            url: b.url.to_string().into(),
        })
        .collect();
    chrome.set_bookmarks(ModelRc::from(Rc::new(VecModel::from(bookmarks))));

    let history_entries: Vec<HistoryInfo> = source
        .history()
        .recent(50)
        .into_iter()
        .map(|h| HistoryInfo {
            title: h.title.clone().into(),
            url: h.url.clone().into(),
            visits: h.visit_count as i32,
        })
        .collect();
    chrome.set_history_entries(ModelRc::from(Rc::new(VecModel::from(history_entries))));

    let downloads: Vec<DownloadInfo> = source
        .downloads()
        .downloads()
        .iter()
        .map(|d| {
            let state_text = match d.state {
                DownloadState::Queued | DownloadState::Connecting | DownloadState::Downloading => {
                    l.tr(Str::DownloadInProgress)
                }
                DownloadState::Paused => l.tr(Str::DownloadPaused),
                DownloadState::Completed => l.tr(Str::DownloadCompleted),
                DownloadState::Failed | DownloadState::Cancelled => l.tr(Str::DownloadFailed),
            };
            DownloadInfo {
                id: d.id as i32,
                name: d
                    .destination
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| d.url.clone())
                    .into(),
                progress: d.progress_fraction().unwrap_or(0.0) as f32,
                state_text: state_text.into(),
                done: d.state.is_terminal(),
            }
        })
        .collect();
    chrome.set_downloads(ModelRc::from(Rc::new(VecModel::from(downloads))));
}

/// Atomic JSON write helper shared with the app state.
pub(crate) fn write_atomic(path: &std::path::Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}
