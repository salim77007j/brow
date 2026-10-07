# brow — Worklog (v0.7 rebuild)

Append-only. One entry per sub-item. Newest at the bottom.
Entry format: `## YYYY-MM-DD · <phase>.<sub-item> · <commit>` then WHAT/WHY/VERIFIED.

---

## 2026-10-08 · Phase 0 setup · (this commit)

**WHAT.**
- Verified local HEAD == `origin/main` == `7cc97381a` (fresh clone after
  environment reset).
- Created rollback tag `v0.6.1-safe` (= v0.6.1 release state) and pushed it.
- Created working branch `v0.7-rebuild` (owner directive 0.2).
- Archived v0.6.1 real-binary audit evidence into the repo:
  `docs/evidence/v0.6.1-audit/` — released-binary screenshots (toolbar
  invisible / CJK tofu / black raster artifact on wikipedia), engine logs,
  measured single-tab RSS (385 MB example.com, 423 MB wikipedia, software-GL
  VM caveats documented in the README there).
- Wrote the process set: `docs/PROCESS.md` (rules 0.1–0.10 operationalized),
  `docs/DECISIONS.md` (D-001..D-005), `docs/RISKS.md` (R-01..R-12).

**WHY.** Phase 0 = risk reduction before any code: rollback point, branch
isolation, evidence preservation, decision/risk records. Owner rule 1: do not
skip it.

**VERIFIED.** `git rev-parse HEAD` == `git ls-remote origin refs/heads/main`;
tag pushed (confirmed by push output); evidence files present in-tree; no
product code touched in this commit.

**Branch protection (owner directive 0.7).** Attempted via GitHub API after
this commit; result recorded in the next WORKLOG entry.

---

## 2026-10-08 · Phase 0 · CI quality gates (`.github/workflows/ci.yml`)

**WHAT.**
- CI now triggers on pushes to `v0.7-rebuild` (was `main` only) + per-ref
  concurrency cancellation (newest push supersedes queued multi-hour runs).
- `build-servo-linux`: added (a) product build `cargo build --release -p
  brow-shell` (shares the engine dependency graph), (b) **binary size gate**
  — stripped `release`-profile brow-shell ≤ 200 MiB, actual size printed to
  the job summary every run (D-004 + amendment), (c) **product smoke** —
  brow-shell under Xvfb must load the privacy filter engine AND reach
  document parse (fingerprint-defenses log line) on example.com, with a 600 MB
  RSS guardrail (target-tracking happens in Phase 4; software-GL CI numbers
  are not real-GPU numbers).
- New `build-servo-windows` job: full engine + brow-shell compile on
  windows-2022 on every push (fast tests run first), smoke = servoshell
  `--version` + brow-shell.exe present. Mirrors release.yml's recipe minus
  packaging (LLVM 20.1, LIBCLANG_PATH, uv, mach fetch + bootstrap-gstreamer).
- Fast gates (`brow-net-core`, `brow-chrome-core`, `brow-privacy`,
  `brow-lints`) now run before both engine-build jobs so trivial failures
  never burn multi-hour builds.

**WHY.** Owner directive 0.7 (automated quality gates on every push) and the
v0.6.x lesson that Windows-only defects reach releases when Windows compiles
only at tag time (D-005).

**VERIFIED.** YAML parses (7 jobs, triggers correct); gate steps are
deterministic pass/fail; no product code touched. Real-gate validation
happens on this push's CI run — first green run also records the size/RSS
baselines for Phase 1 calibration.

---

## 2026-10-08 · Phase 0 · Branch protection active + first gated run validated

**WHAT.**
- Branch protection on `main` ENABLED via API (HTTP 200): required status
  checks = "Build Servo from source (Linux x86_64, release)" + "Build Servo
  + brow-shell (Windows x86_64, release)", strict up-to-date, force-push and
  deletion disabled. `enforce_admins` left **false** as the deliberate escape
  hatch (prevents protection deadlocks; bypass is an explicit, logged act).
- First gated CI run on `v0.7-rebuild` @ `11847e2fe`: run `37652133060`.
  Fast gates validated within minutes: `brow-net-core` ✅,
  `brow-shell-core`/`brow-cache`/`brow-resbench` ✅, clippy+fmt ✅;
  `brow-privacy` + `brow-shell-check` in progress; engine-build jobs queued
  behind them (Linux ~1–2 h, Windows ~3–5 h — results reviewed at Phase 1
  start per PROCESS §6.3).

**WHY.** Owner directive 0.7 ("if any gate fails, the push is rejected") is
now mechanically enforced on `main` instead of being advisory; directive 0.8
incremental verification begins with every subsequent sub-item.

**VERIFIED.** API response 200; run visible in Actions with the branch
trigger; job conclusions observed via the API, not assumed.

**Risk updates.** R-08 (branch protection unenforceable) → **CLOSED**;
R-07 (CI cost) → **MITIGATED** (concurrency + fast-gates-first observed
working). Next phase entry updates both in `docs/RISKS.md` at Phase 1 close.


