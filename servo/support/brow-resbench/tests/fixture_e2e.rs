/* End-to-end: the harness measures a REAL process (the fixture binary) and the
 * numbers must be physically sane: RSS >= the 40 MiB allocation, idle CPU ~0. */

use brow_resbench::{run_profile, TargetProfile};
use std::path::PathBuf;

fn fixture_path() -> PathBuf {
    // Provided by cargo to integration tests of the same package.
    std::env::var("CARGO_BIN_EXE_resbench-fixture")
        .expect("fixture binary must be built by cargo test")
        .into()
}

#[test]
fn fixture_measured_end_to_end() {
    let profile = TargetProfile {
        name: "fixture-40mib".into(),
        command: fixture_path().to_string_lossy().into(),
        arg_template: vec![
            "--mb".into(),
            "40".into(),
            "--alloc-sec".into(),
            "1".into(),
            "--hold-sec".into(),
            "6".into(),
        ],
        tabs: 1,
        urls: vec![],
        warmup_secs: 0,
        active_secs: 1,
        idle_secs: 2,
        interval_ms: 100,
    };
    let result = run_profile(&profile).expect("fixture run must succeed");
    assert!(result.samples >= 20, "expected dense samples: {}", result.samples);
    let mib = result.rss_max_bytes / (1024 * 1024);
    assert!(mib >= 35, "fixture allocated 40 MiB, measured {mib} MiB max");
    // Idle CPU must be ~0 (fixture sleeps, no work).
    assert!(
        result.idle_cpu_percent < 1.0,
        "idle CPU {}% must be < 1%",
        result.idle_cpu_percent
    );
    // Active phase covers the allocation burst: real work happened (>0) and
    // the sampler stays physically bounded (<= 100%).
    assert!(result.active_cpu_percent > 0.0, "active CPU must be > 0");
    assert!(result.active_cpu_percent <= 100.0);
}
