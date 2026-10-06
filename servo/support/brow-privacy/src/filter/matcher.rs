/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Hot-path matching: Aho-Corasick candidate generation over the lowercased
//! URL, followed by exact anchor/pattern verification. No regex compilation
//! at load time — patterns are matched piece-wise (`literal` / `*` / `^`)
//! with a bounded backtracking budget.

use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use memchr::memmem;

use super::rule::{Anchor, NetworkRule, PatternPiece};

/// Upper bound on backtracking steps for one pattern verification. EasyList
/// patterns carry at most a handful of `*` wildcards; the budget only exists
/// to make hostile patterns DoS-safe.
const MATCH_STEP_BUDGET: u32 = 8192;

/// Lowercase a URL quickly; ASCII fast path avoids an allocation.
pub fn lower_url(url: &str) -> std::borrow::Cow<'_, str> {
    if url.bytes().all(|b| !b.is_ascii_uppercase()) {
        std::borrow::Cow::Borrowed(url)
    } else if url.is_ascii() {
        std::borrow::Cow::Owned(url.to_ascii_lowercase())
    } else {
        std::borrow::Cow::Owned(url.to_lowercase())
    }
}

/// Byte offsets where a `||`-anchored pattern may legitimately start inside
/// the URL: the start of the authority and every dot boundary inside it.
pub fn authority_label_starts(url: &str) -> Vec<usize> {
    let mut starts = Vec::with_capacity(8);
    let Some(auth_start) = find_authority_start(url) else {
        return starts;
    };
    starts.push(auth_start);
    let auth_end = url[auth_start..]
        .find(['/', '?', '#'])
        .map(|i| auth_start + i)
        .unwrap_or(url.len());
    let mut i = auth_start;
    while i < auth_end {
        if url.as_bytes()[i] == b'.' && i + 1 < auth_end {
            starts.push(i + 1);
        }
        i += 1;
    }
    starts
}

fn find_authority_start(url: &str) -> Option<usize> {
    let scheme = url.find("://")?;
    let after = scheme + 3;
    let authority_zone_end = url[after..]
        .find(['/', '?', '#'])
        .map(|i| after + i)
        .unwrap_or(url.len());
    if let Some(at) = url[after..authority_zone_end].rfind('@') {
        Some(after + at + 1)
    } else {
        Some(after)
    }
}

/// Piece-wise matcher: consumes `url` from `pos`; literals use `memmem`,
/// wildcards backtrack over every position, separators check the ABP
/// boundary set `/?:=&.` or end-of-string. `must_end` pins the pattern to
/// the end of the URL (`$` anchor).
pub fn match_pieces(url: &str, pieces: &[PatternPiece], pos: usize, must_end: bool) -> bool {
    let mut budget = MATCH_STEP_BUDGET;
    match_pieces_inner(url, pieces, pos, must_end, &mut budget)
}

fn match_pieces_inner(
    url: &str,
    pieces: &[PatternPiece],
    pos: usize,
    must_end: bool,
    budget: &mut u32,
) -> bool {
    let bytes = url.as_bytes();
    if *budget == 0 {
        return false;
    }
    *budget -= 1;

    let mut i = 0usize;
    let mut pos = pos;
    while i < pieces.len() {
        if *budget == 0 {
            return false;
        }
        *budget -= 1;
        match &pieces[i] {
            PatternPiece::Literal(lit) => {
                if i == 0 {
                    // The leading literal is pinned to the anchor position
                    // (`||` label boundary, or the exact AC hit position).
                    if pos + lit.len() > bytes.len() || &bytes[pos..pos + lit.len()] != lit.as_bytes() {
                        return false;
                    }
                    pos += lit.len();
                    i += 1;
                    continue;
                }
                let finder = memmem::Finder::new(lit.as_bytes());
                match finder.find(&bytes[pos.min(bytes.len())..]) {
                    Some(rel) => {
                        pos = pos + rel + lit.len();
                        i += 1;
                    }
                    None => return false,
                }
            }
            PatternPiece::Wildcard => {
                let rest = &pieces[i + 1..];
                let mut try_pos = pos;
                loop {
                    if match_pieces_inner(url, rest, try_pos, must_end, budget) {
                        return true;
                    }
                    if try_pos >= bytes.len() || *budget == 0 {
                        return false;
                    }
                    *budget -= 1;
                    try_pos += 1;
                }
            }
            PatternPiece::Separator => {
                if pos >= bytes.len() {
                    // `^` also matches the end of the URL; pos stays pinned.
                    i += 1;
                    continue;
                }
                let ok = matches!(
                    bytes[pos],
                    b'/' | b'?' | b':' | b'=' | b'&' | b'.'
                );
                if !ok {
                    return false;
                }
                pos += 1;
                i += 1;
            }
        }
    }
    if must_end {
        pos == bytes.len()
    } else {
        true
    }
}

/// Full verification for a candidate rule against a lowercased URL.
/// `hit_pos` is the AC prefilter hit for the first literal (or None when the
/// rule was reached without a prefilter, e.g. pure-wildcard fallback rules).
pub fn verify(url: &str, rule: &NetworkRule, hit_pos: Option<usize>) -> bool {
    match rule.anchor {
        Anchor::DoubleAnchor => {
            let starts = authority_label_starts(url);
            starts.into_iter().any(|s| match_pieces(url, &rule.pattern, s, ends_pinned(rule)))
        }
        Anchor::Start => match_pieces(url, &rule.pattern, 0, ends_pinned(rule)),
        Anchor::StartEnd => match_pieces(url, &rule.pattern, 0, true),
        Anchor::End => {
            let Some(p) = hit_pos else { return false };
            match_pieces(url, &rule.pattern, p, true)
        }
        Anchor::None => {
            let Some(p) = hit_pos else { return false };
            match_pieces(url, &rule.pattern, p, false)
        }
    }
}

fn ends_pinned(rule: &NetworkRule) -> bool {
    matches!(rule.anchor, Anchor::End | Anchor::StartEnd)
}

/// Position of the first non-empty literal piece — the AC prefilter needle.
pub fn first_literal(pattern: &[PatternPiece]) -> Option<&str> {
    pattern.iter().find_map(|p| match p {
        PatternPiece::Literal(l) if !l.is_empty() => Some(l.as_str()),
        _ => None,
    })
}

/// Host hint for `||` rules: the leading literal run before the first
/// wildcard/separator piece — used as the AC needle for the host automaton.
pub fn host_hint(pattern: &[PatternPiece]) -> Option<String> {
    let mut hint = String::new();
    for p in pattern {
        match p {
            PatternPiece::Literal(l) => hint.push_str(l),
            _ => break,
        }
    }
    if hint.is_empty() {
        None
    } else {
        Some(hint)
    }
}

/// Build an AhoCorasick automaton from needles.
///
/// `MatchKind::Standard` is REQUIRED here: the automaton is a candidate
/// *generator*, so every match (including overlapping ones) must be
/// reported. `LeftmostFirst` would silently swallow overlapping needles —
/// e.g. `/gpt.js$script` is shadowed by another rule's `gpt.js` hit.
pub fn build_ac(needles: &[String]) -> AhoCorasick {
    AhoCorasickBuilder::new()
        .match_kind(MatchKind::Standard)
        .build(needles.iter().map(|n| n.as_str()))
        .expect("aho-corasick build cannot fail on valid needles")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces(pattern: &str) -> Vec<PatternPiece> {
        crate::filter::parser::tokenize_pattern(pattern)
    }

    fn rule(pattern: &str, anchor: Anchor) -> NetworkRule {
        NetworkRule {
            kind: crate::filter::rule::FilterKind::Block,
            important: false,
            party: crate::filter::rule::PartyScope::Any,
            types: None,
            not_types: crate::filter::rule::ResourceTypeMask::empty(),
            include_domains: Vec::new(),
            exclude_domains: Vec::new(),
            sitekey_present: false,
            has_unsupported_options: false,
            anchor,
            pattern: pieces(pattern),
            regex: None,
            raw: pattern.to_string(),
        }
    }

    #[test]
    fn label_starts() {
        let url = "https://ads.example.com/path?q=1";
        let starts = authority_label_starts(url);
        let auth = url.find("ads.").unwrap();
        assert!(starts.contains(&auth));
        assert!(starts.contains(&url.find("example.").unwrap()));
        assert!(starts.contains(&url.find("com").unwrap()));
        assert_eq!(starts[0], "https://".len());
    }

    #[test]
    fn userinfo_is_skipped() {
        let url = "https://user@s.example.com/x";
        let starts = authority_label_starts(url);
        assert_eq!(starts[0], url.find("s.example.com").unwrap());
    }

    #[test]
    fn double_anchor_match() {
        let r = rule("ads.example.com^", Anchor::DoubleAnchor);
        assert!(verify(
            lower_url("https://ads.example.com/pixel.js").as_ref(),
            &r,
            None
        ));
        assert!(!verify(
            lower_url("https://notads.example.com/pixel.js").as_ref(),
            &r,
            None
        ));
    }

    #[test]
    fn separator_matches_end() {
        let r = rule("ads.example.com^", Anchor::DoubleAnchor);
        assert!(verify(
            lower_url("https://ads.example.com").as_ref(),
            &r,
            None
        ));
    }

    #[test]
    fn wildcard_backtrack() {
        let r = rule("/ad*/*banner*", Anchor::None);
        let url = lower_url("https://x.com/ad/img/small/banner/big.png");
        assert!(verify(url.as_ref(), &r, url.find("/ad")));
        let r2 = rule("foo*bar*baz", Anchor::None);
        let url2 = lower_url("https://q.com/xxfooyybarzzbazww");
        assert!(verify(url2.as_ref(), &r2, url2.find("foo")));
        // budget-bounded miss
        let url3 = lower_url("https://q.com/fooybar");
        assert!(!verify(url3.as_ref(), &r2, url3.find("foo")));
    }

    #[test]
    fn start_end_anchor() {
        let r = rule("https://a.com/b^", Anchor::StartEnd);
        assert!(verify("https://a.com/b/", &r, None));
        assert!(!verify("https://a.com/b/c", &r, None));
    }

    #[test]
    fn end_anchor_pins_tail() {
        let r = rule(".swf", Anchor::End);
        let hit = "https://x.com/a/b.swf".find(".swf");
        assert!(verify("https://x.com/a/b.swf", &r, hit));
        assert!(!verify("https://x.com/a/b.swf?q", &r, hit));
    }
}
