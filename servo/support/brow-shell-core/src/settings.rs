/* Settings registry: typed fields, validated updates, JSON persistence.
 *
 * Shell-level settings only — engine prefs (network.doh.*, etc.) remain in
 * servo's Preferences and are bridged by the shell at startup.
 */

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Lang;
use crate::StoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    Light,
    Dark,
    #[default]
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SearchEngine {
    DuckDuckGo,
    #[default]
    Google,
    Bing,
}

impl SearchEngine {
    pub fn search_url(&self, query: &str) -> String {
        let q = urlencoding_lite(query);
        match self {
            SearchEngine::DuckDuckGo => format!("https://duckduckgo.com/?q={q}"),
            SearchEngine::Google => format!("https://www.google.com/search?q={q}"),
            SearchEngine::Bing => format!("https://www.bing.com/search?q={q}"),
        }
    }
}

/// Percent-encode everything outside the unreserved set. Minimal, dependency-
/// free implementation for search terms.
fn urlencoding_lite(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// All shell settings, with brow's defaults.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    pub locale: Lang,
    pub search_engine: SearchEngine,
    /// Page opened for new tabs (empty = engine default new-tab page).
    pub home_page: String,
    /// Tab sleeping: idle seconds before a background tab sleeps.
    pub sleep_after_idle_secs: u64,
    /// Engine pref bridge: animation FPS cap for hidden webviews.
    pub hidden_webview_fps: u32,
    /// Per-tab memory target in MiB (the <100 MB goal).
    pub per_tab_memory_budget_mb: u32,
    /// Total memory budget in MiB driving discard pressure.
    pub total_memory_budget_mb: u32,
    /// Automatically sleep background tabs.
    pub auto_sleep_tabs: bool,
    /// Phase 4 hook: built-in ad/tracker blocking.
    pub block_ads: bool,
    /// Phase 2 pref bridge: DoH template URL (empty = built-in defaults).
    pub doh_template: String,
    /// Phase 2 pref bridge: minimum TLS version ("1.2" | "1.3").
    pub min_tls_version: String,
    /// Directory where downloads are saved.
    pub downloads_dir: PathBuf,
    /// Restore the previous session's tabs on startup.
    pub restore_session: bool,
    /// Send Do-Not-Track header.
    pub dnt: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            locale: Lang::En,
            search_engine: SearchEngine::Google,
            home_page: String::from("servo:browhome"),
            sleep_after_idle_secs: 10 * 60,
            hidden_webview_fps: 1,
            per_tab_memory_budget_mb: 100,
            total_memory_budget_mb: 2048,
            auto_sleep_tabs: true,
            block_ads: true,
            doh_template: String::new(),
            min_tls_version: String::from("1.2"),
            downloads_dir: PathBuf::from("."),
            restore_session: true,
            dnt: false,
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum SettingsError {
    #[error("unknown setting key: {0}")]
    UnknownKey(String),
    #[error("invalid value for {key}: {reason}")]
    InvalidValue { key: String, reason: String },
}

impl Settings {
    pub fn load(path: &Path) -> Result<Self, StoreError> {
        if path.exists() {
            let s: Settings = serde_json::from_str(&std::fs::read_to_string(path)?)?;
            Ok(s)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), StoreError> {
        let json = serde_json::to_string_pretty(self)?;
        crate::bookmarks::write_atomic(path, json.as_bytes())?;
        Ok(())
    }

    /// Generic setter used by the settings UI. Returns a validation error for
    /// bad values; unknown keys are also errors (typo protection).
    pub fn set_from_str(&mut self, key: &str, value: &str) -> Result<(), SettingsError> {
        let invalid = |reason: &str| SettingsError::InvalidValue {
            key: key.to_string(),
            reason: reason.to_string(),
        };
        match key {
            "theme" => {
                self.theme = match value {
                    "light" => Theme::Light,
                    "dark" => Theme::Dark,
                    "system" => Theme::System,
                    _ => return Err(invalid("expected light|dark|system")),
                };
            }
            "locale" => {
                self.locale = match value {
                    "en" => Lang::En,
                    "ar" => Lang::Ar,
                    _ => return Err(invalid("expected en|ar")),
                };
            }
            "search_engine" => {
                self.search_engine = match value {
                    "duckduckgo" => SearchEngine::DuckDuckGo,
                    "google" => SearchEngine::Google,
                    "bing" => SearchEngine::Bing,
                    _ => return Err(invalid("expected duckduckgo|google|bing")),
                };
            }
            "home_page" => {
                if value.contains(char::is_whitespace) {
                    return Err(invalid("URL must not contain whitespace"));
                }
                self.home_page = value.to_string();
            }
            "sleep_after_idle_secs" => {
                let v: u64 = value.parse().map_err(|_| invalid("not a number"))?;
                if v < 30 {
                    return Err(invalid("minimum 30 seconds"));
                }
                self.sleep_after_idle_secs = v;
            }
            "hidden_webview_fps" => {
                let v: u32 = value.parse().map_err(|_| invalid("not a number"))?;
                if v == 0 || v > 60 {
                    return Err(invalid("expected 1..=60"));
                }
                self.hidden_webview_fps = v;
            }
            "per_tab_memory_budget_mb" => {
                let v: u32 = value.parse().map_err(|_| invalid("not a number"))?;
                if !(16..=4096).contains(&v) {
                    return Err(invalid("expected 16..=4096 MiB"));
                }
                self.per_tab_memory_budget_mb = v;
            }
            "total_memory_budget_mb" => {
                let v: u32 = value.parse().map_err(|_| invalid("not a number"))?;
                if v < 256 {
                    return Err(invalid("minimum 256 MiB"));
                }
                self.total_memory_budget_mb = v;
            }
            "auto_sleep_tabs" => self.auto_sleep_tabs = parse_bool(value).ok_or_else(|| invalid("expected true|false"))?,
            "block_ads" => self.block_ads = parse_bool(value).ok_or_else(|| invalid("expected true|false"))?,
            "doh_template" => {
                if !value.is_empty() && !value.starts_with("https://") {
                    return Err(invalid("DoH template must be https:// or empty"));
                }
                self.doh_template = value.to_string();
            }
            "min_tls_version" => {
                if !matches!(value, "1.2" | "1.3") {
                    return Err(invalid("expected 1.2|1.3"));
                }
                self.min_tls_version = value.to_string();
            }
            "downloads_dir" => {
                if value.trim().is_empty() {
                    return Err(invalid("path must not be empty"));
                }
                self.downloads_dir = PathBuf::from(value);
            }
            "restore_session" => self.restore_session = parse_bool(value).ok_or_else(|| invalid("expected true|false"))?,
            "dnt" => self.dnt = parse_bool(value).ok_or_else(|| invalid("expected true|false"))?,
            _ => return Err(SettingsError::UnknownKey(key.to_string())),
        }
        Ok(())
    }

    /// Enumerate editable keys for the settings UI.
    pub fn keys() -> &'static [&'static str] {
        &[
            "theme",
            "locale",
            "search_engine",
            "home_page",
            "sleep_after_idle_secs",
            "hidden_webview_fps",
            "per_tab_memory_budget_mb",
            "total_memory_budget_mb",
            "auto_sleep_tabs",
            "block_ads",
            "doh_template",
            "min_tls_version",
            "downloads_dir",
            "restore_session",
            "dnt",
        ]
    }
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_brow_targets() {
        let s = Settings::default();
        assert_eq!(s.per_tab_memory_budget_mb, 100);
        assert_eq!(s.hidden_webview_fps, 1);
        assert_eq!(s.min_tls_version, "1.2");
        assert!(s.block_ads);
    }

    #[test]
    fn set_and_validate() {
        let mut s = Settings::default();
        s.set_from_str("theme", "dark").unwrap();
        assert_eq!(s.theme, Theme::Dark);
        s.set_from_str("locale", "ar").unwrap();
        assert_eq!(s.locale, Lang::Ar);
        s.set_from_str("hidden_webview_fps", "5").unwrap();
        assert_eq!(s.hidden_webview_fps, 5);
        assert_eq!(s.set_from_str("hidden_webview_fps", "0"), Err(SettingsError::InvalidValue { key: "hidden_webview_fps".into(), reason: "expected 1..=60".into() }));
        assert_eq!(s.set_from_str("nope", "1"), Err(SettingsError::UnknownKey("nope".into())));
        assert_eq!(s.set_from_str("doh_template", "http://x"), Err(SettingsError::InvalidValue { key: "doh_template".into(), reason: "DoH template must be https:// or empty".into() }));
        assert_eq!(s.set_from_str("min_tls_version", "1.1"), Err(SettingsError::InvalidValue { key: "min_tls_version".into(), reason: "expected 1.2|1.3".into() }));
    }

    #[test]
    fn search_urls_encode() {
        let s = Settings::default();
        assert_eq!(
            s.search_engine.search_url("rust Borrow checker"),
            "https://www.google.com/search?q=rust%20Borrow%20checker"
        );
        let mut s2 = Settings::default();
        s2.set_from_str("search_engine", "duckduckgo").unwrap();
        assert!(s2.search_engine.search_url("a&b").starts_with("https://duckduckgo.com/?q=a%26b"));
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        {
            let mut s = Settings::default();
            s.set_from_str("locale", "ar").unwrap();
            s.set_from_str("sleep_after_idle_secs", "600").unwrap();
            s.save(&path).unwrap();
        }
        let s = Settings::load(&path).unwrap();
        assert_eq!(s.locale, Lang::Ar);
        assert_eq!(s.sleep_after_idle_secs, 600);
    }

    #[test]
    fn partial_file_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.json");
        std::fs::write(&path, r#"{"theme": "dark"}"#).unwrap();
        let s = Settings::load(&path).unwrap();
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.locale, Lang::En); // defaulted via #[serde(default)]
    }

    #[test]
    fn keys_complete() {
        // Every key must be settable round-trip via set_from_str.
        for key in Settings::keys() {
            let mut s = Settings::default();
            let probe = match *key {
                "theme" => "light",
                "locale" => "ar",
                "search_engine" => "bing",
                "home_page" => "https://home.example/",
                "sleep_after_idle_secs" => "120",
                "hidden_webview_fps" => "2",
                "per_tab_memory_budget_mb" => "128",
                "total_memory_budget_mb" => "1024",
                "auto_sleep_tabs" | "block_ads" | "restore_session" | "dnt" => "false",
                "doh_template" => "",
                "min_tls_version" => "1.3",
                "downloads_dir" => "/tmp/dl",
                _ => unreachable!(),
            };
            assert!(s.set_from_str(key, probe).is_ok(), "key {key} failed");
        }
    }
}
