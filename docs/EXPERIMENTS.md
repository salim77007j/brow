# brow — Experiment Log

Append-only record of every experiment (PROCESS.md rule 0.5). One section per
experiment; update the Status line in place as results land, never rewrite history.

---

## E-001 — O2/O3 SIGSEGV codegen bisect matrix

- **Date started:** 2026-10-08
- **Branch:** `experiment/o2o3-codegen` (only place the experiment exists)
- **Workflow:** `.github/workflows/experiment-codegen-matrix.yml`
- **Run:** [37660240086](https://github.com/salim77007j/brow/actions/runs/37660240086)
  (first attempt 37660103020 failed at YAML parse — invalid GH expression, fixed in
  commit on the experiment branch; recorded here for honesty)
- **Hypothesis:** the v0.6.x O2/O3 SIGSEGV (Script thread, wikipedia repro, see
  `[profile.production]` comment in `servo/Cargo.toml`) is caused by the fat-LTO
  (`lto=true`) × high-opt-level interaction, not by opt-level alone — because the
  default release profile (O3, no LTO, 16 CGU) builds green in gated CI with a
  passing smoke test, and Firefox/Chrome ship ThinLTO-class LTO, not fat LTO.
- **Method:** build `brow-shell` under `[profile.production]` with single-knob
  CARGO_PROFILE_* overrides per variant; load
  `https://www.wikipedia.org/wiki/Servo_(software)` under Xvfb + software GL;
  classify exit as CRASH(signal)/PASS(alive 150s)/EXITED(rc).
- **Variants:** s-control (expect PASS) · o3-fat-repro (expect CRASH) ·
  o3-nolto · o3-thin · o3-fat-mozjsO1 (mozjs+mozjs-sys pinned to opt 1).
- **Anchor rule:** verdicts trusted only if s-control PASSES **and** o3-fat-repro
  CRASHES; otherwise the repro must be re-established on hardware before any
  conclusion (DECISIONS D-007).
- **Status:** ROUND 2 RUNNING (run
  [37678828465](https://github.com/salim77007j/brow/actions/runs/37678828465)
  dispatched 2026-10-08 after the harness fix; results appended below on completion).

### Round 1 — INVALID (harness bug, D-007 anchor rule fired as designed)

- Run 37660240086 completed with all **five** jobs failed, and **zero verdict
  lines** in any job summary.
- **Root cause (harness, not codegen):** the repro step launched
  `timeout 150 xvfb-run … brow-shell &` and later executed `wait "$PID"` in a
  `bash -e` step. Any non-zero `wait` (timeout-kill 124, SIGSEGV 13x, clean
  exit) trips `set -e` and aborts the step with that exit code **before** the
  verdict echo, the log copy, and the artifact upload. The only verdict path
  that could ever print (process still alive after the 155-iteration watch
  loop) is nearly unreachable because `timeout 150` fires first.
- **Raw data salvaged from job logs:** all five variants — s-control included —
  ended with `Process completed with exit code 124`, i.e. every browser
  instance was still alive when the 150 s window closed. **No SIGSEGV, no
  panic, no clean early exit in any variant.** Engine-build phases completed
  normally in all five (e.g. s-control `Finished production profile
  [optimized] target(s) in 16m 51s`).
- **Why this is not a verdict:** "alive" was never tied to page-load evidence
  (crash-test.log was never copied/uploaded on any path), so a browser parked
  on an error page would look identical to a loaded wikipedia. D-007 requires
  s-control PASS **and** o3-fat-repro CRASH; neither verdict was rendered.
- **Fix (experiment branch `90d74a22f`):** verdict now derived from
  `timeout(1)`'s own exit status (124 = PASS alive, ≥128 = CRASH signal N−128,
  else EXITED = red); engine load markers (`filter engine loaded`,
  `fingerprint defenses active` — same signals the product smoke gate uses)
  printed on every path; crash-test.log copied and uploaded even on red jobs;
  orphan cleanup after the window; verdict drives job color (green = alive).
- **Lesson (also relevant to CI design generally):** under `bash -e`, a bare
  `wait`/`grep`/`kill` whose non-zero result is *meaningful data* must be
  guarded (`cmd || RC=$?`), or the harness aborts before it reports. Same
  class of defect found and fixed the same day in the product smoke's RSS
  guardrail (`c362e09da`: it sampled the timeout wrapper's ~2 MB VmRSS instead
  of the browser's, making the 600 MB guardrail decorative).

<!-- Results template:
| Variant | Verdict | Notes |
|---|---|---|
| s-control | ... | ... |
Conclusion: ...
-->

### Round 2 — partial (3/5 verdicts; anchors pending on a stuck GHA runner)

Run [37678828465](https://github.com/salim77007j/brow/actions/runs/37678828465),
dispatched 2026-10-08 20:01 UTC with the fixed harness.

| Variant | Verdict | Evidence |
|---|---|---|
| o3-nolto (O3, no LTO, 16 CGU) | **PASS(alive 150s)** — job green | completed 20:27:40Z |
| o3-thin (O3, thin LTO, 1 CGU) | **PASS(alive 150s)** — job green | completed 20:40:17Z |
| o3-fat-mozjsO1 (O3, fat LTO, mozjs at O1) | **PASS(alive 150s)** — job green | completed 20:57:04Z |
| s-control | *pending* | stuck >110 min in `mach bootstrap` (GHA infra flake, not brow code — same step took ~9 min in round 1) |
| o3-fat-repro | *pending* | same |

- With the fixed harness, a green job **is** a rendered verdict: alive through
  the 150 s window, with load markers and log tail now emitted on every path.
- **No conclusion yet, by design (D-007).** The decision-bearing observations
  so far: the default release config (o3-nolto), the upstream-like middle
  point (o3-thin), and the mozjs-deoptimized fat config all stay up on CI —
  consistent with the hypothesis that fat-LTO×O3 is the suspect, but the
  `o3-fat-repro` anchor has not rendered its verdict, so nothing above may be
  read as a bisect result.
- **Collection protocol:** the run is left alive; the two stuck jobs have a
  330-min timeout. Their verdicts are appended to this table when they land
  (checked at Phase 2 start, PROCESS §6.3). If `o3-fat-repro` completes PASS:
  the crash does not reproduce under CI software-GL, and per D-007 the repro
  moves to owner hardware before any codegen change is proposed. If it
  completes CRASH: the matrix is fully trusted, and the Phase 4 profile fix
  proceeds on the fat-LTO implication with the o3-thin/mozjsO1 data as the
  fallback ladder inputs.
