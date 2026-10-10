/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! # brow-privacy
//!
//! The privacy engine of the **brow** browser (Phase 4):
//!
//! * [`filter`] — network filtering with EasyList/ABP syntax over an
//!   Aho-Corasick prefilter, plus element-hiding (cosmetic) filtering.
//! * [`cname`] — CNAME-cloaking detection (third-party trackers disguised
//!   as first-party hostnames via DNS).
//! * [`chips`] — CHIPS (Cookies Having Independent Partitioned State)
//!   parse/enforce layer for `Partitioned` cookies.
//! * [`fingerprint`] — anti-fingerprinting defense payloads (Canvas 2D,
//!   WebGL, AudioContext, fonts, Navigator) generated per protection level
//!   and injected as userscripts.
//! * [`exceptions`] — per-site user exceptions (persisted JSON).
//! * [`stats`] — blocking statistics, persisted on exit.

pub mod chips;
pub mod cname;
pub mod exceptions;
pub mod filter;
pub mod fingerprint;
pub mod stats;
pub mod userscripts;

pub mod lists {
    //! Filter-list loading helpers.
    use crate::filter::FilterEngine;
    use std::path::Path;

    /// Build an engine from a list of files (UTF-8 filter lists).
    pub fn engine_from_files(paths: &[&Path]) -> std::io::Result<FilterEngine> {
        let mut texts = Vec::with_capacity(paths.len());
        for p in paths {
            let t = std::fs::read_to_string(p)?;
            texts.push(t);
        }
        let refs: Vec<&str> = texts.iter().map(|t| t.as_str()).collect();
        Ok(FilterEngine::from_lists(&refs))
    }

    /// Build an engine from embedded texts plus optional files.
    /// brow (phase7.1): multi-builtin variant — EasyList and EasyPrivacy
    /// ship embedded together; file-based lists (user/pref updates) are
    /// ADDITIVE on top of EasyPrivacy so the tracker half can never be
    /// lost by a file override that only contains EasyList.
    pub fn engine_with_builtins(builtins: &[&str], extra: &[&Path]) -> std::io::Result<FilterEngine> {
        let mut texts: Vec<String> = builtins.iter().map(|b| (*b).to_string()).collect();
        for p in extra {
            texts.push(std::fs::read_to_string(p)?);
        }
        let refs: Vec<&str> = texts.iter().map(|t| t.as_str()).collect();
        Ok(FilterEngine::from_lists(&refs))
    }

    /// Build an engine from embedded text plus optional files.
    pub fn engine_with_builtin(builtin: &str, extra: &[&Path]) -> std::io::Result<FilterEngine> {
        engine_with_builtins(&[builtin], extra)
    }
}
