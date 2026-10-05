/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! DNS-over-HTTPS (RFC 8484) resolver with bootstrap addresses, TTL cache and
//! system-resolver fallback.
//!
//! # Loop avoidance
//!
//! Resolving the DoH template's own host through the system resolver would leak
//! every DoH-hosted query's SNI and defeat the purpose of encrypted DNS. brow
//! therefore requires **bootstrap addresses**: raw IPs used to open the TLS
//! connection, with the SNI still set to the template's hostname so certificate
//! validation stays correct. This mirrors Firefox's `network.trr.bootstrapAddr`.

pub mod doh;
pub mod service;
pub mod wire;

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;

use crate::error::BrowNetError;

/// How the resolver answers lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DnsMode {
    /// Only the OS resolver (plain UDP/getaddrinfo). No DoH.
    #[default]
    SystemOnly,
    /// Try DoH first; on any failure fall back to the OS resolver.
    /// This is the brow default: privacy-preserving without sacrificing
    /// availability on networks that block DoH.
    DohWithSystemFallback,
    /// DoH only; failures surface as resolution failures (strict mode).
    DohOnly,
}

/// Configuration for [`DohResolver`].
#[derive(Debug, Clone)]
pub struct DohConfig {
    /// DNS mode.
    pub mode: DnsMode,
    /// DoH template URLs (RFC 8484 §3), e.g. `https://mozilla.cloudflare-dns.com/dns-query`.
    /// Multiple templates are tried round-robin; a template that fails is
    /// skipped until the rotation wraps around.
    pub templates: Vec<String>,
    /// Bootstrap IPs for the template hosts (see module docs).
    pub bootstrap: Vec<IpAddr>,
    /// Per-attempt timeout for a DoH exchange.
    pub timeout: Duration,
    /// Override the TLS client configuration used for the DoH connection
    /// (e.g. to trust an enterprise root). `None` = public webpki roots.
    pub tls_config: Option<rustls::ClientConfig>,
}

impl Default for DohConfig {
    fn default() -> Self {
        Self {
            mode: DnsMode::SystemOnly,
            templates: Vec::new(),
            bootstrap: vec![
                "1.1.1.1".parse().expect("valid IP"),
                "8.8.8.8".parse().expect("valid IP"),
            ],
            timeout: Duration::from_secs(3),
            tls_config: None,
        }
    }
}

/// A resolved address set with its TTL.
#[derive(Debug, Clone)]
pub struct ResolvedAddrs {
    /// All A/AAAA addresses, IPv6 first (Happy Eyeballs ordering happens in the connector).
    pub addrs: Vec<IpAddr>,
    /// Minimum TTL across answers; entries live at most this long.
    pub ttl: Duration,
}

#[derive(Debug)]
struct CacheEntry {
    addrs: Vec<IpAddr>,
    expires_at: Instant,
}

/// Shared, cached DoH resolver.
///
/// Cheap to clone (internal state is `Arc`'d). Concurrent lookups of the same
/// host are deduplicated: the second caller waits on a per-host lock and then
/// reads the freshly populated cache.
#[derive(Clone)]
pub struct DohResolver {
    inner: Arc<Inner>,
}

struct Inner {
    config: DohConfig,
    cache: Mutex<HashMap<String, CacheEntry>>,
    /// Per-host admission locks to collapse in-flight duplicate queries.
    in_flight: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// Round-robin cursor over `config.templates`.
    template_cursor: AtomicUsize,
    /// The DoH transport (HTTPS client), constructed lazily on first use.
    transport: tokio::sync::OnceCell<doh::DohTransport>,
}

impl DohResolver {
    /// Create a resolver from its configuration.
    pub fn new(config: DohConfig) -> Self {
        Self {
            inner: Arc::new(Inner {
                config,
                cache: Mutex::new(HashMap::new()),
                in_flight: Mutex::new(HashMap::new()),
                template_cursor: AtomicUsize::new(0),
                transport: tokio::sync::OnceCell::new(),
            }),
        }
    }

    /// A resolver that only uses the system resolver.
    pub fn system_only() -> Self {
        Self::new(DohConfig::default())
    }

    /// Is DoH actually going to be attempted?
    pub fn doh_enabled(&self) -> bool {
        self.inner.config.mode != DnsMode::SystemOnly && !self.inner.config.templates.is_empty()
    }

    /// Resolve `host` to addresses. IP literals short-circuit (no cache pollution).
    pub async fn resolve(&self, host: &str, port: u16) -> Result<ResolvedAddrs, BrowNetError> {
        // 1. IP literal fast path.
        if let Ok(ip) = host.parse::<IpAddr>() {
            return Ok(ResolvedAddrs {
                addrs: vec![ip],
                ttl: Duration::from_secs(300),
            });
        }

        // 2. Cache fast path.
        if let Some(entry) = self.inner.cache.lock().get(host) {
            if entry.expires_at > Instant::now() {
                return Ok(ResolvedAddrs {
                    addrs: entry.addrs.clone(),
                    ttl: entry.expires_at - Instant::now(),
                });
            }
        }

        // 3. Collapse concurrent lookups for the same host.
        let host_lock = {
            let mut in_flight = self.inner.in_flight.lock();
            Arc::clone(in_flight.entry(host.to_string()).or_insert_with(|| {
                Arc::new(tokio::sync::Mutex::new(()))
            }))
        };
        let _guard = host_lock.lock().await;

        // Double-check the cache after acquiring the lock.
        if let Some(entry) = self.inner.cache.lock().get(host) {
            if entry.expires_at > Instant::now() {
                return Ok(ResolvedAddrs {
                    addrs: entry.addrs.clone(),
                    ttl: entry.expires_at - Instant::now(),
                });
            }
        }

        // 4. DoH attempt(s), then optional system fallback.
        let resolved = self.resolve_via_doh(host).await;
        match resolved {
            Ok(addrs) => {
                self.inner.cache.lock().insert(
                    host.to_string(),
                    CacheEntry {
                        addrs: addrs.addrs.clone(),
                        expires_at: Instant::now() + addrs.ttl,
                    },
                );
                Ok(addrs)
            },
            Err(doh_error) => {
                if self.inner.config.mode == DnsMode::DohOnly {
                    return Err(BrowNetError::Resolution {
                        host: host.to_string(),
                        last_error: doh_error.to_string(),
                    });
                }
                log::info!("DoH failed for {host} ({doh_error}); falling back to system resolver");
                self.resolve_via_system(host, port)
                    .await
                    .map_err(|sys_error| BrowNetError::Resolution {
                        host: host.to_string(),
                        last_error: format!("DoH: {doh_error}; system: {sys_error}"),
                    })
            },
        }
    }

    async fn resolve_via_doh(&self, host: &str) -> Result<ResolvedAddrs, BrowNetError> {
        if !self.doh_enabled() {
            return Err(BrowNetError::Doh("DoH disabled or no templates configured".into()));
        }

        let templates = &self.inner.config.templates;
        // Try each template once per lookup (round-robin starting point).
        let start = self.inner.template_cursor.fetch_add(1, Ordering::Relaxed);
        let mut last_error = None;
        for i in 0..templates.len() {
            let template = &templates[(start + i) % templates.len()];
            match self.query_template(template, host).await {
                Ok(addrs) => return Ok(addrs),
                Err(err) => {
                    log::debug!("DoH template {template} failed: {err}");
                    last_error = Some(err);
                },
            }
        }
        Err(last_error.unwrap_or_else(|| BrowNetError::Doh("no templates".into())))
    }

    async fn query_template(&self, template: &str, host: &str) -> Result<ResolvedAddrs, BrowNetError> {
        let transport = self
            .inner
            .transport
            .get_or_try_init(|| async { doh::DohTransport::new(&self.inner.config) })
            .await?;

        // Pick a bootstrap IP for this template (round-robin over bootstrap list).
        let bootstrap = self
            .inner
            .config
            .bootstrap
            .first()
            .copied()
            .ok_or_else(|| BrowNetError::Doh("no bootstrap addresses configured".into()))?;

        let wire_query = wire::build_query(host)?;
        let response_bytes = transport
            .exchange(template, bootstrap, wire_query, self.inner.config.timeout)
            .await?;
        wire::parse_response(&response_bytes, host)
    }

    async fn resolve_via_system(&self, host: &str, port: u16) -> Result<ResolvedAddrs, BrowNetError> {
        let addrs = tokio::net::lookup_host((host, port))
            .await
            .map_err(|e| BrowNetError::Io(std::io::Error::new(e.kind(), e.to_string())))?;
        let mut ips: Vec<IpAddr> = addrs.map(|a| a.ip()).collect();
        ips.dedup();
        if ips.is_empty() {
            return Err(BrowNetError::Resolution {
                host: host.to_string(),
                last_error: "system resolver returned no addresses".into(),
            });
        }
        // getaddrinfo gives no usable TTL; cache briefly to avoid hot loops.
        Ok(ResolvedAddrs {
            addrs: ips,
            ttl: Duration::from_secs(30),
        })
    }

    /// Number of cached host entries (for diagnostics/memory reports).
    pub fn cache_len(&self) -> usize {
        self.inner.cache.lock().len()
    }

    /// Drop all cached entries (e.g. on network change events).
    pub fn clear_cache(&self) {
        self.inner.cache.lock().clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ip_literal_short_circuits() {
        let resolver = DohResolver::system_only();
        let resolved = resolver.resolve("192.0.2.7", 443).await.unwrap();
        assert_eq!(resolved.addrs, vec!["192.0.2.7".parse::<IpAddr>().unwrap()]);
    }

    #[tokio::test]
    async fn system_fallback_resolves_localhost() {
        let config = DohConfig {
            mode: DnsMode::DohWithSystemFallback,
            templates: vec!["https://127.0.0.1:1/dns-query".to_string()],
            bootstrap: vec!["127.0.0.1".parse().unwrap()],
            timeout: Duration::from_millis(300),
            tls_config: None,
        };
        let resolver = DohResolver::new(config);
        let resolved = resolver.resolve("localhost", 80).await.expect("localhost must resolve");
        assert!(!resolved.addrs.is_empty());
        assert!(
            resolved
                .addrs
                .iter()
                .any(|ip| matches!(ip, IpAddr::V4(_) | IpAddr::V6(_)))
        );
    }

    #[tokio::test]
    async fn doh_only_mode_fails_without_templates() {
        let config = DohConfig {
            mode: DnsMode::DohOnly,
            ..DohConfig::default()
        };
        let resolver = DohResolver::new(config);
        assert!(resolver.resolve("example.test", 443).await.is_err());
    }
}
