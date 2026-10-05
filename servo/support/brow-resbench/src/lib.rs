/* brow-resbench library: sampling + runner APIs (the CLI is a thin wrapper). */

pub mod runner;
pub mod sample;

pub use runner::{
    comparison_markdown, results_json, run_profile, BenchError, RunResult, TargetProfile,
};
pub use sample::{collect_tree, is_alive, sample_tree, ProcessSample, CLOCK_TICKS_PER_SEC};
