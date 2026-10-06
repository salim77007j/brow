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

use std::path::PathBuf;
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

pub struct PrivacyState {
    /// Filter engine, built lazily from the configured list; `None` when
    /// the filter is disabled or no list could be loaded.
    engine: OnceLock<Option<FilterEngine>>,
    /// Dedicated DoH resolver for CNAME-chain inspection (same config as
    /// the fetch resolver).
    resolver: brow_net_core::dns::DohResolver,
    /// Session verdict cache: flagged host -> verdict.
    cloaked_hosts: parking_lot::Mutex<std::collections::HashMap<String, CloakVerdict>>,
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
                block_third_party_unpartitioned: false,
            },
        }
    }

    /// The network filter engine (lazy). `None` = disabled / no list.
    fn engine(&self) -> Option<&FilterEngine> {
        if !pref!(network_privacy_filter_enabled) {
            return None;
        }
        self.engine
            .get_or_init(|| {
                let path = pref!(network_privacy_filter_list_path);
                let paths: Vec<PathBuf> = if path.is_empty() {
                    // default location shipped next to the engine resources
                    [PathBuf::from("resources/easylist.txt")]
                        .into_iter()
                        .collect()
                } else {
                    [PathBuf::from(path)].into_iter().collect()
                };
                let existing: Vec<&std::path::Path> = paths
                    .iter()
                    .filter(|p| p.is_file())
                    .map(|p| p.as_path())
                    .collect();
                if existing.is_empty() {
                    log::info!(
                        "brow privacy: no filter list found at {paths:?}; network \
                         filtering inactive (CNAME/CHIPS still active)"
                    );
                    return None;
                }
                match brow_privacy::lists::engine_from_files(&existing) {
                    Ok(engine) => {
                        log::info!(
                            "brow privacy: filter engine loaded {} network rules",
                            engine.network_rule_count()
                        );
                        Some(engine)
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
    pub async fn check_cname(&self, request: &ServoUrl) -> Option<CloakVerdict> {
        if !pref!(network_privacy_cname_detection_enabled) {
            return None;
        }
        let host = request.host_str()?.to_ascii_lowercase();
        if host.is_empty() || host.parse::<std::net::IpAddr>().is_ok() {
            return None;
        }
        if let Some(verdict) = self.cloaked_hosts.lock().get(&host) {
            return Some(verdict.clone());
        }

        let chain = self.resolver.resolve_cname_chain(&host).await.ok()?;
        let canonical = chain.last()?.clone();
        if canonical == host {
            return None;
        }
        // Cloaking signature: the canonical name lands on a different
        // registrable domain than the client-visible host.
        let visible_reg = brow_privacy::cname::registrable_suffix(&host);
        let canonical_reg = brow_privacy::cname::registrable_suffix(&canonical);
        if visible_reg == canonical_reg {
            return None;
        }
        let verdict = CloakVerdict {
            reason: CloakReason::CrossDomainAlias {
                visible: host.clone(),
                canonical,
            },
            hops: chain.len(),
        };
        self.stats
            .record_cname(match &verdict.reason {
                CloakReason::CrossDomainAlias { canonical, .. } => canonical,
            });
        self.cloaked_hosts
            .lock()
            .insert(host, verdict.clone());
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
