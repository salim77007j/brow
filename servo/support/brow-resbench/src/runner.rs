/* Benchmark runner: launch real targets, sample real resources, aggregate. */

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::sample::{self, ProcessSample, CLOCK_TICKS_PER_SEC};

/// How a browser target is launched. `command` is the executable path,
/// `arg_template` is expanded with `{url}` and `{tab_index}` per tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetProfile {
    pub name: String,
    pub command: String,
    /// Extra args (expanded per tab with {url} {tab_index}).
    #[serde(default)]
    pub arg_template: Vec<String>,
    /// Number of tabs to open.
    pub tabs: u32,
    /// Load these URLs round-robin across tabs.
    pub urls: Vec<String>,
    /// Seconds to warm up before sampling starts.
    pub warmup_secs: u32,
    /// Seconds of active-use sampling.
    pub active_secs: u32,
    /// Seconds of idle sampling (the near-zero-idle-CPU measurement).
    pub idle_secs: u32,
    /// Sampling interval.
    #[serde(default = "default_interval_ms")]
    pub interval_ms: u64,
}

fn default_interval_ms() -> u64 {
    250
}

/// Aggregated results of one run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunResult {
    pub profile: String,
    pub tabs: u32,
    pub started_unix: u64,
    /// RSS series (bytes) sampled during the whole run.
    pub rss_series_bytes: Vec<u64>,
    /// RSS aggregate stats.
    pub rss_p50_bytes: u64,
    pub rss_p95_bytes: u64,
    pub rss_max_bytes: u64,
    /// Mean CPU percent during the active phase.
    pub active_cpu_percent: f64,
    /// Mean CPU percent during the idle phase — the "near-zero idle" metric.
    pub idle_cpu_percent: f64,
    /// Voluntary context switches per second during idle (wakeup proxy).
    pub idle_wakeups_per_sec: f64,
    pub process_count_max: u32,
    pub samples: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum BenchError {
    #[error("failed to launch target: {0}")]
    Launch(String),
    #[error("target exited early (status {0:?})")]
    TargetDied(Option<std::process::ExitStatus>),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

struct Launched {
    child: Child,
}

impl Launched {
    fn pid(&mut self) -> u32 {
        self.child.id()
    }
}

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((p / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// Run a single profile end-to-end and measure it.
pub fn run_profile(profile: &TargetProfile) -> Result<RunResult, BenchError> {
    let tabs = profile.tabs.max(1);
    let mut processes: Vec<Launched> = Vec::new();

    let started_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Launch one OS process per tab-profile instance. For real browsers that
    // spawn their own child processes the tree sampling in `sample_tree`
    // captures them all; multi-tab single-instance browsers can expand tabs
    // into repeated URL args inside `arg_template` via {tab_index}.
    let urls: Vec<String> = (0..tabs)
        .map(|i| {
            profile
                .urls
                .get(i as usize % profile.urls.len().max(1))
                .cloned()
                .unwrap_or_else(|| "about:blank".to_string())
        })
        .collect();

    for (tab_index, url) in urls.iter().enumerate() {
        let mut cmd = Command::new(&profile.command);
        for arg in &profile.arg_template {
            cmd.arg(arg.replace("{url}", url).replace("{tab_index}", &tab_index.to_string()));
        }
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        let child = cmd
            .spawn()
            .map_err(|e| BenchError::Launch(format!("{}: {e}", profile.command)))?;
        processes.push(Launched { child });
        if tab_index + 1 < urls.len() {
            std::thread::sleep(Duration::from_millis(300));
        }
    }

    let result = sample_phases(profile, &mut processes, started_unix);

    // Cleanup.
    for mut p in processes {
        let _ = p.child.kill();
        let _ = p.child.wait();
    }
    result
}

fn sample_phases(
    profile: &TargetProfile,
    processes: &mut [Launched],
    started_unix: u64,
) -> Result<RunResult, BenchError> {
    let interval = Duration::from_millis(profile.interval_ms.max(50));
    let mut rss_series: Vec<u64> = Vec::new();
    let mut process_count_max: u32 = 0;

    // Phase 1: warmup (no recording).
    let warmup_end = Instant::now() + Duration::from_secs(profile.warmup_secs as u64);
    while Instant::now() < warmup_end {
        for p in processes.iter_mut() {
            if let Some(status) = p.child.try_wait()? {
                return Err(BenchError::TargetDied(status.code().map(|_| status)));
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    // Phase 2: active sampling.
    let mut first_active: Option<ProcessSample> = None;
    let mut last_active: Option<ProcessSample> = None;
    let active_end = Instant::now() + Duration::from_secs(profile.active_secs.max(1) as u64);
    while Instant::now() < active_end {
        let mut tree_sample = ProcessSample::ZERO;
        for p in processes.iter_mut() {
            let s = sample::sample_tree(p.pid());
            tree_sample.rss_bytes += s.rss_bytes;
            tree_sample.cpu_ticks += s.cpu_ticks;
            tree_sample.voluntary_ctxt_switches += s.voluntary_ctxt_switches;
            tree_sample.involuntary_ctxt_switches += s.involuntary_ctxt_switches;
            tree_sample.process_count += s.process_count;
        }
        process_count_max = process_count_max.max(tree_sample.process_count);
        if first_active.is_none() {
            first_active = Some(tree_sample);
        }
        last_active = Some(tree_sample);
        rss_series.push(tree_sample.rss_bytes);
        std::thread::sleep(interval);
    }

    // Phase 3: idle sampling (near-zero idle CPU measurement).
    let mut first_idle: Option<ProcessSample> = None;
    let mut last_idle: Option<ProcessSample> = None;
    let mut idle_voluntary_start = 0;
    let idle_end = Instant::now() + Duration::from_secs(profile.idle_secs.max(1) as u64);
    while Instant::now() < idle_end {
        let mut tree_sample = ProcessSample::ZERO;
        for p in processes.iter_mut() {
            let s = sample::sample_tree(p.pid());
            tree_sample.rss_bytes += s.rss_bytes;
            tree_sample.cpu_ticks += s.cpu_ticks;
            tree_sample.voluntary_ctxt_switches += s.voluntary_ctxt_switches;
            tree_sample.involuntary_ctxt_switches += s.involuntary_ctxt_switches;
            tree_sample.process_count += s.process_count;
        }
        if first_idle.is_none() {
            first_idle = Some(tree_sample);
            idle_voluntary_start = tree_sample.voluntary_ctxt_switches;
        }
        last_idle = Some(tree_sample);
        rss_series.push(tree_sample.rss_bytes);
        std::thread::sleep(interval);
    }
    let _ = idle_voluntary_start;

    // Aggregation.
    let mut sorted = rss_series.clone();
    sorted.sort_unstable();
    let (active_cpu_percent, idle_cpu_percent, idle_wakeups_per_sec) =
        match (&first_active, &last_active, &first_idle, &last_idle) {
            (Some(a0), Some(a1), Some(i0), Some(i1)) => {
                let active_ticks = a1.diff_cpu(a0);
                let active_secs = profile.active_secs.max(1) as f64;
                let active_cpu =
                    (active_ticks as f64 / CLOCK_TICKS_PER_SEC) / active_secs * 100.0;

                let idle_ticks = i1.diff_cpu(i0);
                let idle_secs = profile.idle_secs.max(1) as f64;
                let idle_cpu = (idle_ticks as f64 / CLOCK_TICKS_PER_SEC) / idle_secs * 100.0;

                let dvol = i1.voluntary_ctxt_switches.saturating_sub(i0.voluntary_ctxt_switches);
                let wakeups = dvol as f64 / idle_secs;
                (active_cpu, idle_cpu, wakeups)
            }
            _ => (0.0, 0.0, 0.0),
        };

    Ok(RunResult {
        profile: profile.name.clone(),
        tabs: profile.tabs,
        started_unix,
        rss_p50_bytes: percentile(&sorted, 50.0),
        rss_p95_bytes: percentile(&sorted, 95.0),
        rss_max_bytes: percentile(&sorted, 100.0),
        active_cpu_percent,
        idle_cpu_percent,
        idle_wakeups_per_sec,
        process_count_max,
        samples: rss_series.len(),
        rss_series_bytes: rss_series,
    })
}

/// Render a comparison table (Markdown) across profiles.
pub fn comparison_markdown(results: &[RunResult]) -> String {
    let mut out = String::new();
    out.push_str("| Profile | Tabs | RSS p50 (MiB) | RSS p95 (MiB) | RSS max (MiB) | Active CPU % | Idle CPU % | Idle wakeups/s | Procs |\n");
    out.push_str("|---|---:|---:|---:|---:|---:|---:|---:|---:|\n");
    for r in results {
        let mib = |b: u64| format!("{:.1}", b as f64 / (1024.0 * 1024.0));
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {:.2} | {:.3} | {:.1} | {} |\n",
            r.profile,
            r.tabs,
            mib(r.rss_p50_bytes),
            mib(r.rss_p95_bytes),
            mib(r.rss_max_bytes),
            r.active_cpu_percent,
            r.idle_cpu_percent,
            r.idle_wakeups_per_sec,
            r.process_count_max,
        ));
    }
    out
}

/// Serialize results to JSON (for artifact storage).
pub fn results_json(results: &[RunResult]) -> String {
    serde_json::to_string_pretty(results).unwrap_or_else(|_| "[]".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_basics() {
        assert_eq!(percentile(&[], 50.0), 0);
        assert_eq!(percentile(&[5], 50.0), 5);
        assert_eq!(percentile(&[1, 2, 3, 4], 50.0), 3); // round to idx 2
        assert_eq!(percentile(&[1, 2, 3, 4], 95.0), 4);
    }

    #[test]
    fn comparison_table_renders() {
        let r = RunResult {
            profile: "demo".into(),
            tabs: 3,
            started_unix: 0,
            rss_series_bytes: vec![],
            rss_p50_bytes: 100 * 1024 * 1024,
            rss_p95_bytes: 120 * 1024 * 1024,
            rss_max_bytes: 130 * 1024 * 1024,
            active_cpu_percent: 4.5,
            idle_cpu_percent: 0.02,
            idle_wakeups_per_sec: 3.0,
            process_count_max: 7,
            samples: 10,
        };
        let md = comparison_markdown(&[r.clone()]);
        assert!(md.contains("demo"));
        assert!(md.contains("100.0"));
        assert!(md.contains("0.020"));
        assert!(serde_json::from_str::<serde_json::Value>(&results_json(&[r])).is_ok());
    }
}
