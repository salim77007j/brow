# brow

**An ultra-lightweight, ultra-fast, privacy-first web browser built on the Servo rendering engine, written in Rust.**

```
     _          _
    | |__   ___| | __ _ _   _
    | '_ \ / _ \ |/ _` | | | |
    | |_) |  __/ | (_| | |_| |
    |_.__/ \___|_|\__,_|\__, |
                        |___/
```

## Vision

Every mainstream browser in 2026 either spends hundreds of megabytes of RAM per tab,
phones home to an advertising company, or both. **brow** is our answer:

- **Engine**: [Servo](https://github.com/servo/servo) — the parallel, Rust-native
  rendering engine (Stylo CSS engine + WebRender GPU compositor + SpiderMonkey).
- **Privacy**: network-level ad/tracker blocking, CNAME uncloaking, anti-fingerprinting
  that is *internally consistent* and therefore undetectable, cookie partitioning.
- **Efficiency**: hard target of **< 100 MB RAM per tab**, near-zero idle CPU,
  aggressive tab hibernation and resource sharing.
- **Trust**: 100% Rust core, memory-safe, open source, no telemetry, ever.

The engine is vendored at `servo/` and enhanced in-tree (see
[docs/SERVO_ARCHITECTURE_ANALYSIS.md](docs/SERVO_ARCHITECTURE_ANALYSIS.md) for the
full module map and our extension-point strategy).

## Project Phases

| Phase | Scope | Status |
|-------|-------|--------|
| 1 | Environment setup, Servo v0.6.0 integration, deep architecture study, CI | ✅ **Complete** |
| 2 | Engine enhancement & modernization (HTTP/3+QUIC, DoH/DoT, modern CSS/JS, hardening) | ✅ **Complete** |
| 3 | Ultra-lightweight UI + extreme resource optimization (<100 MB/tab) | ✅ **Complete** |
| 4 | Stealth ad blocker & privacy engine (2026-grade) | ⏸ Awaiting authorization |
| 5 | Build, package & cross-platform compilation (Windows/Linux) | ⏸ |
| 6 | Comprehensive testing & competitive validation vs Chrome | ⏸ |

Phase gates are strict: work on phase *N+1* starts only after explicit authorization.

## Repository Layout

```
brow/
├── servo/                          # Vendored upstream Servo v0.6.0 source (see docs/SERVO_UPSTREAM.md)
│   ├── ports/brow-shell/           # brow frontend: Slint chrome + engine embed (phase 3)
│   ├── support/brow-shell-core/    # chrome brain: tabs, lifecycle, stores, i18n, memwatch (phase 3)
│   ├── support/brow-cache/         # mmap content cache + cross-tab dedup pool (phase 3)
│   ├── support/brow-resbench/      # resource benchmark harness vs Chrome/Firefox/Brave (phase 3)
│   ├── support/brow-net-core/      # HTTP/3+QUIC, DoH, Alt-Svc, COOP/COEP/CORP engine (phase 2)
│   └── support/brow-bench/         # TTFB protocol benchmark (phase 2)
├── docs/                   # Architecture analysis, build guides (incl. PGO), upstream provenance
├── .github/workflows/      # CI (engine build + fast gates) and PGO pipeline
├── LICENSE                 # MPL-2.0 (same license family as Servo itself)
├── PHASE_1_REPORT.md       # Phase 1 completion report
├── PHASE_2_REPORT.md       # Phase 2 completion report
├── PHASE_3_REPORT.md       # Phase 3 completion report
└── README.md
```

## Quick Start (build the baseline engine)

```bash
# Debian/Ubuntu (see docs/BUILDING.md for the full dependency matrix)
cd servo
./mach bootstrap          # installs system deps + pinned Rust toolchain (1.97.1)
./mach build --release    # ~2 h on 4-core CI runner
./mach run --release https://example.com
```

Headless render test (how we validate in CI):

```bash
cd servo && ./target/release/servo -z -o /tmp/shot.png https://example.com
```

## CI

[![brow CI](https://github.com/salim77007j/brow/actions/workflows/ci.yml/badge.svg)](https://github.com/salim77007j/brow/actions/workflows/ci.yml) [![PGO build](https://github.com/salim77007j/brow/actions/workflows/pgo-build.yml/badge.svg)](https://github.com/salim77007j/brow/actions/workflows/pgo-build.yml)

The workflow builds the vendored Servo tree from source on `ubuntu-24.04`,
smoke-tests the binary (`--version` + headless render), and uploads the artifact.

## License

MPL-2.0. Servo is © The Servo Project Developers and also MPL-2.0; vendored code in
`servo/` retains its upstream headers. See [LICENSE](LICENSE).
