# brow — PGO + LTO Build Guide (Phase 3)

brow's <100 MB/tab and speed targets depend not only on runtime mechanisms
(tab sleeping, throttling, dedup) but also on **how the binary is built**.
This document defines brow's optimized build profiles and the exact
Profile-Guided Optimization (PGO) procedure for the engine shell.

## 1. Build profiles

The servo workspace defines these profiles (see `servo/Cargo.toml`):

| Profile        | Purpose                                   | Key flags |
|----------------|-------------------------------------------|-----------|
| `dev`          | Development                               | opt-level 1, incremental |
| `release`      | Default optimized build                   | opt-level 3, thin LTO |
| `pgo-release`  | Final PGO-optimized binary (brow product) | opt-level 3, **fat LTO**, `codegen-units=1`, `panic=abort`(shell), PGO flags via `RUSTFLAGS` |
| `pgo-train`    | Instrumented build used to collect profiles | `-Cprofile-generate`, thin LTO |

Additional size-oriented settings applied for the final binary in Phase 5:

- `strip = "symbols"` — remove debug/symbol tables (keeps panic locations).
- `--remap-path-prefix` — scrub home directories from the binary.
- mimalloc as the global allocator (`brow-shell` default feature
  `brow-allocator-mimalloc`) — tighter size classes reduce resident overhead
  vs jemalloc for many-small-object browser workloads. servoshell keeps
  jemalloc (`use-jemalloc`) so both configurations remain measurable.

## 2. Why PGO matters for brow

Servo spends most cycles in layout (Stylo), JavaScript (SpiderMonkey) and
2D rasterization. These are branchy, hot-path-heavy workloads where PGO
typically wins **5–12% throughput** over an identical LTO build. Combined
with fat LTO + `codegen-units=1` the shell binary gets better inlining and
smaller code size (helps I-cache hit rate, which is the memory/speed
trade-off brow cares about).

## 3. Two-stage PGO build (Linux x86_64 example)

### Stage A — instrumented build + training

```bash
cd servo
export RUSTFLAGS="-Cprofile-generate=/tmp/brow-pgo"
cargo build --release -p servoshell   # or -p brow-shell
export LLVM_PROFILE_FILE="/tmp/brow-pgo/servo-%p-%m.profraw"

# Train with real workloads (each exercises different hot paths):
./target/release/servoshell --headless tests/html/scrollbar.html
./target/release/servoshell https://en.wikipedia.org/wiki/Rust_(programming_language)   # layout + CSS
./target/release/servoshell https://www.youtube.com    # JS + media + canvas
./target/release/servoshell https://news.ycombinator.com  # DOM-heavy small pages
# ...interact briefly, then close. The browser writes .profraw on exit
# (servoshell's schedule_exit hook also calls __llvm_profile_write_file).
```

### Stage B — merge profiles + optimized build

```bash
llvm-profdata merge -output=/tmp/brow-pgo/merged.profdata /tmp/brow-pgo
# llvm-profdata ships with the rustup llvm-tools component:
#   rustup component add llvm-tools
#   $(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/llvm-profdata

export RUSTFLAGS="-Cprofile-use=/tmp/brow-pgo/merged.profdata -Cllvm-args=-pgo-instr-value-profiling=0"
cargo build --profile pgo-release -p servoshell
```

Notes:

- On Linux, `LLVM_PROFILE_FILE` works without any in-tree hook because
  rustc's PGO runtime honors it natively. (The `llvm_pgo` cfg hook in
  servoshell covers Android/OHOS where the env var is unavailable; brow
  keeps it untouched.)
- Keep the *same* commit and dependency set between stage A and B.
- CI (`.github/workflows/pgo-build.yml`) automates both stages with a
  deterministic training set so artifacts are reproducible.

## 4. Allocator choice

| Allocator | Feature | When to use |
|-----------|---------|-------------|
| mimalloc | `brow-shell` (default) / `servo-allocator/use-mimalloc` | brow product builds — lowest RSS, strong tail latency |
| jemalloc | `servoshell` default / `servo-allocator/use-jemalloc` | comparison builds; required for `allocation-tracking` heap reports paths used by the memory dashboard |
| System   | neither | debug/portability only |

The two custom allocator features are mutually exclusive and enforced at
compile time (`compile_error!` in `components/allocator/lib.rs`).

## 5. Verifying the build

```bash
# Symbols stripped?
file target/pgo-release/brow | grep -q stripped && echo OK

# Profile actually applied? Look for "merged.profdata" in the build log
# and compare binary size vs plain release (expect ~5-15% smaller .text).

# Allocator: RSS smoke test with 10 tabs of example.com via brow-resbench.
```
