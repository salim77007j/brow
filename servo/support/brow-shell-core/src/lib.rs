/* brow-shell-core — browser chrome brain for brow.
 * MPL-2.0. Part of the brow project (https://github.com/salim77007j/brow).
 *
 * This crate is deliberately engine-agnostic: it owns all *state and policy*
 * of the browser chrome (tabs, bookmarks, history, downloads, settings,
 * localization, memory budgeting) as pure, fully tested Rust. The `brow-shell`
 * frontend binds this logic to winit/Slint/libservo.
 */

pub mod bookmarks;
pub mod downloads;
pub mod history;
pub mod i18n;
pub mod lifecycle;
pub mod memwatch;
pub mod settings;
pub mod tabs;

pub use bookmarks::BookmarkStore;
pub use downloads::{Download, DownloadId, DownloadManager, DownloadState};
pub use history::HistoryStore;
pub use i18n::{Lang, L10n, Str};
pub use lifecycle::{DiscardPolicy, MemoryPressure, SleepPolicy};
pub use memwatch::{BudgetVerdict, MemoryBudgetTracker, MemorySample};
pub use settings::{Settings, SettingsError};
pub use tabs::{Tab, TabEvent, TabId, TabManager, TabState};

/// Common error type for chrome stores.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
}

/// Parse and normalize a URL string, rejecting empty or non-URL input that has
/// no scheme. Schemes commonly typed in address bars get a sensible default.
pub fn normalize_url(input: &str, default_scheme: &str) -> Result<url::Url, StoreError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(StoreError::InvalidUrl(trimmed.to_string()));
    }
    // If the input has no scheme and looks like a host (contains a dot or
    // localhost), prepend the default scheme so "example.com" works.
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else if looks_like_host(trimmed) {
        format!("{default_scheme}://{trimmed}")
    } else {
        trimmed.to_string()
    };
    let parsed = url::Url::parse(&candidate)
        .map_err(|_| StoreError::InvalidUrl(trimmed.to_string()))?;
    Ok(parsed)
}

fn looks_like_host(s: &str) -> bool {
    // Treat "host[:port][/path]" and "localhost..." as hosts; keep searches
    // ("two words") and single tokens without dots out of host detection.
    let host = s.split(['/', '?', '#']).next().unwrap_or(s);
    host == "localhost"
        || host.starts_with("localhost:")
        || (host.contains('.') && !host.contains(' ') && !host.ends_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_adds_scheme_for_hosts() {
        let u = normalize_url("example.com", "https").unwrap();
        assert_eq!(u.as_str(), "https://example.com/");
    }

    #[test]
    fn normalize_keeps_existing_scheme() {
        let u = normalize_url("http://example.com/x?q=1", "https").unwrap();
        assert_eq!(u.as_str(), "http://example.com/x?q=1");
    }

    #[test]
    fn normalize_localhost() {
        let u = normalize_url("localhost:8080/admin", "http").unwrap();
        assert_eq!(u.as_str(), "http://localhost:8080/admin");
    }

    #[test]
    fn normalize_search_terms_are_not_urls() {
        // A plain search phrase without a scheme is not a URL; the shell is
        // expected to route it to the search engine instead.
        assert!(normalize_url("rust Borrow checker explained", "https").is_err());
    }

    #[test]
    fn normalize_rejects_empty() {
        assert!(normalize_url("   ", "https").is_err());
    }
}
