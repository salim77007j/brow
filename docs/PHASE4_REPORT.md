# Phase 4 Report — Owner-Hardware Failure-Class Fixes

Status: **implementation complete, awaiting CI artifact + owner re-validation**.
Baseline: owner-tested artifact `a7c59a49e` (run 37939128861, Windows 11 +
AMD iGPU). Branch: `v0.7-rebuild`. Plan: `docs/PHASE4_PLAN.md` (EXECUTED).
Date: 2026-10-09.

---

## 0. Executive summary

All four owner-reported failure classes were root-caused (research phase,
5/5 lead spot-checks verified verbatim) and every queued fix was
implemented, committed, and pushed:

| # | Owner issue | Priority | Root cause | Fix commit | State |
|---|---|---|---|---|---|
| 1 | Fast-scroll lag + intermittent crashes | P0 | O3/no-LTO build = upstream #48109 segfault class; invisible because the Windows crash handler was a no-op (#48110); + wheel-per-tick script round trips; 76 px/line | `748d9fa83` (profile), `0324d9f6f` (capture), `b32f3e01c` (feel) | code-complete; crash class exited by construction |
| 2 | IME: separated words, no preedit | P0 | egui-winit re-disables `set_ime_allowed` every pass; Windows `Ime::Disabled`-after-commit blurred the editable | `c7f2ae367` | code-complete; 8 state-machine tests green |
| 3 | YouTube JS parse errors | P1 | silent mid-body truncation + cache poisoning; NOT the privacy engine | `4c73c2f9b` | code-complete; h3 truncation test green on a real QUIC loopback |
| 4 | General slowness | P1 | O3/no-LTO build + per-tick wheel round trips + no coalescing | `748d9fa83`, `b32f3e01c` | code-complete (profiling-based numbers deferred to owner A/B) |

Honest limitations are in §5; the new artifact and owner checklist are in §6.

## 1. Scroll crash (P0) — root cause and fix

**Root cause (locked).** The CI artifact was built with plain `--release`
while `[profile.release]` is UNDEFINED in `servo/Cargo.toml` — i.e. O3
codegen with NO LTO. That is exactly upstream servo/servo #48109's
segfault class (0xC0000005 on Windows, independent of opt-level, prevented
by LTO; reproducible on pure upstream). brow's own E-001 mystery (O2/O3
wikipedia segfault) was the same missing-LTO axis all along — R-01/E-001
CLOSED-EXPLAINED, D-007 anchor resolved. Crashes were invisible because
`crash_handler::install()` was a no-op on Windows (#48110) and stderr is
detached on double-click launch.

**Fixes.**
- **4.1 Windows crash capture** (`0324d9f6f`): `SetUnhandledExceptionFilter`
  + dbghelp `MiniDumpWriteDump` → `%LOCALAPPDATA%\brow\crashes\brow-minidump-<epoch>.dmp`;
  rolling `brow.log` (1 MiB rollover) records startup (version, git sha,
  build profile via baked `BROW_BUILD_PROFILE`), GL driver strings
  (`GL_VERSION`/`GL_RENDERER`), and every panic. The dump dir is surfaced
  in the startup log. macOS/Linux signal path unchanged.
- **4.2 Profile switch** (`748d9fa83`, D-015): CI Linux+Windows jobs build
  `--profile production-stripped` (LTO, codegen-units=1, opt-level="s" —
  the config family of every v0.6.x release artifact and of release.yml).
  By construction the artifact the owner tests next is OUTSIDE the #48109
  crash class. The 160 MiB size gate is unchanged (LTO binaries are the
  smaller ones historically).
- **R-15 hardening note**: the five catalogued Rust-panic candidates in the
  paint path are now DIAGNOSABLE (minidump + log) rather than invisible;
  defensive rewrites deliberately deferred until a real dump justifies
  them — fixing panics speculatively risks regressions in working paths.

**Verification.** windows-sys API surface validated item-by-item against
the vendored 0.61.2 sources; the whole Windows arm cross-compiles clean
(`cargo check --target x86_64-pc-windows-msvc`, features
Win32_Security/Win32_System_Memory were required for CreateFileW/
MiniDumpWriteDump — caught locally, not on CI). Platform-independent date
logic: 5 unit tests green. Runtime dump-write is verifiable only on owner
hardware by design — that is the point of 4.1.

## 2. IME composition (P0) — root cause and fix

**Root cause (locked), two independent shell bugs.**
1. egui-winit 0.34 re-calls `set_ime_allowed(ime.is_some())` on every egui
   pass, debounced against a private flag that never learns about
   servoshell's direct `set_ime_allowed(true)` from `show_ime` → next pass
   with an unfocused URL bar disables IME delivery → winit gates off
   WM_IME_* → raw WM_CHAR per keystroke = "separated words, no preedit".
2. Windows winit sends `Ime::Disabled` at the end of EVERY composition —
   including successful commits. servoshell mapped it to
   `ImeEvent::Dismissed` whenever the page owned the IME, and Dismissed
   blurs the focused editable → typing died after each commit.

**Fix** (`c7f2ae367`).
- `show_ime` records "IME already off" into egui-winit's debouncer
  (`set_allow_ime(false)` — its documented embedder sync point) so the
  per-pass re-disable can never cancel page IME.
- All four winit IME events route through a new pure state machine
  (`ports/servoshell/desktop/ime.rs`): commit-then-Disabled maps to
  SessionEnd (no blur); genuine cancellation/dismissal still blurs;
  macOS/Linux semantics preserved via a platform-parameterized constructor
  (they do not send Disabled after commit); engine-initiated hides never
  blur; preedit re-asserts `set_ime_cursor_area` so the candidate window
  stays anchored.

**Verification.** 8 unit tests green locally (shadow crate compiles the
real file with real keyboard-types 0.8 types), covering both platform
tables, fast commits without preedit, implicit starts, hides, re-arms.
The test suite caught a real semantic bug during development (engine-hide
during Composing wrongly blurred) — fixed and pinned by a test. True
inline preedit rendering (real composing range in TextInput) remains
upstream-track (deferred honestly; #20770-family, #46936 for isComposing).

## 3. YouTube JS parse errors (P1) — root cause and fix

**Root cause (locked).** Privacy engine EXONERATED (userscript payload
runs in the page realm; zero body rewriting — verified in research). The
errors were truncated script sources: `http_loader.rs` delivered partial
bodies as `Data::Done` on any non-decompression mid-body error; brow's
default-ON h3 path added 30 s chunk timeouts, pool eviction under
in-flight streams, and treated early FIN as clean EOF with no
Content-Length check anywhere; truncated bodies poisoned the memory cache
(live shared Arc) and the disk cache across restarts — hence persistent,
reproducible errors. Full write-up: `docs/UPSTREAM/youtube-status.md`.

**Fix** (`4c73c2f9b` + `55d42dfe6`).
1. Mid-body errors fail the resource: `Data::Error(ResourceLoadError)`,
   body marked empty, shared `aborted` flag set (also for decompression
   failures and cancelled fetches).
2. h3 pump counts WIRE bytes (pre-decompression data frames) and rejects a
   clean FIN contradicting Content-Length — exact check where it is sound.
3. Cache hygiene: disk flush skips aborted entries; serve/revalidate
   already rejected them; cache-format namespace `brow-cache-v2` makes
   pre-4.4 (possibly poisoned) disk entries unreachable and they age out
   under the size cap. D-017 documents why a serve-time CL equality check
   would be UNSOUND (stored bodies are post-decompression).
4. `network_http3_enabled` defaults FALSE for v0.7 (D-016) — the custom h3
   path ships opt-in until owner A/B validation.
5. New CI gate runs the engine net crate tests (`cargo test -p servo-net`).

**Verification.** Real QUIC loopback (quinn + h3 server, UDP localhost):
the new `/truncated` route (CL 100, 40 bytes, clean FIN) errs on collect —
`h3_truncated_body_is_an_error_not_a_partial_body` green locally (3/3 suite).
Engine-level: `test_truncated_response_body_is_a_network_error` green
(384/384 suite with documented R-18 skips). Honest scope note: the
fetch-level guarantee (partial bytes never reach the parser) is the
load-bearing fix; a script-level `error`-event test would need script-crate
plumbing and was not added.

## 4. Performance (P1) — root cause and fix

**Root cause (locked).** O3/no-LTO build (both the crash class and a
fatter binary than the LTO profiles); every wheel tick round-tripped
through the script thread (hit-test + DOM dispatch) before the compositor
scrolled; 76 px/line; no vsync (fixed 120 Hz timer); double render per
presented frame (WR → offscreen FBO → egui blit; architectural consequence
of the Phase 2 single-window chrome).

**Fix** (`748d9fa83`, `b32f3e01c`).
- Profile switch to LTO+opt-"s" (4.2) — same codegen as v0.6.x releases.
- Wheel coalescing per event-loop burst: N fast ticks = 1 script round
  trip + 1 compositor scroll (flush on next non-wheel event preserves DOM
  order; `about_to_wait` flushes before the loop sleeps).
- LINE_HEIGHT/LINE_WIDTH 76 → 100 px (Firefox/Windows ballpark; upstream
  #38072 direction).
- NOT attempted (honest): momentum/fling; vsync; the double-render
  architecture; PGO/BOLT (deferred to Phase 5/6 with measurements);
  memory <200 MB audit (R-06 — the profile switch alone will not get
  there). A/B numbers vs the a7c59a49e artifact are the owner's 4.7
  validation step.

## 5. Honest limitations and deferred items

- **YouTube remains degraded-but-functional at best** (R-17): fixing
  truncation removes the parse-error class; upstream #47963's missing
  `Animation`/`SVGAnimatedString` API gap is upstream-scale and inherited
  verbatim. `[bugsnag] No valid entry type provided to observe()` is
  servo's own PerformanceObserver warning (cosmetic, unrelated).
- **True inline preedit UI** (composition string rendered in the input
  with underline/caret) — upstream-track; the shell now forwards sessions
  faithfully, so this is no longer a data-loss bug, only a visual gap.
- **isComposing=false** — upstream #46936; blueprint noted; we hold the
  shell-side session state a patch would need.
- **PGO/BOLT, RSS < 200 MB, vsync, momentum** — deferred with reasons in
  PHASE4_PLAN §3.
- **R-18** — six pre-existing cookie test failures + one flaky filemanager
  test (fail on the pre-4.4 baseline too; verified) are skipped in the CI
  gate with an explicit list; also fixed a latent repo hygiene bug: the
  `*.key` gitignore rule had silently excluded the testing TLS key since
  Phase 1 (restored from upstream, modulus-verified against the vendored
  crt).
- **E-002 (optional)** — O3+LTO as a *performance* experiment is NOT
  adopted; it is a new untested configuration. v0.7 ships the
  proven-stable LTO+opt-"s".

## 6. Deliverable artifact + owner validation checklist

- **Artifact**: `brow-servo-windows-x86_64` from the first green run on or
  after commit `55d42dfe6` (Actions → brow CI → build-servo-windows →
  portable zip). Contains all fixes 4.1–4.6.
- **Crash capture (P0-1)**: after any crash, check
  `%LOCALAPPDATA%\brow\crashes\` for `brow-minidump-*.dmp` + `brow.log`
  (startup lines include version/profile/GL strings). Then re-run the
  10-minute aggressive fast-scroll test on the same pages as before.
  Expected: no crash (LTO class exited); if one occurs, it is now
  actionable.
- **IME (P0-2)**: re-run the Phase 3 IME matrix §1 (zh pinyin / ja / ar /
  he in URL bar + page form). Expected: no "separated words" (IME stays
  enabled during page typing) and typing does not die after each commit.
- **YouTube (P1-3)**: fresh profile (old poisoned disk entries are
  namespaced out, but fresh cache is the clean A/B), h3 off (default).
  Expected: no `expected expression, got end of script` class; page loads
  further; residual failures = documented upstream API gaps (R-17).
- **Performance (P1-4)**: same scroll/load pages as the a7c59a49e session;
  compare feel (100 px/line + coalescing) and note any crash. Numbers
  (load time, scroll FPS) welcome for PHASE4_REPORT before/after figures —
  CI-side measurements are not comparable to real GPUs.
- **If anything fails**: `brow.log` + minidump from
  `%LOCALAPPDATA%\brow\crashes\` plus the failing site; revert-first policy
  per PROCESS.md.

## 7. Commit ledger (Phase 4)

| Commit | Item |
|---|---|
| `0f528dc02` | plan: 4 root causes locked; fix queue 4.1–4.7 |
| `0324d9f6f` | 4.1 Windows crash capture (minidumps + brow.log) |
| `748d9fa83` | 4.2 CI profile → production-stripped (D-015) |
| `c7f2ae367` | 4.3 IME relay fixes + session state machine (8 tests) |
| `4c73c2f9b` | 4.4 truncation/cache hygiene + h3 default-off (D-016/D-017) |
| `b32f3e01c` | 4.5 100 px/line + per-burst wheel coalescing |
| `1031fc566` | 4.6 upstream pack + honest YouTube status |
| `55d42dfe6` | 4.7 gate fixes: lock regen, edition fix, net test repairs, TLS key restore, R-18 |
