/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! brow Phase 6 validation harness.
//!
//! Three measurement surfaces, all real (no simulated data):
//! 1. `bench`  — 10 site categories × 10 real URLs: HTTPS fetch (hyper +
//!    rustls, the same crates brow's net stack is built from) → HTML5 parse
//!    (html5ever, the engine's own parser) → per-page metrics.
//! 2. `stress` — repeated load rounds over the category set: success rate,
//!    latency percentiles, throughput and self-RSS sampled from /proc.
//! 3. `parse`  — single-URL pipeline detail (fetch + parse breakdown).

pub mod bench;
pub mod http;
pub mod metrics;
pub mod parse;
pub mod report;
pub mod sites;
pub mod stress;

pub use metrics::{percentile, FetchOutcome, FetchRecord, ParseStats, StressSummary};
pub use sites::{CATEGORIES, SITE_COUNT};
