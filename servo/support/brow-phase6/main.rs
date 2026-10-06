/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! brow-phase6 CLI: sites | bench | stress | parse

use std::path::PathBuf;
use std::time::Duration;

use brow_phase6::{bench, report, sites, stress};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("help");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    match mode {
        "sites" => cmd_sites(),
        "bench" => rt.block_on(cmd_bench(&args[1..])),
        "stress" => rt.block_on(cmd_stress(&args[1..])),
        "parse" => rt.block_on(cmd_parse(&args[1..])),
        _ => print_help(),
    }
}

fn print_help() {
    println!(
        "brow-phase6 — Phase 6 comparative validation harness

USAGE:
  brow-phase6 sites                          Print the 10 × 10 site matrix
  brow-phase6 bench  [opts]                  Fetch+parse benchmark over the matrix
      [--category ID] [--concurrency N] [--timeout S] [--out DIR] [--no-probe]
  brow-phase6 stress [opts]                  Sustained load over the matrix
      [--rounds N] [--timeout S] [--out DIR]
  brow-phase6 parse  --url URL [--out DIR]   Single-URL pipeline detail

Results are written as JSON + Markdown to --out (default: results/)."
    );
}

fn cmd_sites() {
    for c in sites::CATEGORIES {
        println!("{} — {}", c.id, c.label);
        for u in c.urls {
            println!("    {u}");
        }
    }
    println!("\n{} categories, {} URLs", sites::CATEGORIES.len(), sites::SITE_COUNT);
}

fn arg_value(args: &[String], key: &str) -> Option<String> {
    args.windows(2)
        .find(|w| w[0] == key)
        .map(|w| w[1].clone())
}

fn arg_flag(args: &[String], key: &str) -> bool {
    args.iter().any(|a| a == key)
}

fn out_dir(args: &[String]) -> PathBuf {
    arg_value(args, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("results"))
}

fn write_artifacts(dir: &PathBuf, stem: &str, json: &serde_json::Value, md: &str) {
    std::fs::create_dir_all(dir).expect("create out dir");
    let json_path = dir.join(format!("{stem}.json"));
    let md_path = dir.join(format!("{stem}.md"));
    std::fs::write(&json_path, serde_json::to_string_pretty(json).unwrap()).expect("write json");
    std::fs::write(&md_path, md).expect("write md");
    println!("artifacts: {} + {}", json_path.display(), md_path.display());
}

async fn cmd_bench(args: &[String]) {
    let cfg = bench::BenchConfig {
        timeout: Duration::from_secs(
            arg_value(args, "--timeout").and_then(|v| v.parse().ok()).unwrap_or(20),
        ),
        concurrency: arg_value(args, "--concurrency").and_then(|v| v.parse().ok()).unwrap_or(4),
        with_connect_probe: !arg_flag(args, "--no-probe"),
        only_category: arg_value(args, "--category"),
    };
    println!(
        "bench: timeout={}s concurrency={} probe={} category={:?}",
        cfg.timeout.as_secs(),
        cfg.concurrency,
        cfg.with_connect_probe,
        cfg.only_category
    );
    let (summaries, records) = bench::run_bench(cfg).await;
    println!("\n== Category summary ==");
    print!("{}", report::bench_markdown(&summaries));

    let json = serde_json::json!({
        "tool": "brow-phase6 bench",
        "categories": summaries,
        "records": records,
    });
    let mut md = String::from("# brow-phase6 — 10-category page benchmark\n\n");
    md.push_str(&report::bench_markdown(&summaries));
    md.push_str("\n## Per-URL detail\n\n");
    md.push_str(&report::detail_markdown(&records));
    write_artifacts(&out_dir(args), "bench", &json, &md);
}

async fn cmd_stress(args: &[String]) {
    let cfg = stress::StressConfig {
        rounds: arg_value(args, "--rounds").and_then(|v| v.parse().ok()).unwrap_or(3),
        timeout: Duration::from_secs(
            arg_value(args, "--timeout").and_then(|v| v.parse().ok()).unwrap_or(20),
        ),
        inter_round_pause: Duration::from_millis(500),
    };
    println!(
        "stress: rounds={} timeout={}s (sequential over the full matrix)",
        cfg.rounds,
        cfg.timeout.as_secs()
    );
    let (summary, records) = stress::run_stress(cfg).await;
    println!("\n== Stress summary ==");
    print!("{}", report::stress_markdown(&summary));

    let json = serde_json::json!({
        "tool": "brow-phase6 stress",
        "summary": summary,
        "records": records,
    });
    let mut md = String::from("# brow-phase6 — stress test\n\n");
    md.push_str(&report::stress_markdown(&summary));
    md.push_str("\n## Per-URL detail\n\n");
    md.push_str(&report::detail_markdown(&records));
    write_artifacts(&out_dir(args), "stress", &json, &md);
}

async fn cmd_parse(args: &[String]) {
    let url = match arg_value(args, "--url") {
        Some(u) => u,
        None => {
            eprintln!("parse: --url required");
            std::process::exit(2);
        }
    };
    let fetcher = brow_phase6::http::Fetcher::new();
    let rec = brow_phase6::http::fetch_and_parse(
        &fetcher.client,
        "single",
        &url,
        Duration::from_secs(30),
        true,
    )
    .await;
    println!(
        "status={} version={} ttfb={}ms total={}ms bytes={} outcome={:?}",
        rec.status, rec.http_version, rec.ttfb_ms, rec.total_ms, rec.body_bytes, rec.outcome
    );
    if let Some(p) = &rec.parse {
        println!(
            "parse: nodes={} elements={} max_depth={} text_chars={} parse={}us ({:.1}k nodes/s)",
            p.nodes,
            p.elements,
            p.max_depth,
            p.text_chars,
            p.parse_us,
            p.nodes_per_sec as f64 / 1000.0
        );
    }
    let json = serde_json::json!({ "tool": "brow-phase6 parse", "record": rec });
    write_artifacts(&out_dir(args), "parse", &json, &report::detail_markdown(&[rec.clone()]));
}
