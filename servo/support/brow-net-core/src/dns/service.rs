/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! hyper-util connector integration: a `Service<Name>` implementation that
//! routes every engine connection through the brow DoH resolver.
//!
//! hyper-util 0.1's `HttpConnector` accepts any `tower_service::Service<Name>`
//! whose response is an `Iterator<Item = SocketAddr>` (the `Resolve` "trait
//! alias" is blanket-implemented for such services). We implement exactly
//! that; the engine's `ServoHttpConnector` constructs its
//! `HttpConnector::new_with_resolver(BrowDnsService::new(...))` with it,
//! replacing hyper's default getaddrinfo resolver.
//!
//! Port numbers produced here are 0: hyper substitutes the request URI's port
//! when dialing; only the IP ordering matters (we hand back DoH's address
//! list, letting hyper's Happy Eyeballs pick the family).

use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::vec::IntoIter as VecIntoIter;

use hyper_util::client::legacy::connect::dns::Name;
use tower::Service;

use super::DohResolver;

/// A hyper-compatible DNS resolver service backed by [`DohResolver`].
///
/// Cheap to clone; the underlying resolver caches per host.
#[derive(Clone)]
pub struct BrowDnsService {
    resolver: Arc<DohResolver>,
}

impl BrowDnsService {
    /// Wrap a configured resolver.
    pub fn new(resolver: DohResolver) -> Self {
        Self {
            resolver: Arc::new(resolver),
        }
    }

    /// Access the inner resolver (diagnostics, cache management).
    pub fn inner(&self) -> &DohResolver {
        &self.resolver
    }
}

impl Service<Name> for BrowDnsService {
    type Response = VecIntoIter<SocketAddr>;
    type Error = Infallible;
    type Future =
        Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, name: Name) -> Self::Future {
        let resolver = Arc::clone(&self.resolver);
        let host = name.as_str().to_string();
        Box::pin(async move {
            // Never fail the service: errors are surfaced as an empty address
            // list, which hyper reports as "failed to lookup address
            // information" — the same observable behavior as getaddrinfo
            // failures on the default path.
            let addrs = match resolver.resolve(&host, 0).await {
                Ok(resolved) => resolved
                    .addrs
                    .into_iter()
                    .map(|ip| SocketAddr::new(ip, 0))
                    .collect(),
                Err(err) => {
                    log::warn!("DNS resolution failed for {host}: {err}");
                    Vec::new()
                },
            };
            Ok(addrs.into_iter())
        })
    }
}
