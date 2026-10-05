/* Bookmark store: flat list + tags, JSON-persisted. */

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

use crate::StoreError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: u64,
    pub url: Url,
    pub title: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub added_at_unix: u64,
}

/// A flat, tag-based bookmark store (folders are a UI concern layered on tags).
#[derive(Debug)]
pub struct BookmarkStore {
    bookmarks: Vec<Bookmark>,
    next_id: u64,
    path: Option<PathBuf>,
}

impl Default for BookmarkStore {
    fn default() -> Self {
        Self {
            bookmarks: Vec::new(),
            next_id: 1,
            path: None,
        }
    }
}

impl BookmarkStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Open (or create) a store backed by a JSON file.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let store = if path.exists() {
            let raw = std::fs::read_to_string(path)?;
            let bookmarks: Vec<Bookmark> = serde_json::from_str(&raw)?;
            let next_id = bookmarks.iter().map(|b| b.id).max().unwrap_or(0) + 1;
            Self {
                bookmarks,
                next_id,
                path: Some(path.to_path_buf()),
            }
        } else {
            Self {
                bookmarks: Vec::new(),
                next_id: 1,
                path: Some(path.to_path_buf()),
            }
        };
        Ok(store)
    }

    pub fn len(&self) -> usize {
        self.bookmarks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bookmarks.is_empty()
    }

    pub fn all(&self) -> &[Bookmark] {
        &self.bookmarks
    }

    pub fn contains_url(&self, url: &Url) -> bool {
        self.bookmarks.iter().any(|b| &b.url == url)
    }

    pub fn add(
        &mut self,
        url: Url,
        title: impl Into<String>,
        tags: Vec<String>,
        now_unix: u64,
    ) -> u64 {
        // Re-adding an existing URL updates title/tags instead of duplicating.
        if let Some(existing) = self.bookmarks.iter_mut().find(|b| b.url == url) {
            existing.title = title.into();
            existing.tags = tags;
            existing.added_at_unix = now_unix;
            let id = existing.id;
            self.persist();
            return id;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.bookmarks.push(Bookmark {
            id,
            url,
            title: title.into(),
            tags,
            added_at_unix: now_unix,
        });
        self.persist();
        id
    }

    pub fn remove(&mut self, id: u64) -> bool {
        let before = self.bookmarks.len();
        self.bookmarks.retain(|b| b.id != id);
        let removed = self.bookmarks.len() != before;
        if removed {
            self.persist();
        }
        removed
    }

    pub fn rename(&mut self, id: u64, title: impl Into<String>) -> bool {
        let changed = self
            .bookmarks
            .iter_mut()
            .find(|b| b.id == id)
            .map(|b| {
                b.title = title.into();
                true
            })
            .unwrap_or(false);
        if changed {
            self.persist();
        }
        changed
    }

    pub fn set_tags(&mut self, id: u64, tags: Vec<String>) -> bool {
        let changed = self
            .bookmarks
            .iter_mut()
            .find(|b| b.id == id)
            .map(|b| {
                b.tags = tags;
                true
            })
            .unwrap_or(false);
        if changed {
            self.persist();
        }
        changed
    }

    /// Case-insensitive substring search over title, URL and tags.
    pub fn search(&self, query: &str) -> Vec<&Bookmark> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return self.bookmarks.iter().collect();
        }
        self.bookmarks
            .iter()
            .filter(|b| {
                b.title.to_lowercase().contains(&q)
                    || b.url.as_str().to_lowercase().contains(&q)
                    || b.tags.iter().any(|t| t.to_lowercase().contains(&q))
            })
            .collect()
    }

    pub fn by_tag(&self, tag: &str) -> Vec<&Bookmark> {
        self.bookmarks
            .iter()
            .filter(|b| b.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)))
            .collect()
    }

    fn persist(&self) {
        if let Some(path) = &self.path {
            let json = match serde_json::to_string_pretty(&self.bookmarks) {
                Ok(j) => j,
                Err(e) => {
                    log::warn!("bookmark serialize failed: {e}");
                    return;
                }
            };
            if let Err(e) = write_atomic(path, json.as_bytes()) {
                log::warn!("bookmark persist failed: {e}");
            }
        }
    }
}

/// Write via temp file + rename so a crash never truncates the store.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn u(s: &str) -> Url {
        s.parse().unwrap()
    }

    #[test]
    fn add_search_remove() {
        let mut store = BookmarkStore::new();
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        store.add(u("https://rust-lang.org/"), "Rust", vec!["dev".into()], now);
        store.add(u("https://servo.org/"), "Servo", vec!["engine".into(), "dev".into()], now);

        assert_eq!(store.len(), 2);
        assert!(store.contains_url(&u("https://rust-lang.org/")));

        let hits = store.search("servo");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Servo");

        let tag_hits = store.by_tag("dev");
        assert_eq!(tag_hits.len(), 2);

        assert!(store.remove(2));
        assert!(!store.contains_url(&u("https://servo.org/")));
    }

    #[test]
    fn re_add_updates_not_duplicates() {
        let mut store = BookmarkStore::new();
        store.add(u("https://a.example/"), "A", vec![], 1);
        store.add(u("https://a.example/"), "A2", vec!["x".into()], 2);
        assert_eq!(store.len(), 1);
        assert_eq!(store.all()[0].title, "A2");
    }

    #[test]
    fn persists_across_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bookmarks.json");
        {
            let mut store = BookmarkStore::open(&path).unwrap();
            store.add(u("https://persist.example/"), "Persist", vec!["p".into()], 42);
        }
        let store = BookmarkStore::open(&path).unwrap();
        assert_eq!(store.len(), 1);
        assert_eq!(store.all()[0].title, "Persist");
        assert_eq!(store.all()[0].added_at_unix, 42);
    }

    #[test]
    fn rename_and_tags() {
        let mut store = BookmarkStore::new();
        let id = store.add(u("https://x.example/"), "X", vec![], 1);
        assert!(store.rename(id, "Y"));
        assert!(store.set_tags(id, vec!["t".into()]));
        assert_eq!(store.all()[0].title, "Y");
        assert_eq!(store.all()[0].tags, vec!["t"]);
        assert!(!store.rename(999, "nope"));
    }
}
