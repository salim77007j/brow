/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! COOP / COEP / CORP parsing and the cross-origin isolation policy engine.
//!
//! This module implements the *header parsing* and *policy decision* halves of:
//!
//! * **COOP** — Cross-Origin-Opener-Policy (RFC / WHATWG HTML integration):
//!   <https://html.spec.whatwg.org/multipage/origin.html#coop>
//! * **COEP** — Cross-Origin-Embedder-Policy:
//!   <https://html.spec.whatwg.org/multipage/origin.html#coep>
//! * **CORP** — Cross-Origin-Resource-Policy:
//!   <https://fetch.spec.whatwg.org/#cross-origin-resource-policy-header>
//!
//! Phase 2 wires the parsing and decisions into the network layer (recording
//! policies on navigation responses and consulting CORP for cross-origin
//! subresource fetches). The full document-level isolation plumbing (browsing
//! context groups, agent-cluster splitting) lands with the Phase 3 shell, which
//! owns the frame tree; the decision functions here are the single source of
//! truth both layers call into.

use serde::{Deserialize, Serialize};

/// Parsed `Cross-Origin-Opener-Policy` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CoopPolicy {
    /// `unsafe-none` (default value).
    #[default]
    UnsafeNone,
    /// `same-origin`.
    SameOrigin,
    /// `same-origin-allow-popups`.
    SameOriginAllowPopups,
}

/// Parsed `Cross-Origin-Embedder-Policy` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CoepPolicy {
    /// `unsafe-none` (default value).
    #[default]
    UnsafeNone,
    /// `require-corp`.
    RequireCorp,
    /// `credentialless` (Chromium extension, standardized in 2023+).
    Credentialless,
}

/// Parsed `Cross-Origin-Resource-Policy` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CorpPolicy {
    /// `same-origin`.
    #[default]
    SameOrigin,
    /// `same-site`.
    SameSite,
    /// `cross-origin`.
    CrossOrigin,
}

/// The result of a policy check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    /// The embedding / opener relationship is allowed.
    Allow,
    /// The response must be blocked; carries the reason for logging/devtools.
    Block(&'static str),
}

/// Parse a COOP header value (first meaningful token; `report-to` is a parameter).
pub fn parse_coop(value: &str) -> Option<CoopPolicy> {
    let token = value.split(';').next()?.trim().to_ascii_lowercase();
    match token.as_str() {
        "unsafe-none" => Some(CoopPolicy::UnsafeNone),
        "same-origin" => Some(CoopPolicy::SameOrigin),
        "same-origin-allow-popups" => Some(CoopPolicy::SameOriginAllowPopups),
        _ => None,
    }
}

/// Parse a COEP header value.
pub fn parse_coep(value: &str) -> Option<CoepPolicy> {
    let token = value.split(';').next()?.trim().to_ascii_lowercase();
    match token.as_str() {
        "unsafe-none" => Some(CoepPolicy::UnsafeNone),
        "require-corp" => Some(CoepPolicy::RequireCorp),
        "credentialless" => Some(CoepPolicy::Credentialless),
        _ => None,
    }
}

/// Parse a CORP header value.
pub fn parse_corp(value: &str) -> Option<CorpPolicy> {
    let token = value.trim().to_ascii_lowercase();
    match token.as_str() {
        "same-origin" => Some(CorpPolicy::SameOrigin),
        "same-site" => Some(CorpPolicy::SameSite),
        "cross-origin" => Some(CorpPolicy::CrossOrigin),
        _ => None,
    }
}

/// COOP enforcement (HTML spec "check browsing context group compatibility").
///
/// Given the *new* document's COOP policy, the *opener's* COOP policy, and
/// whether opener and opener-top-level are same-origin with the new document,
/// decide whether the new document may keep its opener relationship.
///
/// `same-origin-with-opener` / `same-origin-with-opener-top` mirror the spec's
/// booleans computed by the caller (which owns the origin information).
pub fn coop_allows_opener(
    doc_policy: CoopPolicy,
    opener_policy: CoopPolicy,
    same_origin_with_opener: bool,
    same_origin_with_opener_top: bool,
) -> PolicyDecision {
    let opener_enforces = opener_policy != CoopPolicy::UnsafeNone;
    match doc_policy {
        CoopPolicy::UnsafeNone => PolicyDecision::Allow,
        CoopPolicy::SameOrigin => {
            if opener_enforces
                && same_origin_with_opener
                && same_origin_with_opener_top
            {
                PolicyDecision::Allow
            } else {
                PolicyDecision::Block("COOP same-origin: opener is cross-origin or not COOP-enforcing")
            }
        },
        CoopPolicy::SameOriginAllowPopups => {
            // The document itself may keep its opener, but its *openees* are
            // still separated unless same-origin. For the opener relationship
            // of *this* document, allow-popups only demands the opener be
            // same-origin when the opener does NOT enforce COOP itself.
            if same_origin_with_opener || opener_enforces {
                PolicyDecision::Allow
            } else {
                PolicyDecision::Block(
                    "COOP same-origin-allow-popups: opener is cross-origin and not COOP-enforcing",
                )
            }
        },
    }
}

/// CORP check (fetch spec §3.1 "cross-origin resource policy internal check").
///
/// `embedder_site` vs `resource_site` are site strings (schemeful site, e.g.
/// `https://example.com`); equality means same site.
pub fn corp_allows(
    embedder_site: &str,
    resource_site: &str,
    embedder_origin: &str,
    resource_origin: &str,
    corp: CorpPolicy,
) -> PolicyDecision {
    match corp {
        CorpPolicy::CrossOrigin => PolicyDecision::Allow,
        CorpPolicy::SameSite => {
            if embedder_site == resource_site {
                PolicyDecision::Allow
            } else {
                PolicyDecision::Block("CORP same-site: resource site differs from embedder site")
            }
        },
        CorpPolicy::SameOrigin => {
            if embedder_origin == resource_origin {
                PolicyDecision::Allow
            } else {
                PolicyDecision::Block("CORP same-origin: resource origin differs from embedder origin")
            }
        },
    }
}

/// Whether a response may be delivered to a COEP-enforcing embedder.
///
/// * `response_mode_is_cors` — the fetch ran in CORS mode and *passed* the CORS check
///   (fetch spec: "CORS check" success makes `require-corp` unnecessary).
/// * `request_was_no_cors` — the request mode was `no-cors`.
/// * `corp` — the response's CORP header, if any.
/// * `coep` — the embedder document's COEP policy.
pub fn coep_allows_response(
    coep: CoepPolicy,
    response_mode_is_cors: bool,
    request_was_no_cors: bool,
    corp: Option<CorpPolicy>,
) -> PolicyDecision {
    match coep {
        CoepPolicy::UnsafeNone => PolicyDecision::Allow,
        CoepPolicy::RequireCorp => {
            if response_mode_is_cors {
                return PolicyDecision::Allow;
            }
            match corp {
                Some(CorpPolicy::CrossOrigin) => PolicyDecision::Allow,
                Some(_) => {
                    PolicyDecision::Block("COEP require-corp: response lacks cross-origin CORP")
                },
                None => {
                    PolicyDecision::Block("COEP require-corp: response has no CORP header")
                },
            }
        },
        CoepPolicy::Credentialless => {
            // Requests *with credentials* must satisfy CORP; no-credential
            // cross-origin no-cors requests are allowed through (empty key cache).
            if !request_was_no_cors {
                // cors-mode handled like require-corp.
                if response_mode_is_cors {
                    return PolicyDecision::Allow;
                }
                return match corp {
                    Some(CorpPolicy::CrossOrigin) => PolicyDecision::Allow,
                    Some(_) => PolicyDecision::Block(
                        "COEP credentialless: credentialed response lacks cross-origin CORP",
                    ),
                    None => PolicyDecision::Block(
                        "COEP credentialless: credentialed response has no CORP header",
                    ),
                };
            }
            // no-cors request without credentials: allowed (subresources fetched
            // without credentials; CORP header, if present, is still honored).
            match corp {
                Some(CorpPolicy::SameOrigin) | Some(CorpPolicy::SameSite) => {
                    PolicyDecision::Block("COEP credentialless: response CORP restricts same-origin/site")
                },
                _ => PolicyDecision::Allow,
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_coop_values() {
        assert_eq!(parse_coop("same-origin"), Some(CoopPolicy::SameOrigin));
        assert_eq!(
            parse_coop("same-origin-allow-popups"),
            Some(CoopPolicy::SameOriginAllowPopups)
        );
        assert_eq!(
            parse_coop("unsafe-none; report-to=\"csp-endpoint\""),
            Some(CoopPolicy::UnsafeNone)
        );
        assert_eq!(parse_coop("nonsense"), None);
    }

    #[test]
    fn parses_coep_values() {
        assert_eq!(parse_coep("require-corp"), Some(CoepPolicy::RequireCorp));
        assert_eq!(parse_coep("credentialless"), Some(CoepPolicy::Credentialless));
        assert_eq!(parse_coep("unsafe-none"), Some(CoepPolicy::UnsafeNone));
        assert_eq!(parse_coep("garbage; report-to=\"x\""), None);
    }

    #[test]
    fn parses_corp_values() {
        assert_eq!(parse_corp("cross-origin"), Some(CorpPolicy::CrossOrigin));
        assert_eq!(parse_corp(" same-site "), Some(CorpPolicy::SameSite));
        assert_eq!(parse_corp("same-origin"), Some(CorpPolicy::SameOrigin));
        assert_eq!(parse_corp(""), None);
    }

    #[test]
    fn coop_same_origin_blocks_cross_origin_opener() {
        assert!(matches!(
            coop_allows_opener(CoopPolicy::SameOrigin, CoopPolicy::UnsafeNone, false, false),
            PolicyDecision::Block(_)
        ));
        assert!(matches!(
            coop_allows_opener(CoopPolicy::SameOrigin, CoopPolicy::SameOrigin, true, true),
            PolicyDecision::Allow
        ));
        // Opener enforces but is cross-origin → still blocked.
        assert!(matches!(
            coop_allows_opener(CoopPolicy::SameOrigin, CoopPolicy::SameOrigin, false, true),
            PolicyDecision::Block(_)
        ));
    }

    #[test]
    fn coop_allow_popups_matches_spec() {
        // Cross-origin opener that itself enforces COOP → allowed.
        assert!(matches!(
            coop_allows_opener(CoopPolicy::SameOriginAllowPopups, CoopPolicy::SameOrigin, false, false),
            PolicyDecision::Allow
        ));
        // Cross-origin opener with no COOP → blocked.
        assert!(matches!(
            coop_allows_opener(CoopPolicy::SameOriginAllowPopups, CoopPolicy::UnsafeNone, false, false),
            PolicyDecision::Block(_)
        ));
    }

    #[test]
    fn corp_checks() {
        assert!(matches!(
            corp_allows(
                "https://a.example",
                "https://a.example",
                "https://a.example",
                "https://a.example",
                CorpPolicy::SameOrigin
            ),
            PolicyDecision::Allow
        ));
        // Same site, different origin, same-origin CORP → blocked.
        assert!(matches!(
            corp_allows(
                "https://a.example",
                "https://a.example",
                "https://a.example",
                "https://b.a.example",
                CorpPolicy::SameOrigin
            ),
            PolicyDecision::Block(_)
        ));
        // Same site, same-site CORP → allowed.
        assert!(matches!(
            corp_allows(
                "https://a.example",
                "https://a.example",
                "https://a.example",
                "https://b.a.example",
                CorpPolicy::SameSite
            ),
            PolicyDecision::Allow
        ));
        // Different site, same-site CORP → blocked.
        assert!(matches!(
            corp_allows(
                "https://a.example",
                "https://b.example",
                "https://a.example",
                "https://b.example",
                CorpPolicy::SameSite
            ),
            PolicyDecision::Block(_)
        ));
    }

    #[test]
    fn coep_require_corp_matrix() {
        // CORS-passing response: allowed without CORP.
        assert!(matches!(
            coep_allows_response(CoepPolicy::RequireCorp, true, false, None),
            PolicyDecision::Allow
        ));
        // no-cors, no CORP → blocked.
        assert!(matches!(
            coep_allows_response(CoepPolicy::RequireCorp, false, true, None),
            PolicyDecision::Block(_)
        ));
        // no-cors, cross-origin CORP → allowed.
        assert!(matches!(
            coep_allows_response(
                CoepPolicy::RequireCorp,
                false,
                true,
                Some(CorpPolicy::CrossOrigin)
            ),
            PolicyDecision::Allow
        ));
        // no-cors, same-origin CORP → blocked.
        assert!(matches!(
            coep_allows_response(
                CoepPolicy::RequireCorp,
                false,
                true,
                Some(CorpPolicy::SameOrigin)
            ),
            PolicyDecision::Block(_)
        ));
    }

    #[test]
    fn coep_credentialless_matrix() {
        // Credentialed cors-mode: allowed.
        assert!(matches!(
            coep_allows_response(CoepPolicy::Credentialless, true, false, None),
            PolicyDecision::Allow
        ));
        // Credentialed no-cors without CORP: blocked.
        assert!(matches!(
            coep_allows_response(CoepPolicy::Credentialless, false, false, None),
            PolicyDecision::Block(_)
        ));
        // Credentialless no-cors request (no credentials): allowed even without CORP.
        assert!(matches!(
            coep_allows_response(CoepPolicy::Credentialless, false, true, None),
            PolicyDecision::Allow
        ));
        // …but a same-origin CORP response is still restricted.
        assert!(matches!(
            coep_allows_response(
                CoepPolicy::Credentialless,
                false,
                true,
                Some(CorpPolicy::SameOrigin)
            ),
            PolicyDecision::Block(_)
        ));
    }

    #[test]
    fn unsafe_none_allows_everything() {
        assert!(matches!(
            coep_allows_response(CoepPolicy::UnsafeNone, false, false, None),
            PolicyDecision::Allow
        ));
        assert!(matches!(
            coop_allows_opener(CoopPolicy::UnsafeNone, CoopPolicy::UnsafeNone, false, false),
            PolicyDecision::Allow
        ));
    }
}
