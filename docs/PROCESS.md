# brow v0.7 — Development Process (risk-managed)

Governs all work on branch `v0.7-rebuild` toward v0.7.0. Established 2026-10-08
(Phase 0). Owner directives: maximum intelligence, minimum risk, root causes,
evidence for every claim, honest reporting.

## 1. Environment & work protection

- Clone fresh and verify `git rev-parse HEAD` against `git ls-remote origin
  refs/heads/main` at session start. The sandbox resets; the repo is the memory.
- **Never leave uncommitted work.** Commit + push at the end of every sub-item,
  never at the end of a phase only.
- Every sub-item commit message: WHAT changed, WHY, HOW it was verified.
- `docs/WORKLOG.md` gets an entry per sub-item (append-only, dated).
- All evidence (screenshots, logs, measurements) lives in
  `docs/evidence/<topic>/` **inside the repo**, never only on local disk.
- If the environment resets: clone, `git checkout v0.7-rebuild`, continue from
  the last pushed commit. Loss budget = zero commits.

## 2. Branch & rollback

- All product work: branch `v0.7-rebuild`. `main` receives only verified,
  squashed phase results (fast-forward merge or squash PR).
- Rollback points:
  - `v0.6.1-safe` = pre-rebuild release state (`7cc97381a`).
  - Every phase appends a tag `v0.7-phaseN-done` after owner verification.
- **Rollback drill for any bad change**: `git revert <commit>` on
  `v0.7-rebuild` (keeps history) — or `git reset --hard <last-good>` + force
  push if the branch is pre-share. Verify rollback by: fast CI jobs green +
  headless smoke renders example.com + binary size under gate.
- Time budget to revert any single change: < 5 minutes (single-commit changes,
  no mixed commits — enforced by review of our own history).

## 3. Build safety

- Verification ladder (cheapest first):
  1. `cargo fmt --check` + `cargo clippy` on the touched brow crates (local, fast).
  2. `cargo test` on the touched brow crates (local, fast).
  3. Full-crate `cargo check`/build of `brow-shell` — **CI only** (this sandbox
     has 2 cores / 3 GB RAM; the engine + SpiderMonkey build is infeasible
     locally; documented in DECISIONS D-003).
  4. Full engine build Linux + Windows — CI (quality gates, every push).
- If a change breaks a gate: **revert first**, debug in isolation, re-apply.
- Binary size gate: 160 MiB (see §5). Performance/memory regressions: stop,
  fix, then continue (owner rule 0.8).

## 4. Change isolation

- One logical change per commit. Mixed commits are forbidden.
- Risky experiments (O2/O3 segfault bisect, allocator swaps, WebRender config):
  branch `experiment/<topic>`, never `v0.7-rebuild`, never `main`. Every
  experiment documented in the experiment branch's `EXPERIMENT.md` (what,
  result, conclusion) before the branch is deleted or merged.
- Before/after evidence required for every performance/UI/memory change.

## 5. Quality gates (CI, every push — `.github/workflows/ci.yml`)

| Gate | Job | Fails when |
|---|---|---|
| Fast tests | `brow-net-core`, `brow-chrome-core`, `brow-privacy` | any test fails |
| Lints | `brow-lints` | clippy -D warnings or fmt drift on brow crates |
| Shell compile | `brow-shell-check` | brow-shell (Slint + libservo) does not compile |
| Engine build Linux | `build-servo-linux` | `mach build --release` fails |
| Product build | `build-servo-linux` | `cargo build -p brow-shell` fails |
| Headless smoke | `build-servo-linux` | servoshell does not render example.com; brow-shell does not start + load filter engine under Xvfb |
| Binary size | `build-servo-linux` | `brow` > **160 MiB** (`SIZE_GATE_MIB` in the size-gate step) |
| Engine build Windows | `build-servo-windows` | Windows compile breaks (the v0.6.0/v0.6.1 class of Windows-only defects) |
| RSS guardrail | `build-servo-linux` | brow-shell single-tab RSS under Xvfb > 600 MB (guardrail only; the 200 MB target is tracked in Phase 4, software-GL numbers are not comparable to real GPUs) |

- CI runs on pushes to `main`, `v0.7-rebuild`, and PRs, with per-ref
  concurrency (newest push cancels superseded runs).
- Branch protection: attempted via API with the repo token; result recorded in
  DECISIONS/RISKS. If the token lacks admin permission, gates are advisory and
  the discipline is procedural (this document) until the owner grants admin.

## 6. Incremental verification loop (every sub-item)

1. Local: fmt/clippy/tests on touched crates.
2. Push → CI full gates.
3. Before starting the next sub-item, the previous push's fast gates must be
   green; engine-build gates must be green before a phase is declared done.
4. Any regression → STOP, revert, isolate, re-apply. Owner rule 0.8.

## 7. Decision & risk records

- `docs/DECISIONS.md`: every architectural decision — context, choice,
  alternatives rejected, risks accepted. Append-only, dated, referenced by
  commit message (`Decision: D-00N`).
- `docs/RISKS.md`: risk register (likelihood × impact × mitigation × status).
  Updated every phase; no open critical risks allowed at release.
