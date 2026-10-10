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

    /// brow (7.3): the embedded EasyList snapshot, owned here so every
    /// consumer (net stack, script thread, shell) shares ONE binary string.
    pub const EMBEDDED_EASYLIST: &str = include_str!("../assets/easylist-snapshot.txt");

    /// brow (7.3): the embedded EasyPrivacy snapshot (tracker half —
    /// phase 7.1). Same single-copy rationale.
    pub const EMBEDDED_EASYPRIVACY: &str = include_str!("../assets/easyprivacy-snapshot.txt");

    /// brow (7.4): uBlock filters (uAssets) — the uBlock Origin core
    /// filter set: anti-adblock circumvention, popup blocking, uBlock-
    /// specific rules that EasyList lacks. uBlock-specific extended
    /// syntax (scriptlets `##+js()`, procedural `#?#`) that the parser
    /// cannot apply is skipped as recorded-invalid — the valid subset
    /// (network + plain CSS) loads. GPL-3.0 (uAssets); list content is
    /// public data.
    pub const EMBEDDED_UBLOCK_FILTERS: &str = include_str!("../assets/ufilter-snapshot.txt");

    /// brow (7.4): Peter Lowe's ad-server domain list — compact
    /// high-signal domain blocklist (~7k domains), complements EasyList's
    /// pattern rules with plain domain blocks. Attribution requested by
    /// the author (see snapshot header + assets/ licences docs).
    pub const EMBEDDED_PETER_LOWES: &str = include_str!("../assets/plower-snapshot.txt");

    /// brow (7.3): process-global engine over the embedded snapshots.
    /// One instance per process, shared by the net stack (network
    /// decisions) and the script thread (cosmetic element hiding) —
    /// without this, the cosmetic wiring would build a second engine
    /// per content process and double the rule memory (phase 6).
    ///
    /// `None` = the embedded text failed to parse (unreachable in
    /// practice; logged at build of the lock).
    static GLOBAL_ENGINE: std::sync::OnceLock<Option<FilterEngine>> = std::sync::OnceLock::new();

    pub fn global_engine() -> Option<&'static FilterEngine> {
        // `from_lists` is infallible (parse failures become recorded stats,
        // not errors) — the Option models a future failure mode only.
        // brow (7.4): the full uBlock-class stack — EasyList (ads) +
        // EasyPrivacy (trackers) + uBlock filters (circumvention/popups)
        // + Peter Lowe's (domain blocks).
        GLOBAL_ENGINE
            .get_or_init(|| {
                Some(FilterEngine::from_lists(&[
                    EMBEDDED_EASYLIST,
                    EMBEDDED_EASYPRIVACY,
                    EMBEDDED_UBLOCK_FILTERS,
                    EMBEDDED_PETER_LOWES,
                ]))
            })
            .as_ref()
    }

    /// brow (7.3): the generic element-hiding plane as a CSS stylesheet
    /// body — one `display:none!important` rule per selector (per-rule
    /// isolation: a future bad parse can only kill its own rule, never
    /// the whole sheet). Empty when the engine is unavailable.
    pub fn generic_cosmetic_css() -> String {
        let Some(engine) = global_engine() else {
            return String::new();
        };
        let selectors = engine.cosmetic().generic_effective();
        let mut css = String::with_capacity(selectors.len() * 48);
        for sel in selectors {
            css.push_str(&sel);
            css.push_str("{display:none!important}\n");
        }
        css
    }

    /// brow (7.3): the domain-scoped element-hiding plane for one site
    /// host (lowercase), as a CSS body (same per-rule assembly). Empty
    /// when the engine is unavailable or nothing is scoped to the host.
    pub fn site_cosmetic_css(site_host: &str) -> String {
        let Some(engine) = global_engine() else {
            return String::new();
        };
        let result = engine.cosmetic().site_scoped_result(site_host);
        let selectors = result.effective();
        let mut css = String::with_capacity(selectors.len() * 48);
        for sel in selectors {
            css.push_str(&sel);
            css.push_str("{display:none!important}\n");
        }
        css
    }

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
