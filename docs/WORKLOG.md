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

