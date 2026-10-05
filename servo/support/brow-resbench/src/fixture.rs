/* resbench-fixture — a deterministic memory/CPU target for harness tests and
 * smoke-checking the sampling pipeline. It allocates `--mb` MiB of anonymous
 * memory over `--alloc-sec` seconds (touching every page), then holds it
 * quietly for `--hold-sec` seconds. NOT a fake browser: it is a measurement
 * fixture, exactly like a load generator in a perf lab.
 */

use std::time::{Duration, Instant};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let get = |name: &str, default: u64| -> u64 {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let mb = get("--mb", 16);
    let alloc_sec = get("--alloc-sec", 1);
    let hold_sec = get("--hold-sec", 5);

    let total = mb * 1024 * 1024;
    let mut buffer: Vec<u8> = Vec::with_capacity(total as usize);

    // Grow and touch, so RSS actually reflects the allocation.
    let start = Instant::now();
    let chunk = 4 * 1024 * 1024; // 4 MiB
    let alloc_deadline = start + Duration::from_secs(alloc_sec.max(1));
    let mut written = 0u64;
    while written < total {
        let take = chunk.min((total - written) as usize);
        let pos = buffer.len();
        buffer.extend(std::iter::repeat(0xABu8).take(take));
        let _ = buffer[pos]; // touch the first byte of the new region
        written += take as u64;
        let elapsed = start.elapsed().as_secs_f64().max(0.001);
        let want = total as f64 * (elapsed / alloc_deadline.duration_since(start).as_secs_f64().max(0.001));
        if written as f64 > want {
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    // Hold quietly: no syscalls beyond a long sleep → near-zero idle CPU.
    std::thread::sleep(Duration::from_secs(hold_sec));
    // Keep the compiler from optimizing the buffer away; print one touched byte.
    println!("fixture done: {} MiB, first={:#x}", mb, buffer[0]);
}
