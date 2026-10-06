/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Criterion benchmarks for the brow-privacy filter engine,
//! including the real vendored EasyList snapshot.

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use url::Url;

use brow_privacy::filter::FilterEngine;
use brow_privacy::fingerprint::{build_payload, DefenseLevel, FingerprintConfig, SessionKey};

const EASYLIST: &str = include_str!("../assets/easylist-snapshot.txt");

const SITE: &str = "https://www.news-site.example/article/2026/brow-phase4";
const URLS: &[&str] = &[
    // typical ad/tracker shapes
    "https://pagead2.googlesyndication.com/pagead/show_ads.js",
    "https://securepubads.g.doubleclick.net/tag/js/gpt.js",
    "https://www.google-analytics.com/analytics.js",
    "https://connect.facebook.net/en_US/fbevents.js",
    "https://cdn.taboola.com/libtrc/loader.js",
    // benign shapes
    "https://www.news-site.example/static/css/main.css",
    "https://fonts.gstatic.com/s/roboto/v30/KFOmCnqEu92Fr1Mu4mxK.woff2",
];

fn engine() -> FilterEngine {
    FilterEngine::from_lists(&[EASYLIST])
}

fn bench_engine_build(c: &mut Criterion) {
    c.bench_function("engine_build_easylist_full", |b| {
        b.iter(|| FilterEngine::from_lists(black_box(&[EASYLIST])))
    });
}

fn bench_decisions(c: &mut Criterion) {
    let e = engine();
    let site = Url::parse(SITE).unwrap();
    let mut group = c.benchmark_group("should_block");
    for (i, u) in URLS.iter().enumerate() {
        let req = Url::parse(u).unwrap();
        group.bench_with_input(BenchmarkId::from_parameter(i), u, |b, _| {
            b.iter(|| {
                e.should_block(
                    black_box(&site),
                    black_box(&req),
                    1 << 1, // script
                    true,
                )
            })
        });
    }
    group.finish();
}

fn bench_fingerprint_payload(c: &mut Criterion) {
    let session = SessionKey::from_entropy();
    c.bench_function("fingerprint_payload_strict", |b| {
        b.iter(|| {
            build_payload(
                black_box(DefenseLevel::Strict),
                black_box(&session),
                &FingerprintConfig::default(),
            )
        })
    });
}

criterion_group!(
    benches,
    bench_engine_build,
    bench_decisions,
    bench_fingerprint_payload
);
criterion_main!(benches);
