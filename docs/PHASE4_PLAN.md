# Phase 4 Plan — Owner-Hardware Failure-Class Fixes (scroll crash, IME, YouTube JS, performance)

Status: **PLAN — awaiting owner approval before implementation** (per owner
directive "Report your plan before implementing").
Date: 2026-10-09. Baseline: `v0.7-rebuild` @ `a7c59a49e` (artifact the owner
tested: run 37939128861, `brow-servo-windows-x86_64` 170.2 MB).

---

## 0. Inputs and method

**Owner validation results (Windows 11 + AMD iGPU, artifact a7c59a49e):**
Phase 2 single-window rebuild **validated on real hardware** (R-04 closes);
Arabic/Chinese page text renders correctly (Phase 3 fonts validated);
privacy counter live. Failing: (1) fast-scroll lag + intermittent crashes,
(2) IME composition broken in page fields ("separated words, no preedit"),
(3) YouTube killed by JS parse errors, (4) general slowness.

**Method.** Four parallel deep-reads of the vendored tree (scroll/crash path,
IME event chain, network body pipeline, upstream sweep of servo/servo
issues/PRs + servo.org blog), followed by **five lead spot-checks of the
load-bearing claims — 5/5 confirmed verbatim**. Vendored snapshot ≈ upstream
master late Sep 2026 (servo 0.6.1 window; mozjs 0.26.3, rustc 1.97.1).

**Headline discovery.** Our long-running E-001 mystery ("O2/O3 segfault,
Script thread, wikipedia.org, no repro on CI") is **upstream servo/servo
#48109** — release builds segfault on Windows (0xC0000005), reproducible on
pure upstream, *independent of opt-level, prevented by LTO*. And the CI
artifact the owner tested is built with plain `--release` — and
`[profile.release]` is **not defined** in `servo/Cargo.toml` (verified:
absent) — i.e. O3 codegen, **no LTO**: the exact crashing configuration of
#48109. The owner's intermittent scroll-time crashes are the expected
behavior of that build, not a mystery.

---

## 1. Root causes (locked, with evidence)

### 1.1 Scroll crash (owner P0)

| Layer | Finding | Evidence |
|---|---|---|
| Codegen | CI artifact = default `release` = O3, no LTO = upstream segfault class | `servo/Cargo.toml` has **no** `[profile.release]` (verified absent); upstream #48109 matrix: `release` crashes, `release+lto` does not; brow's own E-001 record (Cargo.toml:436-459) matches |
| Diagnosability | **Windows crash handler is a no-op**; stderr is detached when launched by double-click (`FreeConsole`) — every crash invisible | `ports/servoshell/crash_handler.rs:7-8` `#[cfg(not(macos|linux|android))] pub fn install() {}` (verified); upstream #48110 |
| Rust-panic candidates in the paint/scroll path | `make_current` expects in egui present path (`desktop/gui.rs:750-752`, `window.rs:140-146`); painter-shutdown race `expect("painter_id not found")` (`paint/paint.rs:320-338`); stale-webview `panic!` (`running_app_state.rs:485-488`); pinch-transform inversion expects (`paint/pinch_zoom.rs:52,64`); `pending_frames` underflow (`paint/painter.rs:778-780`) | file:line from deep-read; panic-class fixes are defensive hardening, applied after crash capture exists |

### 1.2 IME composition (owner P0)

Two independent shell bugs; the engine's composition plumbing itself is
mostly present (CompositionEvents are dispatched; `TextInput` inserts the
full commit string in one operation):

1. **IME-allowance fight: egui-winit kills page IME.** egui-winit 0.34
   re-calls `window.set_ime_allowed(ime.is_some())` on **every egui pass**,
   debounced against a private flag that never learns about servoshell's
   direct `set_ime_allowed(true)` from `show_ime` (page field focused).
   Next RedrawRequested (egui runs before the servo pump), URL bar unfocused
   → `set_ime_allowed(false)` → winit gates off all `WM_IME_*` delivery and
   disassociates the IME context → raw `WM_CHAR` per keystroke →
   **"separated words, no preedit"** — precisely the owner symptom.
   Evidence: egui-winit 0.34.3 `lib.rs:1105-1115`; `headed_window.rs:457-472`
   (show_ime bypasses egui's flag); egui pass order `headed_window.rs:543-547`
   before pump `app.rs:212`.
2. **`Ime::Disabled` after every Windows commit → viewport blur.** winit
   sends `Ime::Disabled` at the end of *every* composition (including
   successful commits). We map it to `ImeEvent::Dismissed` whenever an IME
   is visible, and Dismissed **blurs the focused editable**
   (`document_event_handler.rs:1652-1654`); subsequent Preedit/Commit are
   then dropped (`:1664-1667`). Result: typing dies after each commit until
   re-click — "words committed in fragments". Evidence:
   `headed_window.rs:743-756` (verified verbatim; the code's own comment
   admits it cannot distinguish user-dismissal from session end).
3. Contributing gaps (deferred): no real composing range in `TextInput`
   (preedit emulated as insert+select, `text_input.rs:873-892`);
   preedit cursor range discarded (`headed_window.rs:727`); `isComposing`
   hardcoded false (upstream #46936 has a small, well-specified blueprint).

### 1.3 YouTube JS parse errors (owner P1)

The privacy engine is **exonerated**: the fingerprint-defense payload runs
as a delayed userscript in the page realm and never touches response bodies
(`components/script/dom/userscripts.rs:59-93`; no body-rewriting anywhere in
`components/net`). The parse errors are **truncated script sources**:

1. **Silent truncation on mid-body errors.** In `components/net/
   http_loader.rs:2528-2545` (verified verbatim): only `InvalidData`
   (decompression) produces `Data::Error`; **every other mid-body error**
   marks the partial body `ResponseBody::Done(partial)` and sends
   `Data::Done` → the script element compiles whatever arrived →
   "expected expression, got end of script" at the truncation point.
2. **brow's custom HTTP/3 path amplifies it.** `network_http3_enabled`
   defaults **true** (`prefs.rs:636`); YouTube advertises `Alt-Svc: h3`.
   The h3 pump (`support/brow-net-core/src/h3.rs:219-273`) has per-chunk
   30 s timeouts, pooled-connection eviction that can close QUIC connections
   under in-flight streams, treats early FIN as clean EOF — and **nothing
   anywhere compares received bytes to Content-Length**.
3. **Cache poisoning makes it persist.** The memory cache stores the live
   shared body Arc (`http_cache.rs:1195`); the truncated body is marked
   `Done` and served to every later load; on eviction it is flushed to the
   disk cache (`disk_cache.rs:262-317` accepts anything `is_done()`) and
   restored across browser restarts (`http_cache.rs:1126-1128`). Matches the
   owner's persistent, reproducible errors.
4. `[bugsnag] No valid entry type provided to observe()` is **servo's own
   console warning** from `performanceobserver.rs:161-172` — a
   PerformanceObserver API gap, cosmetic, unrelated to truncation.

Upstream context: #46508 (merged, in snapshot) already normalizes
decompression failures; upstream's own YouTube failure mode is different
(missing `Animation`/`SVGAnimatedString` APIs, #47963) — upstream parses
these multi-MB scripts fine, so our truncation finding is **new information
worth filing upstream**.

### 1.4 Performance (owner P1)

- Build: O3/no-LTO artifact — both the crash class *and* a slower, fatter
  binary than the LTO profiles (upstream releases use `--profile production`).
- Scroll architecture: scrolling is compositor-driven (offsets only, no
  re-layout per tick — the upstream-merged 2026 scroll-perf fixes are all in
  this snapshot), **but every wheel tick round-trips through the script
  thread** (layout hit-test + DOM dispatch) before the compositor scrolls;
  there is no wheel coalescing window, no momentum/fling, 76 px/line
  (`window.rs:24,26`), no vsync on Windows (fixed 120 Hz timer,
  `paint/refresh_driver.rs:299-304`), and a double render per presented
  frame (WR → offscreen FBO → egui blit; architectural consequence of the
  Phase 2 single-window chrome, not removable now).

---

## 2. Execution queue (ordered by impact × feasibility; one commit set each)

| # | Sub-item | Fixes | Est. |
|---|---|---|---|
| **4.1** | **Windows crash capture (P0)** — implement `crash_handler::install()` for Windows: `SetUnhandledExceptionFilter` + `MiniDumpWriteDump` (dbghelp) to `%LOCALAPPDATA%\brow\crashes\`, panic hook → rolling `brow.log` next to minidumps, startup log line with GL renderer string + build profile; log dir surfaced in the shell's about/version output. Any future crash becomes actionable. | #48110 class | 1 session |
| **4.2** | **Build profile: exit the segfault class (P0)** — switch CI Linux+Windows jobs from `--release` to `--profile production-stripped` (LTO, opt-level "s", codegen-units=1 — the *proven* config per E-001 ladder and upstream #48109; same family as v0.6.1 release artifacts and upstream release.yml). Record D-015. The O3+LTO performance config is *not* adopted now — it is a new experiment (E-002, optional owner A/B later). Size gate re-check: LTO+opt-s binaries are the small ones; 160 MiB gate stays green. | #48109, E-001 close | 0.5 session + ~3 h CI |
| **4.3** | **IME shell relay fixes (P0)** — (a) single IME-allowance authority: when the page requests IME (`visible_input_method` set), re-assert `set_ime_allowed(true)` + cursor rect after every egui pass so egui-winit's debounced disable can never win while the page owns focus; (b) `Ime::Disabled` mapping: track the composition session — Disabled right after Preedit/Commit (winit's Windows session-end signal) is **not** user-dismissal and must not blur; Dismissed only for genuine dismissal (no session) or engine-initiated hide; (c) unit-test the mapping table (Enabled/Preedit/Commit/Disabled × session states); (d) relay the preedit caret rect to `set_ime_cursor_area` (fixes misanchored IME window, `document_embedder_controls.rs:124` FIXME). Owner re-runs the Phase 3 IME matrix on the next artifact. | owner symptom 2 | 1 session |
| **4.4** | **Network truncation + cache hygiene (P1)** — (a) mid-body errors must fail the resource: send `Data::Error(NetworkError::...)` instead of `Data::Done(partial)` in `http_loader.rs` and the h3 pump; partial bodies never reach the parser; (b) Content-Length verification at body completion and at cache serve (memory + disk); mismatch = network error; (c) never store/serve bodies that did not end cleanly; (d) **default `network_http3_enabled=false` for v0.7** (D-016) — the custom h3 path ships but stays opt-in until it survives owner validation; (e) unit tests: a truncating/flaky test server drives script `error` events (not parse errors), CL-mismatch rejection, poisoned-cache rejection. YouTube diagnostic for owner: `--pref network_http3_enabled=false` + fresh cache. | owner symptom 3 | 1.5 sessions |
| **4.5** | **Scroll feel quick wins (P1)** — wheel line height 76 → 100 px (Windows Notepad/Firefox ballpark; upstream #38072 direction); batch pending wheel events per presented frame (verify the existing batching, add a coalescing window so N ticks = 1 scroll+1 frame); keep upstream-merged scroll-perf work untouched. Momentum/fling is *not* attempted this phase. | owner symptom 1 (lag) | 0.25 session |
| **4.6** | **Upstream pack + YouTube honesty (P1)** — `docs/UPSTREAM/`: (i) Windows crash-handler PR description (answers #48110); (ii) issue draft: silent body truncation + cache poisoning with our file:line evidence and a proposed fix (new info upstream); (iii) comment draft for #46936 (isComposing) + #37037 note; (iv) note for #42593/#45668 (stale scroll-node panics) referencing our hardening. Plus honest YouTube status in the report: parse errors fixed ≠ full compat — upstream #47963 documents missing `Animation`/`SVGAnimatedString`; YouTube remains degraded. | owner rule: upstream-first | 0.5 session |
| **4.7** | **Artifact + Phase 4 report (gate)** — rebuild via new profile, fresh `brow-servo-windows-x86_64`, `docs/PHASE4_REPORT.md` with before/after: crash-capture live, profile recorded, IME fixes + retest matrix, network tests green, scroll tune numbers, YouTube honest verdict; owner validation checklist. | deliverable | 0.5 session + CI |

Total implementation estimate: **~5 working sessions + ~6 h CI wall-clock**.

### Deferred (honest, with reasons)

- True inline preedit rendering (real composing range in `TextInput`) —
  upstream #20770-family work, touches script text editing core; upstream-track.
- Off-thread JS compilation — upstream PR #45687 still open; porting a
  moving target; revisit after v0.7.0.
- PGO/BOLT builds — infrastructure exists (`pgo-build.yml`); schedule with
  Phase 5/6 perf measurements, not before the crash class is closed.
- Memory <200 MB (R-06) — allocation audit is its own work item; the
  profile switch alone will not get there; keep target negotiated with data.
- h3 hardening beyond default-off (progress watchdog, no-eviction-under-
  flight) — after owner A/B validation of h3 on/off.
- YouTube full API compat (`Animation`, `SVGAnimatedString`, …) — upstream
  scale; document honestly.

---

## 3. Verification ladder

1. **Per commit:** fast gates (crate tests, clippy+fmt, servoshell compile
   gate), YAML parse for workflow edits.
2. **New unit tests:** IME mapping table; network error propagation
   (truncating server → `error` event, not parse error); Content-Length
   verification; cache hygiene (no poisoned entries stored/served).
3. **CI:** 7/7 green with `production-stripped` artifacts; Windows zip
   uploads; size gate.
4. **Owner hardware (closes each issue):**
   - Crash: any crash now leaves a minidump + log in `%LOCALAPPDATA%\brow\crashes\`.
   - IME: Phase 3 owner matrix §1 (zh pinyin / ja / ar / he in URL bar + page form).
   - YouTube: h3 off + fresh cache → no parse errors (degraded-but-functional verdict acceptable; documented).
   - Scroll: fast-scroll A/B vs a7c59a49e artifact; no crash in 10 min aggressive scroll.
5. **Rollback:** every sub-item is independent; revert-first policy per
   PROCESS.md; artifact provenance recorded per run ID.

## 4. Register updates shipped with this plan

- R-04 → **CLOSED** (owner validated single-window on Windows/AMD).
- R-01/E-001 → **CLOSED-EXPLAINED** (upstream #48109; LTO is the determinant;
  CI moves to the proven profile; D-007 anchor resolved).
- R-05 → root cause locked (two shell bugs); fix = 4.3; owner matrix remains
  the closer.
- **New R-15** (panic candidates in paint path — harden after crash capture),
  **R-16** (silent body truncation + cache poisoning — fix 4.4), **R-17**
  (YouTube API-compat gap — documented limitation, upstream #47963).
- D-015 (CI profile = production-stripped), D-016 (h3 opt-in for v0.7).
