# brow v0.7 — Risk Register

Status: OPEN / MITIGATED / CLOSED. Updated at every phase end (owner rule 0.10).
Likelihood/Impact: LOW / MED / HIGH.

| # | Risk | L | I | Mitigation | Status |
|---|---|---|---|---|---|
| R-01 | **O2/O3 SIGSEGV root cause unknown** (wikipedia repro, Script thread). If unfixable, opt-3+PGO cannot ship. | MED | HIGH | Phase 4 dedicated bisect on `experiment/o2o3-segfault`; document every experiment; fallback = best stable profile + honest report. Last resort: toolchain/rust-toolchain bump test in isolation. | OPEN |
| R-02 | **Servo engine gaps on complex sites** (layout/API completeness) cap "works on 90% of 2026 web". | HIGH | HIGH | Phase 1 profiling identifies exact per-site failure modes; fixes limited to config/prefs/shell-level workarounds feasible in-tree; honest per-site verdict table in Phase 5 report; upstream-first contributions only if cheap. | OPEN |
| R-03 | **Windows-only regressions** (FFI, allocator, GPU/ANGLE path) — demonstrated bug class in v0.6.x. | MED | HIGH | Windows full-build gate on every push (D-005); Windows-specific changes require Windows CI green before merge; portable zip smoke run in Phase 5. | OPEN |
| R-04 | **Single-window rebuild regresses what works today** (v0.6.1 shell starts, renders, blocks trackers). | MED | HIGH | Phase 2 on `v0.7-rebuild` only; keep `brow-classic` servoshell binary as fallback shell in every release; headless smoke + screenshots before/after; revert-first discipline. | OPEN |
| R-05 | **IME/RTL work in a custom Slint platform** is unproven territory (Slint 1.17 software renderer pinned by mozjs icu pin). | MED | MED | Phase 3 spike first: prototype IME forwarding on `experiment/ime`; if Slint 1.17 blocks IME events, evaluate pin-lift vs custom key handling; document in DECISIONS before committing. | OPEN |
| R-06 | **Memory target < 200 MB/tab unreachable** on Servo (baseline 385–423 MB single tab incl. engine). | MED | MED | Phase 4 allocation audit (allocator, image cache, per-frame buffers); tab discard already shipped; target re-negotiated with owner if software-GL-independent floor proves higher — with measurements, not guesses. | OPEN |
| R-07 | **CI cost/latency** (Windows ~3–5 h per push) slows iteration. | HIGH | MED | Per-ref concurrency cancellation; fast gates run first and fail early; Windows job `needs` fast tests; batch pushes per sub-item. | MITIGATED |
| R-08 | **Branch protection unenforceable** if the PAT lacks admin scope. | MED | LOW | Attempted via API (recorded in WORKLOG); if 403: gates stay advisory + procedural discipline per PROCESS.md §5; owner can enable protection in UI settings in one click. | OPEN |
| R-09 | **Wikipedia black-artifact raster bug** may be a WebRender/WR-surfman interaction needing upstream debugging. | MED | MED | Phase 4: capture with `WR_DEBUG` overlays, SoftwareGL vs ANGLE matrix, minimize repro; if upstream-worthy, file against servo/servo with our repro and keep shipping workaround. | OPEN |
| R-10 | **Sandbox environment resets** mid-phase lose local state. | HIGH | LOW | Everything committed+pushed per sub-item; evidence inside repo; docs/WORKLOG.md is the re-entry point. | MITIGATED |
| R-11 | **Font licensing/size** for CJK bundling (Noto Sans CJK is ~20 MB per weight OTF). | MED | LOW | Subset to common ranges (e.g. pyftsubset) or ship language-pack download-on-demand; keep base installer under the D-004 size gate; OFL license allows both. | OPEN |
| R-12 | **Privacy engine regression** while touching net stack for perf (CNAME cache, filter hot path). | LOW | HIGH | brow-privacy + brow-net-core suites gate every push (EasyList snapshot tests, CHIPS policy tests); Phase 4 perf changes require those suites green + before/after bench from `brow-privacy --bench match_bench`. | MITIGATED |

## Phase-end checklist (tick in WORKLOG at each phase close)

- [ ] New risks discovered this phase added above with IDs.
- [ ] Statuses updated (nothing silently stale).
- [ ] No open critical risks at release time (v0.7.0 gate).
