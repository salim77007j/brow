/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! HTTPS fetch pipeline built on the same network crates as brow's engine
//! net stack (hyper + hyper-rustls + webpki-roots), with explicit per-stage
//! timing and a separate raw TCP connect probe.

use std::time::{Duration, Instant};

use http::Uri;
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use hyper_util::rt::TokioExecutor;
use tokio::net::TcpStream;

use crate::metrics::{FetchOutcome, FetchRecord, ParseStats};
use crate::parse;

pub const USER_AGENT: &str =
    "brow-phase6/0.1 (comparative validation; +https://github.com/salim77007j/brow)";

type HttpsClient = Client<
    hyper_rustls::HttpsConnector<HttpConnector>,
    http_body_util::Full<bytes::Bytes>,
>;

fn build_client() -> HttpsClient {
    let mut http = hyper_util::client::legacy::connect::HttpConnector::new();
    http.set_connect_timeout(Some(Duration::from_secs(8)));
    http.set_nodelay(true);
    // hyper-rustls passes the https URI to the inner connector for DNS+TCP;
    // a default HttpConnector enforces scheme==http and rejects it.
    http.enforce_http(false);
    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .wrap_connector(http);
    Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(15))
        .build(https)
}

/// Raw TCP connect probe to host:443 on its own socket. Informative only —
/// reports label it as a probe because it is not on the request path.
pub async fn tcp_connect_probe(url: &str) -> Option<Duration> {
    let uri: Uri = url.parse().ok()?;
    let host = uri.host()?;
    let start = Instant::now();
    let res =
        tokio::time::timeout(Duration::from_secs(6), TcpStream::connect((host, 443u16))).await;
    match res {
        Ok(Ok(_)) => Some(start.elapsed()),
        _ => None,
    }
}

/// Fetch one URL and run the HTML parse pipeline on a text/html body.
pub async fn fetch_and_parse(
    client: &HttpsClient,
    category: &str,
    url: &str,
    timeout: Duration,
    with_connect_probe: bool,
) -> FetchRecord {
    let probe = if with_connect_probe {
        tcp_connect_probe(url).await.map(|d| d.as_millis() as u64)
    } else {
        None
    };

    let uri = match url.parse::<Uri>() {
        Ok(u) => u,
        Err(e) => {
            return FetchRecord {
                category: category.into(),
                url: url.into(),
                http_version: String::new(),
                status: 0,
                tcp_connect_ms: probe,
                ttfb_ms: 0,
                total_ms: 0,
                body_bytes: 0,
                outcome: FetchOutcome::TransportError,
                error: Some(format!("invalid url: {e}")),
                parse: None,
            }
        }
    };

    let req = match hyper::Request::builder()
        .method("GET")
        .uri(uri.clone())
        .header("user-agent", USER_AGENT)
        .header("accept", "text/html,application/xhtml+xml;q=0.9,*/*;q=0.8")
        .header("accept-language", "en-US,en;q=0.9")
        .body(http_body_util::Full::new(bytes::Bytes::new()))
    {
        Ok(r) => r,
        Err(e) => {
            return FetchRecord {
                category: category.into(),
                url: url.into(),
                http_version: String::new(),
                status: 0,
                tcp_connect_ms: probe,
                ttfb_ms: 0,
                total_ms: 0,
                body_bytes: 0,
                outcome: FetchOutcome::TransportError,
                error: Some(format!("request build: {e}")),
                parse: None,
            }
        }
    };

    let start = Instant::now();
    let response = match tokio::time::timeout(timeout, client.request(req)).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            // Walk the error source chain for actionable detail
            // (e.g. "client error (Connect) -> invalid URL, scheme is not http").
            let mut msg = e.to_string();
            let mut src = std::error::Error::source(&e);
            while let Some(s) = src {
                msg.push_str(" -> ");
                msg.push_str(&s.to_string());
                src = s.source();
            }
            return FetchRecord {
                category: category.into(),
                url: url.into(),
                http_version: String::new(),
                status: 0,
                tcp_connect_ms: probe,
                ttfb_ms: start.elapsed().as_millis() as u64,
                total_ms: start.elapsed().as_millis() as u64,
                body_bytes: 0,
                outcome: FetchOutcome::TransportError,
                error: Some(msg),
                parse: None,
            };
        }
        Err(_) => {
            return FetchRecord {
                category: category.into(),
                url: url.into(),
                http_version: String::new(),
                status: 0,
                tcp_connect_ms: probe,
                ttfb_ms: timeout.as_millis() as u64,
                total_ms: timeout.as_millis() as u64,
                body_bytes: 0,
                outcome: FetchOutcome::Timeout,
                error: Some(format!("timed out after {}s", timeout.as_secs())),
                parse: None,
            }
        }
    };
    let ttfb = start.elapsed();

    let version = match response.version() {
        http::Version::HTTP_10 => "HTTP/1.0",
        http::Version::HTTP_11 => "HTTP/1.1",
        http::Version::HTTP_2 => "HTTP/2",
        http::Version::HTTP_3 => "HTTP/3",
        _ => "other",
    }
    .to_string();
    let status = response.status().as_u16();

    let is_html = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase().contains("text/html"))
        .unwrap_or(false);

    let body = match tokio::time::timeout(
        timeout,
        http_body_util::BodyExt::collect(response.into_body()),
    )
    .await
    {
        Ok(Ok(b)) => b.to_bytes(),
        Ok(Err(e)) => {
            let elapsed = start.elapsed().as_millis() as u64;
            return FetchRecord {
                category: category.into(),
                url: url.into(),
                http_version: version,
                status,
                tcp_connect_ms: probe,
                ttfb_ms: ttfb.as_millis() as u64,
                total_ms: elapsed,
                body_bytes: 0,
                outcome: FetchOutcome::TransportError,
                error: Some(format!("body read: {e}")),
                parse: None,
            };
        }
        Err(_) => {
            let elapsed = start.elapsed().as_millis() as u64;
            return FetchRecord {
                category: category.into(),
                url: url.into(),
                http_version: version,
                status,
                tcp_connect_ms: probe,
                ttfb_ms: ttfb.as_millis() as u64,
                total_ms: elapsed,
                body_bytes: 0,
                outcome: FetchOutcome::Timeout,
                error: Some("body read timed out".into()),
                parse: None,
            };
        }
    };
    let total = start.elapsed();
    let body_len = body.len() as u64;

    let outcome = if !(200..300).contains(&status) {
        FetchOutcome::HttpError
    } else if !is_html || body.is_empty() {
        FetchOutcome::ParseError
    } else {
        FetchOutcome::Ok
    };

    let parse_stats: Option<ParseStats> = if outcome == FetchOutcome::Ok {
        // The body was collected into Bytes; parse on a blocking task since
        // tree construction is CPU-bound (2-core runner friendly).
        let bytes_vec = body.to_vec();
        let parsed =
            tokio::task::spawn_blocking(move || parse::parse_html_bytes(&bytes_vec))
                .await
                .ok();
        match parsed {
            Some(Ok(s)) => Some(s),
            Some(Err(e)) => {
                return FetchRecord {
                    category: category.into(),
                    url: url.into(),
                    http_version: version,
                    status,
                    tcp_connect_ms: probe,
                    ttfb_ms: ttfb.as_millis() as u64,
                    total_ms: total.as_millis() as u64,
                    body_bytes: body_len,
                    outcome: FetchOutcome::ParseError,
                    error: Some(format!("parse: {e}")),
                    parse: None,
                }
            }
            None => None,
        }
    } else {
        None
    };

    FetchRecord {
        category: category.into(),
        url: url.into(),
        http_version: version,
        status,
        tcp_connect_ms: probe,
        ttfb_ms: ttfb.as_millis() as u64,
        total_ms: total.as_millis() as u64,
        body_bytes: body_len,
        outcome,
        error: None,
        parse: parse_stats,
    }
}

/// A shared client handle.
pub struct Fetcher {
    pub client: HttpsClient,
}

impl Fetcher {
    pub fn new() -> Self {
        Fetcher {
            client: build_client(),
        }
    }
}

impl Default for Fetcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Read this process's current RSS in KiB from /proc (Linux). Returns None on
/// non-Linux platforms so the harness stays portable for CI.
pub fn current_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kib: u64 = rest.trim().trim_end_matches(" kB").trim().parse().ok()?;
            return Some(kib);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_reader_reads_something_real() {
        // On Linux CI this must produce a plausible RSS (>= 1 MiB, <= 4 GiB).
        if let Some(kib) = current_rss_kib() {
            assert!(kib >= 1024, "rss too small: {kib} KiB");
            assert!(kib <= 4 * 1024 * 1024, "rss implausible: {kib} KiB");
        }
        // Non-Linux: None is acceptable (documented platform behavior).
    }

    #[tokio::test]
    async fn transport_error_on_bad_host() {
        let f = Fetcher::new();
        let rec = fetch_and_parse(
            &f.client,
            "test",
            "https://brow-phase6-invalid-host.invalid/",
            Duration::from_secs(5),
            true,
        )
        .await;
        assert_eq!(rec.outcome, FetchOutcome::TransportError);
        assert_eq!(rec.status, 0);
        assert!(rec.error.is_some(), "error detail must be captured");
    }

    #[tokio::test]
    async fn transport_error_on_malformed_url() {
        let f = Fetcher::new();
        let rec = fetch_and_parse(
            &f.client,
            "test",
            "not a url at all",
            Duration::from_secs(5),
            false,
        )
        .await;
        assert_eq!(rec.outcome, FetchOutcome::TransportError);
        assert!(rec.tcp_connect_ms.is_none());
        assert!(rec.error.unwrap().contains("invalid url"));
    }
}
