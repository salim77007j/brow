/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Per-site user privacy exceptions, persisted as JSON (atomic write, same
//! pattern as the phase-3 stores).

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::fingerprint::DefenseLevel;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PrivacyExceptions {
    /// Sites where the *network filter* is fully disabled
    /// (`https://example.com` — no port, lowercase host).
    pub filter_allow: Vec<String>,
    /// Sites with an explicit fingerprint-defense level override.
    pub defense_levels: std::collections::BTreeMap<String, DefenseLevel>,
    /// Sites where cosmetic (element-hiding) is disabled.
    pub cosmetic_disabled: Vec<String>,
}

/// Thread-safe persisted store.
pub struct ExceptionsStore {
    state: RwLock<PrivacyExceptions>,
    path: Option<PathBuf>,
}

impl ExceptionsStore {
    pub fn new(path: Option<PathBuf>) -> ExceptionsStore {
        let initial = path
            .as_ref()
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        ExceptionsStore { state: RwLock::new(initial), path }
    }

    pub fn snapshot(&self) -> PrivacyExceptions {
        self.state.read().clone()
    }

    /// True when the network filter must be bypassed for this site host.
    pub fn filter_allowed(&self, site_host: &str) -> bool {
        let s = self.state.read();
        s.filter_allow.iter().any(|e| host_matches_entry(site_host, e))
    }

    pub fn cosmetic_disabled(&self, site_host: &str) -> bool {
        let s = self.state.read();
        s.cosmetic_disabled.iter().any(|e| host_matches_entry(site_host, e))
    }

    pub fn defense_level(
        &self,
        origin: &str,
        default: DefenseLevel,
    ) -> DefenseLevel {
        let s = self.state.read();
        s.defense_levels
            .get(origin)
            .copied()
            .unwrap_or(default)
    }

    pub fn add_filter_allow(&self, entry: String) {
        let mut s = self.state.write();
        if !s.filter_allow.contains(&entry) {
            s.filter_allow.push(entry);
        }
        self.persist(&s);
    }

    pub fn remove_filter_allow(&self, entry: &str) {
        let mut s = self.state.write();
        s.filter_allow.retain(|e| e != entry);
        self.persist(&s);
    }

    pub fn set_defense_level(&self, origin: String, level: DefenseLevel) {
        let mut s = self.state.write();
        s.defense_levels.insert(origin, level);
        self.persist(&s);
    }

    fn persist(&self, state: &PrivacyExceptions) {
        if let Some(path) = &self.path {
            if let Some(dir) = path.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let tmp = path.with_extension("json.tmp");
            if serde_json::to_vec(state)
                .ok()
                .and_then(|bytes| std::fs::write(&tmp, bytes).ok())
                .is_some()
            {
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }
}

/// Exception entries may be a bare host or `sub.`-suffixed wildcard form
/// (`example.com` matches www.example.com at label boundaries).
fn host_matches_entry(host: &str, entry: &str) -> bool {
    let entry = entry.trim().to_ascii_lowercase();
    host == entry || host.ends_with(&format!(".{entry}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("brow-priv-exc-{tag}-{}.json", std::process::id()))
    }

    #[test]
    fn allow_and_remove_roundtrip() {
        let p = tmp_path("roundtrip");
        let _ = std::fs::remove_file(&p);
        let store = ExceptionsStore::new(Some(p.clone()));
        store.add_filter_allow("trusted.example".into());
        assert!(store.filter_allowed("trusted.example"));
        assert!(store.filter_allowed("www.trusted.example"));
        assert!(!store.filter_allowed("evil-trusted.example"));
        store.remove_filter_allow("trusted.example");
        assert!(!store.filter_allowed("trusted.example"));

        // persisted across stores
        store.add_filter_allow("keep.example".into());
        let store2 = ExceptionsStore::new(Some(p));
        assert!(store2.filter_allowed("keep.example"));
    }

    #[test]
    fn defense_level_override() {
        let p = tmp_path("defense");
        let _ = std::fs::remove_file(&p);
        let store = ExceptionsStore::new(Some(p));
        assert_eq!(
            store.defense_level("https://a.example", DefenseLevel::Standard),
            DefenseLevel::Standard
        );
        store.set_defense_level("https://a.example".into(), DefenseLevel::Strict);
        assert_eq!(
            store.defense_level("https://a.example", DefenseLevel::Standard),
            DefenseLevel::Strict
        );
    }

    #[test]
    fn cosmetic_toggle() {
        let store = ExceptionsStore::new(None);
        store.add_filter_allow("x.example".into());
        assert!(!store.cosmetic_disabled("x.example"));
    }
}
