# brow — Decision Record

Append-only. One entry per decision. Format: context → decision → rejected
alternatives → accepted risks. Referenced from commit messages (`Decision: D-00N`).

---

## D-001 — Strategy: finish brow on Servo (Option 2), not WebView2/CEF

**Date:** 2026-10-08 · **Decided by:** owner (repo owner directive)

**Context.** v0.6.1 real-world testing failed on performance, UI, stability.
Three strategic options were presented with evidence (two-window shell, CPU
chrome, size-optimized unstable engine, engine maturity ceiling). Options:
(1) rebase product on WebView2, (2) rebuild the shell on servoshell's
single-window pattern and fix the engine speed profile, (3) embed Chromium
(CEF).

**Decision.** Option 2 — keep the independent Rust engine (Servo), rebuild the
product shell properly, fix engine performance honestly. Mission: 90%+ of real
2026 websites, feel-fast interaction, professional single-window UI, zero
crashes, preserve the working privacy engine.

**Rejected alternatives.** (1) WebView2: fastest to competitive, but abandons
the engine identity and caps fingerprinting defense at JS-injection level.
(3) CEF: Chrome-class but kills the lightweight identity (150–250 MB binaries,
Chrome-class RAM) and the independence goal.

**Risks accepted.** Servo's layout/API gaps on complex sites are upstream;
"competitive with Chrome on every site" may be unreachable at any near date —
we target "works on 90%+, honest about the rest." O2/O3 segfault root cause is
unknown; if unfixable in Phase 4, v0.7 ships the best provably-stable profile
and documents the gap (owner rule: honesty over claims).

---

## D-002 — Branch, tag, and rollback model

**Date:** 2026-10-08

**Decision.** All work on `v0.7-rebuild`; `main` gets verified phase merges
only. Tag `v0.6.1-safe` (`7cc97381a`) is the rollback point to the last
released state. Phase completion tags `v0.7-phaseN-done` added as phases pass
owner verification. Reverts (not rewrites) are the default undo.

**Rejected.** Working on `main` directly (v0.6.1 era) — a broken push is then
user-visible; no isolation. Long-lived un-pushed local state — sandbox resets
would lose work.

**Risks accepted.** Branch lag vs main is nil (only this branch moves);
double-merge friction at phase end is deliberate (verification gate).

---

## D-003 — Local verification is limited to small crates; engine builds run in CI

**Date:** 2026-10-08

**Context.** The dev sandbox has 2 cores / 3 GB RAM. A full Servo +
SpiderMonkey build (or even `cargo check` triggering mozjs-sys's build script)
is infeasible locally; historical builds ran on GitHub Actions
(Linux ~1–2 h, Windows ~3–5 h).

**Decision.** Locally: fmt, clippy, unit tests for the brow protocol/core
crates (`brow-net-core`, `brow-shell-core`, `brow-privacy`, `brow-cache`) and
headless UI tests that don't pull the engine. In CI (every push): full
`brow-shell` compile check, full engine builds (Linux + Windows), headless
smoke of the released-style binaries, size and RSS gates. A sub-item is only
"done" when its CI gates are green.

**Rejected.** Local full builds (RAM-starved, would dominate session time and
still not cover Windows); check-only CI (the v0.6.x Windows-only FFI crash
class proves compile ≠ links ≠ runs).

**Risks accepted.** Iteration latency (engine-feedback loop is CI-length);
mitigated by isolating brow logic into engine-free crates where possible.

---

## D-004 — Binary size gate set at 160 MiB (single source of truth in CI)

**Date:** 2026-10-08

**Context.** v0.6.1 `brow` (Linux, `production-stripped`, opt-level="s") is
104.7 MiB. Phase 4 plans opt-level 3 + PGO, historically +10–25% code size.
CJK font bundles (Phase 3) add ~1–2 MB per face (OFL, subsettable).

**Decision.** Gate = **160 MiB** on the Linux `brow` binary
(`SIZE_GATE_MIB` in `ci.yml`, `build-servo-linux` job). Rationale: headroom
for opt-3+PGO (~125 MiB projected) + CJK fonts without blessing bloat; any
change that would blow the gate must justify itself (e.g. a new codec) or
subset/lazy-load instead.

**Rejected.** 120 MiB (blocks planned Phase 3/4 work); 250 MiB (no longer a
real constraint); no gate (v0.6 binary crept 45.8 → 92.1 MB tar.gz between
releases with no decision record).

**Risks accepted.** A legitimately large feature may require re-negotiating
the gate via a DECISIONS entry — that friction is intentional.

---

## D-005 — Windows full-engine build gates every push (cost accepted)

**Date:** 2026-10-08

**Context.** Owner directive 0.7: CI must check Linux + Windows on every push.
v0.6.0 shipped a Windows-inert memory governor and v0.6.1 hit Windows-only
FFI/allocator crashes — Windows breakage is a demonstrated, repeated failure
mode. Windows engine build ≈ 3–5 h per run.

**Decision.** `build-servo-windows` job on every push to `v0.7-rebuild`/`main`
(after fast tests pass), with per-ref concurrency cancellation so newer pushes
supersede queued Windows runs.

**Rejected.** Windows-on-demand only (leaves the door open for the exact bug
class that bit v0.6.x); Linux-only CI (violates owner directive).

**Risks accepted.** Long feedback loops during heavy phases; mitigated by
batching pushes per sub-item (not per file) and relying on fast local gates.
