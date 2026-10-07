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


---

## 2026-10-08 · Phase 1.1 · Environment re-verified + first gated run reviewed

**WHAT.**
- Sandbox was reset between sessions: restored git credentials (file outside
  the repo, mode 600), fresh-cloned `v0.7-rebuild`, verified
  `HEAD == origin/v0.7-rebuild == b6ee2f48e`; `v0.6.1-safe` tag confirmed on
  remote; local worklog re-read as the re-entry point (PROCESS 0.1).
- Reviewed the Phase 0 gated run `37652829943` @ `b6ee2f48e` (as pre-agreed at
  Phase 0 close): all five fast gates **green** (brow-privacy EasyList/CHIPS
  suite, clippy+fmt, brow-shell compile check, brow-net-core, brow-shell-core
  cache/resbench); Linux engine build **green** with **stripped release
  brow-shell = 147 MiB** (gate 200); Windows engine build still in progress
  (~1 h elapsed, within its 3–5 h budget). A superseded run (`37652133060`)
  was auto-cancelled by the concurrency group — working as designed.
- Observed (not acted on): a `brow release` run on `main` (`37651149310`,
  triggered at Phase 0 setup) has a Linux packaging failure (22 s) — noted for
  the Phase 6 packaging work; `main` is frozen, no action on the branch.

**WHY.** Owner rules: verify the environment after any reset; review heavy-gate
evidence before building on it (PROCESS 0.8); record baselines with evidence.

**VERIFIED.** `git ls-remote` vs `git rev-parse` match; job conclusions read
from the GitHub API; size line taken from the job log (`147 MiB (gate 200)`).

---

## 2026-10-08 · Phase 1.2 · 10-site v0.6.1 profiling (evidence in repo)

**WHAT.**
- Profiled the released v0.6.1 Linux binary on 10 real sites (example.com,
  wikipedia article, HN, docs.rs, github, MDN, BBC, old.reddit, x.com,
  youtube) under Xvfb + software GL: fresh profile per site, process-tree RSS
  at 200 ms, screenshots t25/t50, fixed dwell.
- Evidence + verdict table + caveats: `docs/evidence/phase1-site-profiling/`.

**Key findings.** Zero crashes on all 10 (shipped opt-"s" is stable). GitHub,
BBC, Wikipedia, MDN, docs.rs, HN render usable-to-excellent layouts — the
product failure is the *shell*, not these pages. CJK renders as tofu (no font
bundle); icon fonts render as solid black squares (MDN everywhere) — real
bug, re-scoped R-09. old.reddit server-blocks the Servo UA → R-14 (UA policy).
Tree RSS 270–615 MB/tab → R-06 baseline updated.
**Phantom avoided:** the "×" glyph on many screenshots is the X11 root cursor
(explained in the evidence README) — not a browser bug; would have wasted
Phase 4 effort.

**VERIFIED.** Harness `scripts/profile_sites.py` (sandbox); per-site
`result.json` committed; screenshots visually inspected, not assumed.

---

## 2026-10-08 · Phase 1.3 · servoshell blueprint fully read → docs/V2_PLAN.md

**WHAT.**
- Complete read of the in-tree single-window pattern:
  `desktop/app.rs` (winit 0.30 ApplicationHandler, ControlFlow::Wait),
  `desktop/headed_window.rs` (1358 lines: WindowRenderingContext +
  OffscreenRenderingContext compositing, full input routing, IME
  Enabled/Preedit/Commit/Disabled → CompositionEvent, ScaleFactorChanged
  handling, embedder controls, dialogs), `desktop/gui.rs` (EguiGlow chrome;
  webview composited via background-layer PaintCallback — GPU blit, no CPU
  path), `desktop/event_loop.rs`.
- Wrote `docs/V2_PLAN.md`: root causes A/B/C with evidence links, the §3
  blueprint (window/rendering, input, IME, DPI, what brow adds), the E-001
  bisect design + fallback ladder, target metrics + measurement methods,
  phase order, open questions. No product code changed (Phase 1 is research).

**WHY.** Phase 2 rebuilds the product shell on this exact pattern; the plan
must be specific enough that Phase 2/3/4 work items cite file-level evidence.

**VERIFIED.** All claims cite read source lines (file:line references in the
plan) or committed evidence directories.

---

## 2026-10-08 · Phase 1.4 · E-001 codegen bisect dispatched (experiment branch)

**WHAT.**
- Created `experiment/o2o3-codegen` (PROCESS 0.5: experiments live on
  dedicated branches, never main/`v0.7-rebuild`) with
  `experiment-codegen-matrix.yml`: 5 matrix variants building brow-shell
  under `[profile.production]` with single-knob CARGO_PROFILE_* overrides
  (s-control / o3-fat-repro / o3-nolto / o3-thin / o3-fat-mozjsO1), each
  loading the wikipedia repro under Xvfb + software GL; verdicts +
  crash logs to job summary and artifacts.
- Run [37660240086] dispatched (5 jobs building). First attempt (37660103020)
  failed at workflow-parse (invalid GH expression) — fixed and recorded in
  `docs/EXPERIMENTS.md` E-001.

**WHY.** R-01 is the single largest known performance blocker (forced
opt-level="s" for v0.6.x). The default release profile (O3, no LTO) building
green + Firefox/Chrome shipping ThinLTO-class LTO makes fat-LTO×O3 the prime
suspect; the matrix answers it in one parallel round.

**VERIFIED.** D-007 anchor rule recorded before results exist: no variant
verdict is trusted unless s-control PASSES and o3-fat-repro CRASHES; results
land in `docs/EXPERIMENTS.md` before any fix reaches `v0.7-rebuild`.

---

## 2026-10-08 · Phase 1.5 · Gate tightened on measured baseline (D-006) + risk register refresh

**WHAT.**
- `SIZE_GATE_MIB` 200 → 160 in `ci.yml` (measured 147 MiB baseline, D-006).
- `docs/RISKS.md`: R-09 re-scoped (icon-font glyph failure; × phantom
  resolved), R-05 updated (Slint concern dissolved by the egui/winit IME
  path; new risk = winit IME unvalidated on owner hardware), R-06 baseline
  updated (270–615 MB), new R-13 (bisect reproducibility — MITIGATED by the
  D-007 anchor rule), new R-14 (UA blocking — OPEN, Phase 4).
- `docs/DECISIONS.md`: D-006, D-007 appended.

**WHY.** D-004 promised tightening once the real baseline existed; risks must
be updated at every phase end (owner rule 0.10), including retiring phantoms.

**VERIFIED.** Gate value grep-verified in `ci.yml`; all risk rows cite
committed evidence; decision entries follow the template with rejected
alternatives and accepted risks.

---

## 2026-10-08 · Phase 1.6 · Process slip corrected (wrong-branch push)

**WHAT.**
- The four Phase 1 research commits were first pushed to
  `experiment/o2o3-codegen` by mistake (the checkout was still on the
  experiment branch after dispatching E-001 — the working tree never left
  `v0.7-rebuild`'s content, only the ref was wrong).
- Correction: cherry-picked all four onto `v0.7-rebuild`
  (`573bc2e93`..`adf958dd5`, pushed); force-reset `experiment/o2o3-codegen`
  back to its workflow-only commit `421c9110e`. No content difference existed
  between the two states — the experiment branch now contains ONLY the
  experiment workflow, as PROCESS 0.5 requires.

**WHY.** Change isolation (owner rule 0.4): experiment branches must never
carry mainline research/docs; any fix landing on `v0.7-rebuild` must go
through its own verified commits.

**VERIFIED.** `git ls-remote`: `experiment/o2o3-codegen = 421c9110e`
(workflow only), `v0.7-rebuild = adf958dd5`. Cherry-pick produced identical
trees (same file contents, new shas). Lesson recorded: after creating an
experiment branch, immediately switch back or open a worktree.


---

## 2026-10-08 · Phase 1.7 · Session re-entry, harness audit, E-001 round-2 dispatch, RSS guardrail fix

**WHAT.**
- Sandbox was reset between sessions; re-ran the 0.1 protocol: credentials
  restored (outside repo, mode 600), fresh clone, local HEAD == origin/main ==
  `7cc9738` (== `v0.6.1-safe`); discovered remote `v0.7-rebuild` already
  carried the completed Phase 0 and Phase 1.1–1.6 — aligned the local branch
  to `origin/v0.7-rebuild` (`a89c2d1`) instead of duplicating any of it
  (a redundant local commit was discarded *before* push; nothing force-pushed).
- Audited the gated run `37661290512` @ `adf958dd5`: **success** — all fast
  gates green, Linux engine build green (147 MiB vs 160 gate), Windows build
  green; branch protection on `main` re-verified live via API (required
  checks: both platform builds, strict, no force-push/deletion).
- E-001 round 1 (run 37660240086): **INVALID** — harness bug (`wait` under
  `bash -e` aborts before any verdict/evidence). Raw logs show all five
  variants (s-control included) alive at the 150 s kill; no SIGSEGV anywhere.
  Postmortem + fix recorded in `docs/EXPERIMENTS.md` (E-001).
- Fixed the E-001 harness on `experiment/o2o3-codegen` (`90d74a22f`) and
  re-dispatched: run [37678828465] (round 2, 5 variants).
- Fixed the product smoke's RSS guardrail on `v0.7-rebuild` (`c362e09da`):
  it sampled the timeout wrapper's VmRSS (decorative 600 MB gate); now samples
  the real brow-shell process. D-008 records the rule behind both fixes.
- Process notes: byte-level verification (`hexdump`) was used before recording
  the trigger-filter finding — the suspected `branches:` YAML corruption turned
  out to be a rendering artifact of the local toolchain (file bytes were valid
  `[main, v0.7-rebuild]` all along); **no false finding entered the repo**.

**WHY.** Owner rules 0.1 (verify environment/remote state after reset),
0.5 (experiments on dedicated branches with recorded results), 0.8 (gates must
measure reality), 0.9/0.10 (decisions and risks updated as they surface).

**VERIFIED.** All claims above cite API responses, job logs fetched from
run 37660240086, or byte-level file reads; round-2 run id recorded; both
fixes pushed and visible in remote history.
