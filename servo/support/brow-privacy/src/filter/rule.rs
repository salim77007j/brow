/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Typed representation of parsed filter rules.

use serde::{Deserialize, Serialize};

/// Bitmask of resource destinations a network filter can apply to,
/// mirroring the ABP `$type` options used by EasyList.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceTypeMask(pub u32);

impl ResourceTypeMask {
    pub const OTHER: u32 = 1 << 0;
    pub const SCRIPT: u32 = 1 << 1;
    pub const IMAGE: u32 = 1 << 2;
    pub const STYLESHEET: u32 = 1 << 3;
    pub const OBJECT: u32 = 1 << 4;
    pub const SUBDOCUMENT: u32 = 1 << 5;
    pub const DOCUMENT: u32 = 1 << 6;
    pub const XHR: u32 = 1 << 7;
    pub const WEBSOCKET: u32 = 1 << 8;
    pub const MEDIA: u32 = 1 << 9;
    pub const FONT: u32 = 1 << 10;
    pub const PING: u32 = 1 << 11;
    pub const POPUP: u32 = 1 << 12;
    pub const WEBRTC: u32 = 1 << 13;

    /// Every type we can currently observe from the fetch pipeline.
    pub const ALL_OBSERVABLE: u32 = Self::OTHER
        | Self::SCRIPT
        | Self::IMAGE
        | Self::STYLESHEET
        | Self::OBJECT
        | Self::SUBDOCUMENT
        | Self::DOCUMENT
        | Self::XHR
        | Self::WEBSOCKET
        | Self::MEDIA
        | Self::FONT
        | Self::PING;

    pub const fn empty() -> Self {
        ResourceTypeMask(0)
    }

    pub const fn all_observable() -> Self {
        ResourceTypeMask(Self::ALL_OBSERVABLE)
    }

    pub const fn contains(self, bit: u32) -> bool {
        self.0 & bit != 0
    }

    pub const fn union(self, other: ResourceTypeMask) -> Self {
        ResourceTypeMask(self.0 | other.0)
    }

    /// Map a fetch-pipeline destination string onto the mask.
    pub fn from_destination(dest: &str) -> u32 {
        match dest {
            "script" => Self::SCRIPT,
            "img" => Self::IMAGE,
            "style" => Self::STYLESHEET,
            "object" => Self::OBJECT,
            "iframe" | "frame" => Self::SUBDOCUMENT,
            "document" => Self::DOCUMENT,
            "xhr" | "fetch" => Self::XHR,
            "websocket" => Self::WEBSOCKET,
            "media" => Self::MEDIA,
            "font" => Self::FONT,
            "ping" | "beacon" => Self::PING,
            "webrc" | "webrtc" => Self::WEBRTC,
            _ => Self::OTHER,
        }
    }
}

impl std::ops::BitOr for ResourceTypeMask {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        ResourceTypeMask(self.0 | rhs.0)
    }
}

/// An anchored pattern: how the pattern is pinned inside the URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Anchor {
    /// No anchor — plain substring match.
    None,
    /// `||host^` — matches at any hostname label boundary of the URL prefix.
    DoubleAnchor,
    /// `|pat` — matches at the very start of the URL.
    Start,
    /// `pat|` — matches at the very end of the URL.
    End,
    /// `|pat|` — full-URL equality.
    StartEnd,
}

/// Where a `^` separator marker may appear inside a pattern.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatternPiece {
    /// Literal text (already lowercased, percent-decoding handled at match time).
    Literal(String),
    /// `*` wildcard — any run of characters (may be empty).
    Wildcard,
    /// `^` separator — one of `/?:=&.` plus end-of-URL.
    Separator,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilterKind {
    /// Plain blocking rule.
    #[default]
    Block,
    /// `@@` exception rule.
    Exception,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartyScope {
    /// No `$third-party` / `$~third-party` option — applies to both.
    #[default]
    Any,
    /// `$third-party`
    ThirdPartyOnly,
    /// `$~third-party` / `$first-party`
    FirstPartyOnly,
}

/// A parsed network filter (blocking or exception).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NetworkRule {
    pub kind: FilterKind,
    /// `$important`
    pub important: bool,
    pub party: PartyScope,
    /// Resource-type restriction; `None` means every type.
    pub types: Option<ResourceTypeMask>,
    /// Types explicitly negated with `~` inside a mixed type list.
    pub not_types: ResourceTypeMask,
    /// `$domain=a|~b` (aliases: `$from=`) — include/exclude site list.
    pub include_domains: Vec<String>,
    pub exclude_domains: Vec<String>,
    /// `$sitekey=` — surfaced for parity but not evaluated (no ABP signing in brow).
    pub sitekey_present: bool,
    /// True when the rule has options the engine ignores (documented drop).
    pub has_unsupported_options: bool,
    pub anchor: Anchor,
    pub pattern: Vec<PatternPiece>,
    /// `/regex/`-delimited pattern (ABP regex rules): the raw regex source.
    /// Matched case-sensitively against the raw URL by the `regex` crate.
    pub regex: Option<String>,
    /// Original text for diagnostics.
    pub raw: String,
}

impl NetworkRule {
    /// Whether the pattern is a bare host pattern (`||host` optionally with
    /// one trailing `^`). These feed the fast hostname index.
    pub fn is_pure_host_pattern(&self) -> bool {
        if self.anchor != Anchor::DoubleAnchor || self.pattern.is_empty() {
            return false;
        }
        let body = match self.pattern.last() {
            Some(PatternPiece::Separator) => &self.pattern[..self.pattern.len() - 1],
            _ => &self.pattern[..],
        };
        !body.is_empty()
            && body.iter().all(|p| matches!(p, PatternPiece::Literal(_)))
    }

    /// For pure host patterns: the host body (concatenated literals).
    pub fn pure_host(&self) -> Option<String> {
        if !self.is_pure_host_pattern() {
            return None;
        }
        let mut s = String::new();
        for p in &self.pattern {
            if let PatternPiece::Literal(l) = p {
                s.push_str(l);
            }
        }
        Some(s)
    }
}

/// A parsed element-hiding (cosmetic) rule.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CosmeticRule {
    /// `##selector` — hide on every site.
    GenericHide { selector: String, raw: String },
    /// `example.com,~other.org##selector` — hide on matching sites.
    DomainHide {
        include: Vec<String>,
        exclude: Vec<String>,
        selector: String,
        raw: String,
    },
    /// `example.com#@#selector` — unhide (exception) on matching sites.
    DomainUnhide {
        include: Vec<String>,
        exclude: Vec<String>,
        selector: String,
        raw: String,
    },
    /// `example.com#@#` prefix + procedural `#?#` body — recorded, not applied.
    Procedural {
        include: Vec<String>,
        exclude: Vec<String>,
        raw: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ParsedFilter {
    Network(NetworkRule),
    Cosmetic(CosmeticRule),
}

/// Aggregate parse outcome statistics — surfaced in reports and the
/// `FilterEngine::stats()` snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseStats {
    pub lines_total: usize,
    pub network_block: usize,
    pub network_exception: usize,
    pub cosmetic_generic: usize,
    pub cosmetic_domain: usize,
    pub cosmetic_unhide: usize,
    pub cosmetic_procedural: usize,
    pub regex_rules: usize,
    pub comments: usize,
    pub empty: usize,
    pub unsupported: usize,
    pub invalid: usize,
}
