/* brow-resbench CLI — drive resource benchmarks against any browser target. */

use std::path::PathBuf;

use brow_resbench::runner::{comparison_markdown, results_json, run_profile, RunResult};
use brow_resbench::{BenchError, TargetProfile};

const USAGE: &str = "\
brow-resbench — process-level resource benchmark harness

USAGE:
  brow-resbench run <profile.json> [--out <result.json>]
  brow-resbench compare <run1.json> <run2.json> [...] [--out <table.md>]
  brow-resbench profiles                 # print example profiles for known browsers

Profile JSON schema:
{
  \"name\": \"brow-10-tabs\",
  \"command\": \"/path/to/browser\",
  \"arg_template\": [\"--new-window\", \"{url}\"],
  \"tabs\": 10,
  \"urls\": [\"https://example.com\", ...],
  \"warmup_secs\": 10,
  \"active_secs\": 30,
  \"idle_secs\": 30,
  \"interval_ms\": 250
}
";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("{USAGE}");
        std::process::exit(2);
    }
    let action = args[1].as_str();
    let result = match action {
        "run" => cmd_run(&args[2..]),
        "compare" => cmd_compare(&args[2..]),
        "profiles" => {
            print_example_profiles();
            Ok(())
        }
        "--help" | "-h" | "help" => {
            print!("{USAGE}");
            Ok(())
        }
        other => {
            eprintln!("unknown action: {other}\n{USAGE}");
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        match e.downcast_ref::<BenchError>() {
            Some(b) => {
                eprintln!("benchmark error: {b}");
                std::process::exit(1);
            }
            None => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn read_profile(path: &str) -> Result<TargetProfile, Box<dyn std::error::Error>> {
    let raw = std::fs::read_to_string(path)?;
    let profile: TargetProfile = serde_json::from_str(&raw)?;
    Ok(profile)
}

fn cmd_run(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let profile_path = args.first().ok_or("usage: run <profile.json>")?;
    let profile = read_profile(profile_path)?;
    eprintln!(
        "running profile '{}' — {} tabs, warmup {}s, active {}s, idle {}s",
        profile.name, profile.tabs, profile.warmup_secs, profile.active_secs, profile.idle_secs
    );
    let result = run_profile(&profile)?;
    let json = results_json(std::slice::from_ref(&result));

    let summary = format!(
        "\n== {} ==\nRSS p50 {:.1} MiB | p95 {:.1} MiB | max {:.1} MiB\nactive CPU {:.2}% | idle CPU {:.3}% | idle wakeups {:.1}/s\nprocesses: {}\n",
        result.profile,
        result.rss_p50_bytes as f64 / 1048576.0,
        result.rss_p95_bytes as f64 / 1048576.0,
        result.rss_max_bytes as f64 / 1048576.0,
        result.active_cpu_percent,
        result.idle_cpu_percent,
        result.idle_wakeups_per_sec,
        result.process_count_max,
    );
    print!("{summary}");

    if let Some(out) = arg_value(args, "--out") {
        std::fs::write(PathBuf::from(out), json)?;
        eprintln!("results written");
    }
    Ok(())
}

fn cmd_compare(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if files.len() < 2 {
        return Err("compare needs >= 2 run JSON files".into());
    }
    let mut results = Vec::new();
    for f in files {
        let raw = std::fs::read_to_string(f)?;
        // A run file may hold a single result or an array.
        if let Ok(single) = serde_json::from_str::<RunResult>(&raw) {
            results.push(single);
        } else {
            results.extend(serde_json::from_str::<Vec<RunResult>>(&raw)?);
        }
    }
    let md = comparison_markdown(&results);
    print!("{md}");
    if let Some(out) = arg_value(args, "--out") {
        std::fs::write(PathBuf::from(out), &md)?;
        eprintln!("table written");
    }
    Ok(())
}

fn print_example_profiles() {
    let examples = vec![
        TargetProfile {
            name: "brow-10-tabs".into(),
            command: "/usr/local/bin/brow".into(),
            arg_template: vec!["--url".into(), "{url}".into()],
            tabs: 10,
            urls: vec![
                "https://en.wikipedia.org/wiki/Web_browser".into(),
                "https://news.ycombinator.com".into(),
                "https://developer.mozilla.org".into(),
            ],
            warmup_secs: 10,
            active_secs: 30,
            idle_secs: 30,
            interval_ms: 250,
        },
        TargetProfile {
            name: "chrome-10-tabs".into(),
            command: "/usr/bin/google-chrome".into(),
            arg_template: vec!["--new-window".into(), "{url}".into()],
            tabs: 10,
            urls: vec![
                "https://en.wikipedia.org/wiki/Web_browser".into(),
                "https://news.ycombinator.com".into(),
                "https://developer.mozilla.org".into(),
            ],
            warmup_secs: 10,
            active_secs: 30,
            idle_secs: 30,
            interval_ms: 250,
        },
        TargetProfile {
            name: "firefox-10-tabs".into(),
            command: "/usr/bin/firefox".into(),
            arg_template: vec!["--new-tab".into(), "{url}".into()],
            tabs: 10,
            urls: vec![
                "https://en.wikipedia.org/wiki/Web_browser".into(),
                "https://news.ycombinator.com".into(),
                "https://developer.mozilla.org".into(),
            ],
            warmup_secs: 10,
            active_secs: 30,
            idle_secs: 30,
            interval_ms: 250,
        },
        TargetProfile {
            name: "brave-10-tabs".into(),
            command: "/usr/bin/brave-browser".into(),
            arg_template: vec!["--new-window".into(), "{url}".into()],
            tabs: 10,
            urls: vec![
                "https://en.wikipedia.org/wiki/Web_browser".into(),
                "https://news.ycombinator.com".into(),
                "https://developer.mozilla.org".into(),
            ],
            warmup_secs: 10,
            active_secs: 30,
            idle_secs: 30,
            interval_ms: 250,
        },
    ];
    for p in examples {
        println!("{}", serde_json::to_string_pretty(&p).unwrap());
    }
}
