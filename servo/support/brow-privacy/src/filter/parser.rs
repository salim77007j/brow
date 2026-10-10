/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Parser for EasyList / Adblock Plus filter syntax into typed rules.
//!
//! Supported (real, evaluated by the engine):
//! - network patterns with `||`, `|`, `^`, `*` and literal text
//! - `@@` exceptions, `$important`
//! - `$third-party`, `$~third-party`, `$first-party`
//! - `$domain=` / `$from=` include/exclude lists incl. entity wildcards (`example.*`)
//! - resource types: script, image, stylesheet, object, xmlhttprequest,
//!   subdocument, document, websocket, webrtc, media, font, other, ping, popup
//! - element hiding: `##`, domain-scoped `example.com##`, exceptions `#@#`,
//!   procedural `#?#` (recorded, not applied)
//! - `$badfilter` mutual cancellation
//! - hosts-file style lines (`0.0.0.0 tracker.example`)
//!
//! Recognised but NOT evaluated (dropped, counted in [`ParseStats::unsupported`]):
//! `$csp=`, `$replace=`, `$redirect`, `$redirect-rule=`, `$removeparam=`,
//! `$jsonprune=`, `$dnsrewrite=`, `$uritransform=`, `$sitekey=` (recorded),
//! snippet filters (`#$#`, `#@$#`), response filters (`#^#`).

use super::rule::{
    Anchor, CosmeticRule, FilterKind, NetworkRule, ParsedFilter, PartyScope, PatternPiece,
    ParseStats, ResourceTypeMask,
};

/// Options the engine recognises but cannot honestly evaluate. Rules carrying
/// them are dropped and counted, never silently half-applied.
const UNSUPPORTED_OPTS: &[&str] = &[
    "csp",
    "replace",
    "redirect",
    "redirect-rule",
    "removeparam",
    "jsonprune",
    "dnsrewrite",
    "uritransform",
    "popupfilter",
];

/// Types that the fetch pipeline cannot observe; kept in the mask so list
/// semantics stay intact but they never match a request we evaluate.
const KNOWN_TYPES: &[(&str, u32)] = &[
    ("script", ResourceTypeMask::SCRIPT),
    ("image", ResourceTypeMask::IMAGE),
    ("stylesheet", ResourceTypeMask::STYLESHEET),
    ("object", ResourceTypeMask::OBJECT),
    ("xmlhttprequest", ResourceTypeMask::XHR),
    ("subdocument", ResourceTypeMask::SUBDOCUMENT),
    ("document", ResourceTypeMask::DOCUMENT),
    ("websocket", ResourceTypeMask::WEBSOCKET),
    ("webrtc", ResourceTypeMask::WEBRTC),
    ("media", ResourceTypeMask::MEDIA),
    ("font", ResourceTypeMask::FONT),
    ("other", ResourceTypeMask::OTHER),
    ("ping", ResourceTypeMask::PING),
    ("popup", ResourceTypeMask::POPUP),
];

/// A single filter line, parsed.
///
/// Returns `Ok(None)` for comments/blank lines (already counted by the caller).
pub fn parse_line(line: &str, stats: &mut ParseStats) -> Result<Option<ParsedFilter>, String> {
    stats.lines_total += 1;
    let trimmed = line.trim();
    if trimmed.is_empty() {
        stats.empty += 1;
        return Ok(None);
    }
    if trimmed.starts_with('!') || trimmed.starts_with('[') || trimmed.starts_with("!+") {
        stats.comments += 1;
        return Ok(None);
    }

    if is_hosts_line(trimmed) {
        let domain = trimmed
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| "hosts line without domain".to_string())?;
        let lowered = domain.to_ascii_lowercase();
        if lowered.is_empty() {
            stats.invalid += 1;
            return Ok(None);
        }
        let rule = NetworkRule {
            kind: FilterKind::Block,
            important: false,
            party: PartyScope::Any,
            types: None,
            not_types: ResourceTypeMask::empty(),
            include_domains: Vec::new(),
            exclude_domains: Vec::new(),
            sitekey_present: false,
            has_unsupported_options: false,
            hide_only: false,
            anchor: Anchor::DoubleAnchor,
            pattern: vec![PatternPiece::Literal(lowered.clone())],
            regex: None,
            raw: trimmed.to_string(),
        };
        stats.network_block += 1;
        return Ok(Some(ParsedFilter::Network(rule)));
    }

    // Element hiding? If any cosmetic marker is present, the line is a
    // cosmetic rule — never fall through to network parsing (snippet
    // filters `#$#` etc. are dropped here with an unsupported count).
    if has_cosmetic_marker(trimmed) {
        return parse_cosmetic(trimmed, stats).map(|c| c.map(ParsedFilter::Cosmetic));
    }

    parse_network(trimmed, stats)
}

/// Element-hiding markers (subset that decides the parse path).
fn has_cosmetic_marker(line: &str) -> bool {
    COSMETIC_MARKERS.iter().any(|m| line.contains(m))
}

fn is_hosts_line(line: &str) -> bool {
    let mut parts = line.split_whitespace();
    let Some(first) = parts.next() else {
        return false;
    };
    // IP literal, then at least a domain token, and the line has no ABP syntax.
    (first == "0.0.0.0"
        || first == "127.0.0.1"
        || first.parse::<std::net::IpAddr>().is_ok())
        && parts.next().is_some()
        && !line.contains('#')
        && !line.contains('$')
}

// ---------------------------------------------------------------------------
// Network rules
// ---------------------------------------------------------------------------

fn parse_network(line: &str, stats: &mut ParseStats) -> Result<Option<ParsedFilter>, String> {
    let (kind, body) = if let Some(rest) = line.strip_prefix("@@") {
        (FilterKind::Exception, rest)
    } else {
        (FilterKind::Block, line)
    };

    // Split pattern / options at the last '$' that yields a fully valid
    // option list; otherwise '$' belongs to the pattern (e.g. query strings).
    let (pattern_raw, options_raw) = split_options(body);

    let mut rule = NetworkRule {
        kind,
        important: false,
        party: PartyScope::Any,
        types: None,
        not_types: ResourceTypeMask::empty(),
        include_domains: Vec::new(),
        exclude_domains: Vec::new(),
        sitekey_present: false,
        has_unsupported_options: false,
        hide_only: false,
        anchor: Anchor::None,
        pattern: Vec::new(),
        regex: None,
        raw: line.to_string(),
    };

    if let Some(opts) = options_raw {
        if !apply_options(opts, &mut rule, stats)? {
            // only-unsupported semantics: drop the rule entirely
            // (stats.unsupported already counted by apply_options)
            return Ok(None);
        }
    }

    let mut pattern = pattern_raw;
    // `/regex/`-delimited ABP patterns — only when genuine regex constructs
    // appear inside (plain `/path/` stays a literal substring pattern).
    if pattern.len() >= 3
        && pattern.starts_with('/')
        && pattern.ends_with('/')
        && pattern[1..pattern.len() - 1].chars().any(|c| matches!(c, '\\' | '{' | '}' | '(' | ')' | '|' | '+' | '?' | '[' | ']'))
    {
        let src = &pattern[1..pattern.len() - 1];
        match regex::Regex::new(src) {
            Ok(_) => {
                rule.regex = Some(src.to_string());
                stats.regex_rules += 1;
                if rule.kind == FilterKind::Exception {
                    stats.network_exception += 1;
                } else {
                    stats.network_block += 1;
                }
                return Ok(Some(ParsedFilter::Network(rule)));
            }
            Err(_) => {
                // Constructs the `regex` crate cannot honour (JS lookaheads
                // etc.) — dropped and counted, never half-applied.
                stats.unsupported += 1;
                return Ok(None);
            }
        }
 }
    let mut end_anchor = false;
    if pattern.starts_with("||") {
        rule.anchor = Anchor::DoubleAnchor;
        pattern = &pattern[2..];
    } else if let Some(rest) = pattern.strip_prefix('|') {
        rule.anchor = Anchor::Start;
        pattern = rest;
    }
    if pattern.ends_with('|') && !pattern.ends_with("\\|") {
        end_anchor = true;
        pattern = &pattern[..pattern.len() - 1];
    }
    if rule.anchor == Anchor::Start && end_anchor {
        rule.anchor = Anchor::StartEnd;
    } else if end_anchor {
        rule.anchor = Anchor::End;
    }

    if pattern.is_empty() && rule.anchor == Anchor::None && rule.regex.is_none() {
        stats.invalid += 1;
        return Ok(None);
    }

    rule.pattern = if rule.regex.is_some() {
        Vec::new()
    } else {
        let p = tokenize_pattern(pattern);
        if p.is_empty() {
            stats.invalid += 1;
            return Ok(None);
        }
        p
    };

    if rule.kind == FilterKind::Exception {
        stats.network_exception += 1;
    } else {
        stats.network_block += 1;
    }
    Ok(Some(ParsedFilter::Network(rule)))
}

/// Option vocabulary — a trailing `$...` segment is only treated as an
/// option list when every token is a known option name (this is also how
/// the pattern/option boundary is disambiguated when `$` appears in URLs).
const KNOWN_OPTION_NAMES: &[&str] = &[
    "domain", "from", "third-party", "3p", "first-party", "1p", "important",
    "match-case", "sitekey", "badfilter", "script", "image", "stylesheet",
    "object", "xmlhttprequest", "subdocument", "document", "websocket",
    "webrtc", "media", "font", "other", "ping", "popup", "generichide",
    "elemhide", "genericblock", "strict3p", "inline-font", "inline-script",
    "csp", "replace", "redirect", "redirect-rule", "removeparam", "jsonprune",
    "dnsrewrite", "uritransform",
];

fn is_known_option(token: &str) -> bool {
    let name = token.split('=').next().unwrap_or("").trim_start_matches('~');
    !name.is_empty() && KNOWN_OPTION_NAMES.contains(&name.to_ascii_lowercase().as_str())
}

/// Returns `(pattern, Some(options))` or `(whole, None)`.
fn split_options(body: &str) -> (&str, Option<&str>) {
    let Some(dollar) = body.rfind('$') else {
        return (body, None);
    };
    let (pattern, opts) = (&body[..dollar], &body[dollar + 1..]);
    if opts.is_empty() {
        return (body, None);
    }
    // Every comma-separated token must be a known option; otherwise the
    // `$` is part of the pattern (query strings etc.).
    if !opts.split(',').all(is_known_option) {
        return (body, None);
    }
    (pattern, Some(opts))
}

/// Applies `$` options. Returns `false` when the rule must be dropped because
/// every option was unsupported (a rule we cannot evaluate at all).
fn apply_options(opts: &str, rule: &mut NetworkRule, stats: &mut ParseStats) -> Result<bool, String> {
    let mut saw_supported = false;
    let mut saw_unsupported = false;
    let mut type_bits = ResourceTypeMask::empty();
    let mut saw_type_option = false;

    for token in opts.split(',') {
        let token = token.trim();
        let (negated, token) = if let Some(t) = token.strip_prefix('~') {
            (true, t)
        } else {
            (false, token)
        };
        let (name, value) = match token.split_once('=') {
            Some((n, v)) => (n, Some(v)),
            None => (token, None),
        };
        let name_lower = name.to_ascii_lowercase();

        match name_lower.as_str() {
            "third-party" | "3p" => {
                rule.party = if negated {
                    PartyScope::FirstPartyOnly
                } else {
                    PartyScope::ThirdPartyOnly
                };
                saw_supported = true;
            }
            "first-party" | "1p" => {
                rule.party = if negated {
                    PartyScope::ThirdPartyOnly
                } else {
                    PartyScope::FirstPartyOnly
                };
                saw_supported = true;
            }
            "important" | "imp" if !negated => {
                rule.important = true;
                saw_supported = true;
            }
            "domain" | "from" => {
                let Some(value) = value else {
                    stats.invalid += 1;
                    return Err("$domain without value".to_string());
                };
                for entry in value.split('|') {
                    let entry = entry.trim().to_ascii_lowercase();
                    if entry.is_empty() {
                        continue;
                    }
                    if let Some(excl) = entry.strip_prefix('~') {
                        rule.exclude_domains.push(excl.to_string());
                    } else {
                        rule.include_domains.push(entry);
                    }
                }
                saw_supported = true;
            }
            "match-case" => {
                // Engine matches case-insensitively (documented approximation).
                saw_supported = true;
            }
            "sitekey" => {
                rule.sitekey_present = true;
                saw_unsupported = true;
            }
            "badfilter" if !negated => {
                // marker handled by the engine via raw text; keep rule flagged
                rule.has_unsupported_options = false;
                saw_supported = true;
                rule.include_domains.push("\u{1}badfilter".to_string());
            }
            _ => {
                if let Some((_, bit)) = KNOWN_TYPES.iter().find(|(n, _)| *n == name_lower) {
                    saw_type_option = true;
                    if negated {
                        rule.not_types = rule.not_types.union(ResourceTypeMask(*bit));
                    } else {
                        type_bits = type_bits.union(ResourceTypeMask(*bit));
                    }
                    saw_supported = true;
                } else if UNSUPPORTED_OPTS.contains(&name_lower.as_str()) {
                    saw_unsupported = true;
                } else if matches!(name_lower.as_str(), "generichide" | "elemhide" | "genericblock" | "strict3p" | "inline-font" | "inline-script") {
                    // brow (7.2): document-level page-behavior flags. They
                    // describe what may happen ON the page (element hiding
                    // allowed, generic blocking disabled, inline scripts
                    // blocked) — they NEVER except an individual network
                    // request. Exception rules carrying them are marked
                    // hide_only so network decisions skip them (CI diag:
                    // @@||facebook.com^$generichide whitelisted the /tr/
                    // beacon). Blocking rules with page flags are dropped
                    // as unreliable, as before.
                    saw_supported = true;
                    if rule.kind == FilterKind::Block {
                        saw_unsupported = true;
                    } else {
                        rule.hide_only = true;
                    }
                } else {
                    // Unknown option — do not pretend to understand it.
                    saw_unsupported = true;
                }
            }
        }
    }

    if saw_type_option {
        rule.types = Some(if type_bits.0 == 0 { ResourceTypeMask::empty() } else { type_bits });
    }

    if saw_unsupported {
        stats.unsupported += 1;
        rule.has_unsupported_options = true;
    }
    // Drop only when NOTHING in the rule is actionable (e.g. `$csp=...` alone):
    // such rules would otherwise block pages or do nothing, both wrong.
    Ok(saw_supported || rule.kind == FilterKind::Block && rule.include_domains.is_empty() && !saw_unsupported)
}

/// Splits a raw pattern into literal / wildcard / separator pieces.
pub fn tokenize_pattern(pattern: &str) -> Vec<PatternPiece> {
    let mut pieces: Vec<PatternPiece> = Vec::new();
    let mut lit = String::new();
    let lower = pattern.to_ascii_lowercase();
    let mut chars = lower.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => {
                if !lit.is_empty() {
                    pieces.push(PatternPiece::Literal(std::mem::take(&mut lit)));
                }
                if !matches!(pieces.last(), Some(PatternPiece::Wildcard)) {
                    pieces.push(PatternPiece::Wildcard);
                }
            }
            '^' => {
                if !lit.is_empty() {
                    pieces.push(PatternPiece::Literal(std::mem::take(&mut lit)));
                }
                pieces.push(PatternPiece::Separator);
            }
            '\\' if chars.peek() == Some(&'|') => {
                // escaped pipe — literal '|'
                lit.push('|');
                chars.next();
            }
            _ => lit.push(c),
        }
    }
    if !lit.is_empty() {
        pieces.push(PatternPiece::Literal(lit));
    }
    pieces
}

// ---------------------------------------------------------------------------
// Cosmetic rules
// ---------------------------------------------------------------------------

/// Cosmetic separator markers, longest first so `#@%#` wins over `#%#` etc.
const COSMETIC_MARKERS: &[&str] = &[
    "#@$#", "#@%#", "#@#", "#$#", "#%#", "#?#", "##", "#^#",
];

/// Basic CSS-selector sanity check: non-empty, no braces (blocks `{}`
/// injection from malformed or hostile lists), no markup. `>` is a valid
/// child combinator and is allowed; selectors that would throw inside
/// `querySelectorAll` (e.g. EasyList's known-malformed `##ref^=` lines)
/// are rejected here and counted as invalid at parse time.
fn selector_is_sane(sel: &str) -> bool {
    !sel.is_empty()
        && sel.len() <= 8192
        && !sel.contains('{')
        && !sel.contains('}')
        && !sel.contains('<')
        && sel.chars().all(|c| c.is_alphanumeric() || "!\"#$%&()*+,-./:;=>?@[]^_`|~' \t\\".contains(c))
}

fn split_domains(domain_part: &str) -> (Vec<String>, Vec<String>) {
    let mut include = Vec::new();
    let mut exclude = Vec::new();
    for d in domain_part.split(',') {
        let d = d.trim().to_ascii_lowercase();
        if d.is_empty() {
            continue;
        }
        if let Some(excl) = d.strip_prefix('~') {
            exclude.push(excl.to_string());
        } else {
            include.push(d);
        }
    }
    (include, exclude)
}

fn parse_cosmetic(line: &str, stats: &mut ParseStats) -> Result<Option<CosmeticRule>, String> {
    // Find earliest marker occurrence.
    let mut best: Option<(usize, &'static str)> = None;
    for marker in COSMETIC_MARKERS {
        if let Some(pos) = line.find(marker) {
            match best {
                Some((p, _)) if p <= pos => {}
                _ => best = Some((pos, marker)),
            }
        }
    }
    let Some((pos, marker)) = best else {
        return Ok(None);
    };

    let domain_part = &line[..pos];
    let rest = &line[pos + marker.len()..];

    match marker {
        "##" => {
            if domain_part.is_empty() {
                if selector_is_sane(rest) {
                    stats.cosmetic_generic += 1;
                    Ok(Some(CosmeticRule::GenericHide {
                        selector: rest.to_string(),
                        raw: line.to_string(),
                    }))
                } else {
                    stats.invalid += 1;
                    Err(format!("invalid generic selector: {rest}"))
                }
            } else {
                let (include, exclude) = split_domains(domain_part);
                if include.is_empty() {
                    stats.invalid += 1;
                    return Err("domain rule without include domains".to_string());
                }
                if !selector_is_sane(rest) {
                    stats.invalid += 1;
                    return Err(format!("invalid selector: {rest}"));
                }
                stats.cosmetic_domain += 1;
                Ok(Some(CosmeticRule::DomainHide {
                    include,
                    exclude,
                    selector: rest.to_string(),
                    raw: line.to_string(),
                }))
            }
        }
        "#@#" => {
            let (include, exclude) = split_domains(domain_part);
            if !selector_is_sane(rest) {
                stats.invalid += 1;
                return Err(format!("invalid unhide selector: {rest}"));
            }
            stats.cosmetic_unhide += 1;
            Ok(Some(CosmeticRule::DomainUnhide {
                include,
                exclude,
                selector: rest.to_string(),
                raw: line.to_string(),
            }))
        }
        "#?#" => {
            let (include, exclude) = split_domains(domain_part);
            stats.cosmetic_procedural += 1;
            Ok(Some(CosmeticRule::Procedural {
                include,
                exclude,
                raw: line.to_string(),
            }))
        }
        // snippet / response filters — unsupported, dropped
        _ => {
            stats.unsupported += 1;
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> Result<Option<ParsedFilter>, String> {
        let mut st = ParseStats::default();
        parse_line(s, &mut st)
    }

    fn net(s: &str) -> NetworkRule {
        match parse(s).expect("parse ok").expect("not comment") {
            ParsedFilter::Network(r) => r,
            other => panic!("expected network rule, got {other:?}"),
        }
    }

    #[test]
    fn parses_double_anchor_with_options() {
        let r = net("||ads.example.com^$script,third-party,domain=foo.com|~bar.foo.com");
        assert_eq!(r.kind, FilterKind::Block);
        assert_eq!(r.anchor, Anchor::DoubleAnchor);
        assert_eq!(r.party, PartyScope::ThirdPartyOnly);
        assert_eq!(r.include_domains, vec!["foo.com".to_string()]);
        assert_eq!(r.exclude_domains, vec!["bar.foo.com".to_string()]);
        assert!(r.types.unwrap().contains(ResourceTypeMask::SCRIPT));
        assert_eq!(r.pure_host().as_deref(), Some("ads.example.com"));
    }

    #[test]
    fn parses_exception_and_important() {
        let e = net("@@||good.example/notanad^$important");
        assert_eq!(e.kind, FilterKind::Exception);
        assert!(e.important);
        assert_eq!(e.anchor, Anchor::DoubleAnchor);
    }

    #[test]
    fn dollar_in_pattern_is_not_options() {
        let r = net("/track?$pixel");
        assert!(r.options_none_checker());
        assert!(r.pattern.iter().any(|p| matches!(
            p,
            PatternPiece::Literal(l) if l.contains("$pixel")
        )));
    }

    #[test]
    fn unsupported_only_rules_are_dropped() {
        let mut st = ParseStats::default();
        let r = parse_line("/ad/$csp=script-src 'none'", &mut st).unwrap();
        assert!(r.is_none(), "csp-only rule must be dropped");
        assert_eq!(st.unsupported, 1);
    }

    #[test]
    fn separator_and_wildcard_tokenize() {
        let r = net("/ads/banner*.gif^");
        assert_eq!(r.anchor, Anchor::None);
        assert!(r.pattern.contains(&PatternPiece::Wildcard));
        assert!(r.pattern.contains(&PatternPiece::Separator));
        assert!(r
            .pattern
            .iter()
            .any(|p| matches!(p, PatternPiece::Literal(l) if l == "/ads/banner")));
    }

    #[test]
    fn end_anchor() {
        let r = net("swf|");
        assert_eq!(r.anchor, Anchor::End);
    }

    #[test]
    fn cosmetic_generic_and_domain() {
        let mut st = ParseStats::default();
        match parse_line("##.ad-banner", &mut st).unwrap().unwrap() {
            ParsedFilter::Cosmetic(CosmeticRule::GenericHide { selector, .. }) => {
                assert_eq!(selector, ".ad-banner");
            }
            _ => panic!(),
        }
        match parse_line("example.com,~sub.example.com##div#promo", &mut st)
            .unwrap()
            .unwrap()
        {
            ParsedFilter::Cosmetic(CosmeticRule::DomainHide {
                include, exclude, ..
            }) => {
                assert_eq!(include, vec!["example.com".to_string()]);
                assert_eq!(exclude, vec!["sub.example.com".to_string()]);
            }
            _ => panic!(),
        }
        assert_eq!(st.cosmetic_generic, 1);
        assert_eq!(st.cosmetic_domain, 1);
    }

    #[test]
    fn cosmetic_unhide_and_procedural() {
        let mut st = ParseStats::default();
        match parse_line("example.com#@#.ad", &mut st).unwrap().unwrap() {
            ParsedFilter::Cosmetic(CosmeticRule::DomainUnhide { include, .. }) => {
                assert_eq!(include, vec!["example.com".to_string()]);
            }
            _ => panic!(),
        }
        match parse_line("example.com#?#div:has-text(AD)", &mut st).unwrap().unwrap() {
            ParsedFilter::Cosmetic(CosmeticRule::Procedural { .. }) => {}
            _ => panic!(),
        }
        assert_eq!(st.cosmetic_unhide, 1);
        assert_eq!(st.cosmetic_procedural, 1);
    }

    #[test]
    fn snippet_filters_are_unsupported() {
        let mut st = ParseStats::default();
        let r = parse_line("example.com#$#abort-on-property-read ad", &mut st).unwrap();
        assert!(r.is_none());
        assert_eq!(st.unsupported, 1);
    }

    #[test]
    fn comments_and_empty() {
        let mut st = ParseStats::default();
        assert!(parse_line("", &mut st).unwrap().is_none());
        assert!(parse_line("! comment", &mut st).unwrap().is_none());
        assert!(parse_line("[Adblock Plus 2.0]", &mut st).unwrap().is_none());
        assert_eq!(st.comments, 2);
        assert_eq!(st.empty, 1);
    }

    #[test]
    fn hosts_file_line() {
        let r = net("0.0.0.0 tracker.example.com");
        assert_eq!(r.anchor, Anchor::DoubleAnchor);
        assert_eq!(r.pure_host().as_deref(), Some("tracker.example.com"));
    }

    #[test]
    fn entity_tld_wildcard_domain_option() {
        let r = net("||ads.example.*^$domain=example.*");
        assert_eq!(r.include_domains, vec!["example.*".to_string()]);
    }

    #[test]
    fn badfilter_marker_is_recorded() {
        let r = net("||ads.example.com^$badfilter");
        assert!(r.include_domains.contains(&"\u{1}badfilter".to_string()));
    }

    impl NetworkRule {
        fn options_none_checker(&self) -> bool {
            self.types.is_none() && self.party == PartyScope::Any
        }
    }
}
