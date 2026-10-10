/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! brow (phase 4): the privacy engine's integration point with the net
//! stack. One `PrivacyState` lives on each `HttpState` (public + private)
//! and serves:
//!
//! * **network filtering** — EasyList/ABP-syntax lists over the
//!   `brow-privacy` Aho-Corasick engine, consulted from the fetch pipeline;
//! * **CNAME-cloaking detection** — DoH CNAME-chain chase (phase 4 DNS
//!   addition in brow-net-core) + verdict cache;
//! * **CHIPS enforcement** — `Partitioned` requires `Secure` on receive;
//! * **statistics** — lock-free counters persisted on shutdown.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use net_traits::request::Origin;
use servo_config::pref;
use servo_url::ServoUrl;
use content_security_policy::Destination;
use url::Url;

use crate::connector::create_dns_resolver;
use brow_privacy::chips::ChipsConfig;
use brow_privacy::cname::{CloakReason, CloakVerdict};
use brow_privacy::filter::{Decision, FilterEngine};
use brow_privacy::stats::PrivacyStats;

/// brow (v0.6.1 reassessment, fix 3.1): the filter list is embedded in the
/// binary as a last-resort fallback. Real-world testing showed the v0.6.0
/// engine silently disabled filtering in every installed build because the
/// list was resolved relative to the process CWD, which never matches the
/// install directory for Start-Menu / .desktop / symlink launches. With the
/// embedded snapshot (~80k EasyList rules) the engine can no longer no-op
/// in any packaging mode; file-based lists (pref or exe-relative) take
/// precedence so updates still work.
// brow (v0.6.1, fix 3.1 + 7.3): the embedded snapshots now live in
// brow-privacy (`lists::EMBEDDED_EASYLIST` / `EMBEDDED_EASYPRIVACY`) so the
// net stack, the script thread and the shell share ONE copy per process.
// This module references the EasyPrivacy snapshot directly: file-based
// lists (pref / exe-relative updates, typically EasyList-only) are ADDITIVE
// with embedded EasyPrivacy so the tracker half of the stack can never be
// dropped by a list refresh; the no-file fallback path uses the shared
// process-global engine (EasyList + EasyPrivacy).
const EMBEDDED_PRIVACY_LIST: &str = brow_privacy::lists::EMBEDDED_EASYPRIVACY;

pub struct PrivacyState {
    /// Filter engine, built lazily from the configured list; `None` when
    /// the filter is disabled (the embedded fallback makes "no list found"
    /// impossible in release builds). brow (7.3): `&'static` — the embedded
    /// path shares the process-global engine in brow-privacy; the file-list
    /// path leaks its engine (filter engines are process-lifetime state,
    /// they are never torn down in practice — the leak is intentional and
    /// bounded to at most one engine).
    engine: OnceLock<Option<&'static brow_privacy::filter::FilterEngine>>,
    /// Dedicated DoH resolver for CNAME-chain inspection (same config as
    /// the fetch resolver).
    resolver: brow_net_core::dns::DohResolver,
    /// Session verdict cache: flagged host -> verdict. `None` values are
    /// cached too (v0.6.1): a host checked once and found clean is never
    /// re-chased, so the DoH chain resolution costs at most one lookup per
    /// host per session instead of one per connection.
    cloaked_hosts: parking_lot::Mutex<std::collections::HashMap<String, Option<CloakVerdict>>>,
    stats: PrivacyStats,
    chips: ChipsConfig,
}

/// Info about a blocked request, surfaced for logs and counters.
#[derive(Clone, Debug)]
pub struct BlockedInfo {
    pub rule: String,
    pub important: bool,
}

impl PrivacyState {
    pub fn new(stats_path: Option<PathBuf>) -> PrivacyState {
        // brow (phase 4): materialise the fingerprint-defense userscript
        // next to the profile. The shell passes `--userscripts
        // <config_dir>/userscripts` so the engine runs it at document load;
        // when no config dir is available (tests, private session) the
        // generation is simply skipped.
        if let Some(dir) = stats_path
            .as_ref()
            .and_then(|p| p.parent().map(|d| d.join("userscripts")))
        {
            let level = match pref!(network_privacy_fingerprint_level).as_str() {
                "off" => brow_privacy::fingerprint::DefenseLevel::Off,
                "strict" => brow_privacy::fingerprint::DefenseLevel::Strict,
                _ => brow_privacy::fingerprint::DefenseLevel::Standard,
            };
            match brow_privacy::userscripts::write_defense_package(
                &dir,
                level,
                &brow_privacy::fingerprint::SessionKey::from_entropy(),
                &brow_privacy::fingerprint::FingerprintConfig::default(),
            ) {
                Ok(files) if !files.is_empty() => {
                    log::info!(
                        "brow privacy: fingerprint defenses ({level:?}) written to {}",
                        files[0].display()
                    );
                },
                Ok(_) => {},
                Err(err) => log::warn!("brow privacy: userscript generation failed: {err}"),
            }
        }
        PrivacyState {
            engine: OnceLock::new(),
            resolver: create_dns_resolver(),
            cloaked_hosts: parking_lot::Mutex::new(std::collections::HashMap::new()),
            stats: PrivacyStats::new(stats_path),
            chips: ChipsConfig {
                require_secure_for_partitioned: pref!(
                    network_privacy_chips_require_secure_partitioned
                ),
                // brow (v0.6.1, fix 3.3): was hardcoded `false` — the cookie
                // policy module existed but the "block third-party cookies"
                // behaviour never ran. Now driven by a real pref (default on).
                block_third_party_unpartitioned: pref!(
                    network_privacy_block_third_party_cookies
                ),
            },
        }
    }

    /// The network filter engine (lazy). `None` = disabled by pref.
    fn engine(&self) -> Option<&FilterEngine> {
        if !pref!(network_privacy_filter_enabled) {
            return None;
        }
        self.engine
            .get_or_init(|| {
                // brow (v0.6.1, fix 3.1): resolution order — pref path,
                // exe-relative resources (walk the exe's ancestor dirs the
                // way servoshell's resource reader does), CWD (dev builds),
                // embedded snapshot. v0.6.0 only had the CWD entry, which is
                // why installed builds silently filtered nothing.
                let mut paths: Vec<PathBuf> = Vec::new();
                let pref_path = pref!(network_privacy_filter_list_path);
                if !pref_path.is_empty() {
                    paths.push(PathBuf::from(pref_path));
                }
                if let Ok(exe) = std::env::current_exe() {
                    if let Some(exe_dir) = exe.parent() {
                        paths.push(exe_dir.join("resources/easylist.txt"));
                        if let Some(parent) = exe_dir.parent() {
                            paths.push(parent.join("resources/easylist.txt"));
                        }
                    }
                }
                paths.push(PathBuf::from("resources/easylist.txt"));
                let existing: Vec<&Path> = paths
                    .iter()
                    .filter(|p| p.is_file())
                    .map(|p| p.as_path())
                    .collect();
                if existing.is_empty() {
                    // brow (7.3): the process-global embedded engine —
                    // shared with the script thread's cosmetic wiring so
                    // the standard build holds ONE rule engine per content
                    // process, not two.
                    let engine = brow_privacy::lists::global_engine()
                        .expect("embedded filter list parse cannot fail (from_lists infallible)");
                    log::info!(
                        "brow privacy: no filter list file found at {paths:?}; \
                         using process-global embedded EasyList+EasyPrivacy engine ({} network rules)",
                        engine.network_rule_count()
                    );
                    return Some(engine);
                }
                // brow (phase7.1): file-based lists (pref / exe-relative
                // updates, typically EasyList-only) are ADDITIVE with the
                // embedded EasyPrivacy snapshot so the tracker half of the
                // stack can never be dropped by a list refresh.
                match brow_privacy::lists::engine_with_builtins(
                    &[EMBEDDED_PRIVACY_LIST],
                    &existing,
                ) {
                    Ok(engine) => {
                        log::info!(
                            "brow privacy: filter engine loaded {} network rules \
                             (files + embedded EasyPrivacy)",
                            engine.network_rule_count()
                        );
                        // brow (7.3): process-lifetime state — see the field
                        // doc for why the leak is intentional.
                        Some(Box::leak(Box::new(engine)) as &brow_privacy::filter::FilterEngine)
                    },
                    Err(err) => {
                        log::warn!("brow privacy: failed to load list: {err}");
                        None
                    },
                }
            })
            .as_ref()
    }

    /// The site origin for `$domain=` / party evaluation: the client origin
    /// for subresource fetches (the initiating page), or the request URL
    /// itself for navigations. Opaque origins evaluate as third-party.
    pub fn site_of(origin: &Origin, request: &Url) -> Url {
        match origin {
            Origin::Origin(immutable) if immutable.is_tuple() => {
                let url_origin = immutable.clone().into_url_origin();
                Url::parse(&url_origin.ascii_serialization())
                    .unwrap_or_else(|_| request.clone())
            },
            _ => Url::parse("https://brow-opaque.invalid/").expect("static URL"),
        }
    }

    /// Blocking decision for one subresource request. `None` = allowed.
    pub fn check_request(
        &self,
        origin: &Origin,
        request: &ServoUrl,
        destination: Destination,
    ) -> Option<BlockedInfo> {
        let engine = self.engine()?;
        let req_url: &Url = request.as_url();
        let dest_bit = dest_mask(destination);
        let site = Self::site_of(origin, req_url);
        let third_party = brow_privacy::chips::is_third_party(
            req_url.host_str().unwrap_or(""),
            &site,
        );

        let decision = engine.should_block(&site, req_url, dest_bit, third_party);
        match decision {
            Decision::Block { rule, important } => {
                self.stats.record_block(req_url.host_str().unwrap_or(""));
                Some(BlockedInfo { rule, important })
            },
            Decision::AllowExcepted { .. } | Decision::Allow => None,
        }
    }

    /// Evaluate an arbitrary URL against the site — used by the CNAME
    /// path to judge the *canonical* host of a cloaked request.
    pub fn check_canonical(
        &self,
        site: &Url,
        canonical_url: &Url,
        destination: Destination,
    ) -> Option<BlockedInfo> {
        let engine = self.engine()?;
        let third_party =
            brow_privacy::chips::is_third_party(canonical_url.host_str().unwrap_or(""), site);
        let decision =
            engine.should_block(site, canonical_url, dest_mask(destination), third_party);
        match decision {
            Decision::Block { rule, important } => {
                self.stats
                    .record_block(canonical_url.host_str().unwrap_or(""));
                Some(BlockedInfo { rule, important })
            },
            Decision::AllowExcepted { .. } | Decision::Allow => None,
        }
    }

    /// CNAME-cloaking inspection, called (awaited) before connecting when
    /// the destination is an https URL. Resolves the chain through DoH and
    /// caches the verdict for the session.
    ///
    /// brow (v0.6.1, fix 1.3): two cost/coverage changes vs v0.6.0 —
    /// (a) the chase only runs when a rules-capable filter engine exists
    /// (there is no point paying a DoH RTT per host when nothing can act on
    /// the canonical name), and (b) CLEAN verdicts are cached as well, so
    /// each host is resolved at most once per session instead of on every
    /// first connection (v0.6.0 cached only cloaked hosts, taxing every new
    /// https host with a DoH round-trip even on repeats).
    pub async fn check_cname(&self, request: &ServoUrl) -> Option<CloakVerdict> {
        if !pref!(network_privacy_cname_detection_enabled) {
            return None;
        }
        if self.engine().is_none() {
            return None;
        }
        let host = request.host_str()?.to_ascii_lowercase();
        if host.is_empty() || host.parse::<std::net::IpAddr>().is_ok() {
            return None;
        }
        if let Some(verdict) = self.cloaked_hosts.lock().get(&host) {
            return verdict.clone();
        }

        let resolved = self.resolve_cloak_verdict(&host).await;
        self.cloaked_hosts.lock().insert(host, resolved.clone());
        resolved
    }

    /// Resolve the cloak verdict for one host (uncached). `None` = clean or
    /// unresolvable (treated as clean — availability over strictness).
    async fn resolve_cloak_verdict(&self, host: &str) -> Option<CloakVerdict> {
        let chain = self.resolver.resolve_cname_chain(host).await.ok()?;
        let canonical = chain.last()?.clone();
        if canonical == host {
            return None;
        }
        // Cloaking signature: the canonical name lands on a different
        // registrable domain than the client-visible host.
        let visible_reg = brow_privacy::cname::registrable_suffix(host);
        let canonical_reg = brow_privacy::cname::registrable_suffix(&canonical);
        if visible_reg == canonical_reg {
            return None;
        }
        let verdict = CloakVerdict {
            reason: CloakReason::CrossDomainAlias {
                visible: host.to_owned(),
                canonical,
            },
            hops: chain.len(),
        };
        self.stats
            .record_cname(match &verdict.reason {
                CloakReason::CrossDomainAlias { canonical, .. } => canonical,
            });
        Some(verdict)
    }

    /// CHIPS receive-side gate (attribute form, matched to the `cookie` crate
    /// parse the net stack already performed). Returns false when the cookie
    /// MUST be ignored: `Partitioned` without `Secure`
    /// (draft-ietf-httpbis-rfc6265bis §5.6.3).
    pub fn chips_receive_allowed_attrs(&self, partitioned: bool, secure: bool) -> bool {
        if partitioned && self.chips.require_secure_for_partitioned && !secure {
            self.stats.record_cookie_rejected();
            return false;
        }
        true
    }

    /// brow (v0.6.1, fix 3.3): full CHIPS receive policy for one cookie.
    ///
    /// Returns `None` when the cookie must be dropped, or `Some(partition)`
    /// with the partition key to store on the cookie:
    /// * first-party cookies are always accepted unpartitioned;
    /// * third-party `Partitioned` cookies are accepted (when `Secure`, per
    ///   the attribute gate) and keyed to the top-level site;
    /// * third-party unpartitioned cookies are dropped when the
    ///   `network_privacy_block_third_party_cookies` pref is on (default).
    pub fn cookie_receive_policy(
        &self,
        request_host: &str,
        top_site: &Url,
        partitioned: bool,
        secure: bool,
    ) -> Option<Option<String>> {
        if !self.chips_receive_allowed_attrs(partitioned, secure) {
            return None;
        }
        let third_party =
            brow_privacy::chips::is_third_party(request_host, top_site);
        if !third_party {
            return Some(None);
        }
        if partitioned {
            let key = brow_privacy::chips::partition_key(top_site);
            Some(Some(key))
        } else if pref!(network_privacy_block_third_party_cookies) {
            self.stats.record_cookie_rejected();
            log::info!(
                "brow privacy: dropped unpartitioned third-party cookie for {request_host} \
                 (site {top_site})"
            );
            None
        } else {
            Some(None)
        }
    }

    /// The CHIPS send-side config (drive `chips::send_policy` from the
    /// cookie header path). Read live from prefs so the settings panel's
    /// toggles apply without an engine restart.
    pub fn chips_config(&self) -> ChipsConfig {
        ChipsConfig {
            require_secure_for_partitioned: pref!(
                network_privacy_chips_require_secure_partitioned
            ),
            block_third_party_unpartitioned: pref!(
                network_privacy_block_third_party_cookies
            ),
        }
    }

    /// Whether the filter engine is active (rules loaded). Used by the
    /// WebSocket loader to skip the gate cheaply when filtering is off.
    pub fn filtering_active(&self) -> bool {
        self.engine().is_some()
    }

    pub fn persist(&self) {
        self.stats.persist();
    }

    pub fn stats(&self) -> &PrivacyStats {
        &self.stats
    }
}

/// Map a fetch destination onto the filter engine's resource-type bits.
/// The associated consts are plain `u32` bits.
fn dest_mask(destination: Destination) -> u32 {
    use brow_privacy::filter::rule::ResourceTypeMask as M;
    match destination {
        Destination::Script
        | Destination::Worker
        | Destination::SharedWorker
        | Destination::ServiceWorker
        | Destination::AudioWorklet
        | Destination::PaintWorklet => M::SCRIPT,
        Destination::Image => M::IMAGE,
        Destination::Style | Destination::Xslt => M::STYLESHEET,
        Destination::Object => M::OBJECT,
        Destination::IFrame | Destination::Frame | Destination::Embed => M::SUBDOCUMENT,
        Destination::Document | Destination::Manifest | Destination::Json => M::DOCUMENT,
        Destination::Font => M::FONT,
        Destination::Audio | Destination::Video | Destination::Track => M::MEDIA,
        Destination::Report => M::PING,
        _ => M::OTHER,
    }
}
