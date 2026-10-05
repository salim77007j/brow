/* Real-process resource sampling on Linux via /proc.
 *
 * Measures, per target process tree:
 *   - RSS (VmRSS from /proc/<pid>/status), aggregated over the tree
 *   - CPU time (utime+stime+cutime+cstime from /proc/<pid>/stat)
 *   - voluntary/involuntary context switches (idle-wakeup proxy)
 *
 * No external crates: /proc text parsing only. All functions return
 * `Option`/`Result` and degrade gracefully if a process exits mid-sample.
 */

use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Point-in-time sample of one process tree.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ProcessSample {
    /// Sum of VmRSS across the tree, bytes.
    pub rss_bytes: u64,
    /// Sum of utime+stime across the tree, ticks.
    pub cpu_ticks: u64,
    /// Sum of voluntary context switches (lower = quieter idle).
    pub voluntary_ctxt_switches: u64,
    /// Sum of involuntary context switches.
    pub involuntary_ctxt_switches: u64,
    /// Number of live processes in the tree.
    pub process_count: u32,
}

impl ProcessSample {
    pub const ZERO: ProcessSample = ProcessSample {
        rss_bytes: 0,
        cpu_ticks: 0,
        voluntary_ctxt_switches: 0,
        involuntary_ctxt_switches: 0,
        process_count: 0,
    };

    pub fn diff_cpu(&self, earlier: &ProcessSample) -> u64 {
        self.cpu_ticks.saturating_sub(earlier.cpu_ticks)
    }
}

/// Linux tick rate for /proc/stat fields (USER_HZ is fixed at 100 on Linux).
pub const CLOCK_TICKS_PER_SEC: f64 = 100.0;

fn read_vm_rss(status_path: &Path) -> Option<u64> {
    let status = fs::read_to_string(status_path).ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

#[derive(Debug, Default)]
struct ProcStat {
    utime: u64,
    stime: u64,
    cutime: u64,
    cstime: u64,
    voluntary: u64,
    involuntary: u64,
}

fn read_stat_fields(stat_path: &Path) -> Option<ProcStat> {
    let raw = fs::read_to_string(stat_path).ok()?;
    // The comm field may contain spaces/parens: split after the LAST ')'.
    let after = raw.rsplit_once(')')?.1;
    let fields: Vec<&str> = after.split_whitespace().collect();
    // fields[0] is `state` (field 3); utime is field 14 → index 11.
    fn f(fields: &[&str], idx: usize) -> Option<u64> {
        fields.get(idx)?.parse().ok()
    }
    Some(ProcStat {
        utime: f(&fields, 11)?,
        stime: f(&fields, 12)?,
        cutime: f(&fields, 13)?,
        cstime: f(&fields, 14)?,
        voluntary: f(&fields, 8).unwrap_or(0),
        involuntary: f(&fields, 9).unwrap_or(0),
    })
}

/// All direct children PIDs of `pid` (from /proc/<pid>/task/<tid>/children).
fn direct_children(pid: u32) -> Vec<u32> {
    // Walk tasks: children may be attached to any thread's children file.
    let mut out = Vec::new();
    let task_dir = Path::new("/proc").join(pid.to_string()).join("task");
    if let Ok(tasks) = fs::read_dir(&task_dir) {
        for task in tasks.flatten() {
            let children_path = task.path().join("children");
            if let Ok(text) = fs::read_to_string(&children_path) {
                out.extend(text.split_whitespace().filter_map(|p: &str| p.parse::<u32>().ok()));
            }
        }
    }
    out
}

/// Recursively collect the process tree rooted at `pid` (BFS with cycle guard).
/// The root is only included if it is still alive.
pub fn collect_tree(pid: u32) -> Vec<u32> {
    let mut seen = HashMap::new();
    let mut queue = vec![pid];
    let mut tree = Vec::new();
    while let Some(p) = queue.pop() {
        if seen.insert(p, true).is_some() {
            continue;
        }
        // Skip vanished processes (exited between spawn and sample).
        if !Path::new("/proc").join(p.to_string()).join("stat").exists() {
            continue;
        }
        tree.push(p);
        for child in direct_children(p) {
            if !seen.contains_key(&child) {
                queue.push(child);
            }
        }
    }
    tree
}

/// Sample the whole process tree of `pid`. Missing processes contribute 0.
pub fn sample_tree(pid: u32) -> ProcessSample {
    let mut sample = ProcessSample::ZERO;
    for p in collect_tree(pid) {
        let base = Path::new("/proc").join(p.to_string());
        sample.rss_bytes += read_vm_rss(&base.join("status")).unwrap_or(0);
        if let Some(stat) = read_stat_fields(&base.join("stat")) {
            sample.cpu_ticks += stat.utime + stat.stime + stat.cutime + stat.cstime;
            sample.voluntary_ctxt_switches += stat.voluntary;
            sample.involuntary_ctxt_switches += stat.involuntary;
        }
        sample.process_count += 1;
    }
    sample
}

/// Whether a process is still alive.
pub fn is_alive(pid: u32) -> bool {
    Path::new("/proc").join(pid.to_string()).join("stat").exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

    #[test]
    fn samples_self() {
        let sample = sample_tree(std::process::id());
        // On Linux a running test process always has RSS.
        assert!(sample.rss_bytes > 0, "self RSS must be measurable");
        assert!(sample.process_count >= 1);
        assert!(is_alive(std::process::id()));
    }

    #[test]
    fn samples_child_tree() {
        // Spawn a sleeping child and confirm the tree sees it (its RSS adds).
        let mut child = Command::new("sleep")
            .arg("2")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sleep must exist on Linux CI");
        let pid = child.id();
        std::thread::sleep(std::time::Duration::from_millis(150));
        let tree = collect_tree(std::process::id());
        assert!(tree.contains(&pid), "child must appear in tree: {tree:?}");
        let sample = sample_tree(std::process::id());
        assert!(sample.process_count >= 2);
        child.kill().ok();
        child.wait().ok();
    }

    #[test]
    fn dead_process_degrades_to_zero() {
        // Use a REAL pid that has exited: spawn, capture pid, reap.
        let mut child = std::process::Command::new("true")
            .spawn()
            .expect("true must exist");
        let pid = child.id();
        let _ = child.wait();
        assert!(!is_alive(pid), "pid {pid} should be reaped");
        let sample = sample_tree(pid);
        assert_eq!(sample, ProcessSample::ZERO);
    }
}
