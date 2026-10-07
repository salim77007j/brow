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
- **Status:** RUNNING (5 matrix jobs dispatched 2026-10-08; ~2–3 h fat-LTO builds).
- **Results:** *to be appended below when the run completes.*

<!-- Results template:
| Variant | Verdict | Notes |
|---|---|---|
| s-control | ... | ... |
Conclusion: ...
-->
