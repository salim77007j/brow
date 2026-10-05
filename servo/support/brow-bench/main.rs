/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! brow-bench: network protocol micro-benchmarks for the brow engine.
//!
//! Measures time-to-first-byte (TTFB) and transfer time for the same payload
//! across the protocol stack brow ships in Phase 2:
//!
//! * `h1` — HTTP/1.1, cleartext loopback
//! * `h2` — HTTP/2 prior-knowledge, cleartext loopback
//! * `h2s` — HTTP/2 over TLS 1.3 (shows the TLS handshake cost h3 also pays)
//! * `h3` — HTTP/3 over QUIC (TLS 1.3 inherent), via brow-net-core's H3Client
//!
//! Each run spins a real loopback server, performs N requests (optionally
//! after dropping the first warm-up round), and reports min/p50/p90/p99/max.

mod h3loop;
mod servers;

use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Proto {
    H1,
    H2,
    H2Tls,
    H3,
}

impl Proto {
    fn parse(s: &str) -> Option<Proto> {
        match s {
            "h1" | "h1.1" => Some(Proto::H1),
            "h2" => Some(Proto::H2),
            "h2s" | "h2-tls" => Some(Proto::H2Tls),
            "h3" => Some(Proto::H3),
            _ => None,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Proto::H1 => "HTTP/1.1",
            Proto::H2 => "HTTP/2 (h2c)",
            Proto::H2Tls => "HTTP/2 (TLS 1.3)",
            Proto::H3 => "HTTP/3 (QUIC)",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sample {
    /// Time from request dispatch to response head (TTFB).
    pub ttfb: Duration,
    /// Time from response head to full body received.
    pub body: Duration,
}

pub fn percentile(samples: &[Duration], p: f64) -> Duration {
    let mut sorted: Vec<Duration> = samples.to_vec();
    sorted.sort();
    if sorted.is_empty() {
        return Duration::ZERO;
    }
    let idx = (((sorted.len() as f64 - 1.0) * p).round() as usize).min(sorted.len() - 1);
    sorted[idx]
}

fn summarize(label: &str, samples: &[Sample]) {
    let ttfbs: Vec<Duration> = samples.iter().map(|s| s.ttfb).collect();
    let bodies: Vec<Duration> = samples.iter().map(|s| s.body).collect();
    println!("  {label}");
    println!(
        "    ttfb  min={:?} p50={:?} p90={:?} p99={:?} max={:?}",
        ttfbs.iter().min().unwrap_or(&Duration::ZERO),
        percentile(&ttfbs, 0.50),
        percentile(&ttfbs, 0.90),
        percentile(&ttfbs, 0.99),
        ttfbs.iter().max().unwrap_or(&Duration::ZERO),
    );
    println!(
        "    body  min={:?} p50={:?} p90={:?} p99={:?} max={:?}",
        bodies.iter().min().unwrap_or(&Duration::ZERO),
        percentile(&bodies, 0.50),
        percentile(&bodies, 0.90),
        percentile(&bodies, 0.99),
        bodies.iter().max().unwrap_or(&Duration::ZERO),
    );
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    let mode = args.next().unwrap_or_else(|| "ttfb".to_string());
    match mode.as_str() {
        "ttfb" => {
            let mut requests = 50usize;
            let mut size = 64 * 1024usize;
            let mut protos: Vec<Proto> = vec![Proto::H1, Proto::H2, Proto::H2Tls, Proto::H3];
            while let Some(arg) = args.next() {
                match arg.as_str() {
                    "--requests" => requests = args.next().and_then(|v| v.parse().ok()).unwrap_or(50),
                    "--size" => size = args.next().and_then(|v| v.parse().ok()).unwrap_or(64 * 1024),
                    "--proto" => {
                        let p = args
                            .next()
                            .and_then(|v| Proto::parse(&v))
                            .unwrap_or(Proto::H3);
                        protos = vec![p];
                    },
                    other => {
                        eprintln!("unknown arg: {other}");
                        eprintln!("usage: brow-bench ttfb [--requests N] [--size B] [--proto h1|h2|h2s|h3]");
                        std::process::exit(2);
                    },
                }
            }

            println!("brow-bench ttfb: {requests} sequential requests, {size} byte payload");
            let payload: Vec<u8> = std::iter::repeat_n(b'b', size).collect();

            for proto in protos {
                // Warm-up pass (fills caches, establishes connections) — discarded.
                let _ = run(proto, 5, &payload).await;
                let samples = run(proto, requests, &payload).await;
                println!("{:<18}", proto.name());
                summarize("steady-state:", &samples);
            }
        },
        "altsvc" => {
            // Parse-cost micro-benchmark for the Alt-Svc cache path.
            let header = r#"h3=":443"; ma=86400, h3-29="alt.example.com:8443"; ma=3600; persist=1"#;
            let mut cache = brow_net_core::altsvc::AltSvcCache::new();
            let rounds = 100_000;
            let start = Instant::now();
            for _ in 0..rounds {
                cache.update_from_header("https", "example.com", 443, header);
            }
            let elapsed = start.elapsed();
            println!(
                "altsvc: {rounds} parse+cache updates in {elapsed:?} ({:.0}/s)",
                rounds as f64 / elapsed.as_secs_f64()
            );
        },
        other => {
            eprintln!("unknown mode: {other}");
            eprintln!("modes: ttfb, altsvc");
            std::process::exit(2);
        },
    }
}

async fn run(proto: Proto, requests: usize, payload: &[u8]) -> Vec<Sample> {
    match proto {
        Proto::H1 | Proto::H2 => servers::run_plain(proto, requests, payload).await,
        Proto::H2Tls => servers::run_h2_tls(requests, payload).await,
        Proto::H3 => h3loop::run_h3(requests, payload).await,
    }
}
