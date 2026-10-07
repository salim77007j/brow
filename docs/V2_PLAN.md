# brow v0.7 — V2 Plan (Option 2: finish brow on Servo)

Status: **Phase 1 deliverable** (research complete; no product code changed in Phase 1).
Owner decision: D-001. Rollback anchor: `v0.6.1-safe` (`7cc97381a`). Working branch:
`v0.7-rebuild`. Risks: `docs/RISKS.md`. Experiments: `docs/EXPERIMENTS.md`.

---

## 0. Executive summary

v0.6.1 failed in owner real-world testing because of three structural problems, in
order of user-visible impact:

- **A. Two-window shell architecture** — the chrome is a separate 1240×88 OS window
  and every tab is its own OS window. No positioning, no z-order management, no
  focus discipline: the content window covers the toolbar, focus steals happen on
  every load, and the taskbar shows one icon per tab. This is unfixable by polish;
  it must be rebuilt.
- **B. CPU-drawn chrome + hand-rolled input stack** — Slint software renderer
  blitted per-pixel via softbuffer; ~200 lines of hand-written keymap; no IME; no
  CJK font bundle; no DPI handling in content windows.
- **C. Engine speed profile** — shipped at opt-level="s" because O2/O3 both
  segfault (Script thread, wikipedia repro); no PGO; single-tab tree RSS 270–615 MB
  on real sites (this study).

The strategy (D-001): rebuild the shell on servoshell's proven **single-window**
pattern (in-tree, fully read this phase — §3), adopt its real input/IME/DPI stack,
and fix the engine speed profile by bisecting the O2/O3 segfault (experiment
**running now** — §4) and then opt-3 + PGO.

The Phase 1 site study materially improves the outlook: on 6 of 10 real sites the
*pages* render usable-to-excellent layouts in v0.6.1 already. The product's
usability failure is dominated by the shell (A+B), which Phase 2 removes.

---

## 1. Evidence base (all in-repo)

| Source | Location |
|---|---|
| 10-site profiling of released v0.6.1 (screenshots, RSS, verdicts) | `docs/evidence/phase1-site-profiling/` |
| v0.6.1 audit (two-window proof, focus steals, no IME) | `docs/evidence/v0.6.1-audit/` |
| Codegen crash history + shipped profile rationale | `servo/Cargo.toml` comment above `[profile.production]` |
| CI baselines (first gated run `37652829943`) | size 147 MiB stripped release-profile `brow-shell`; Linux engine build green; smoke green |
| O2/O3 bisect experiment | branch `experiment/o2o3-codegen`, workflow `experiment-codegen-matrix.yml`, results → `docs/EXPERIMENTS.md` |
| Competitor codegen practice | Firefox: Clang+LTO on all tier-1 platforms since 2018 + PGO (ThinLTO-class, not fat LTO); Chrome: ThinLTO+PGO. Fat LTO (our production profile) is the unusual choice. |

---

## 2. Root causes → fixes map

### A. Two-window shell (brow-shell, 2.6k lines)
**Proof:** chrome = separate winit window rendered with Slint `MinimalSoftwareWindow`
→ CPU buffer → softbuffer blit; each tab = own winit window + surfman context
(`ports/brow-shell/src/app.rs` `ensure_webview`); zero `set_outer_position` calls; no
`Ime` event arm; no `ScaleFactorChanged` handling for content windows. Screenshots in
`docs/evidence/phase1-site-profiling/` show **no toolbar visible at all** on every
site.

**Fix (Phase 2):** rebuild the product shell on the servoshell single-window pattern
(§3), porting brow's identity (privacy UI state, homepage) onto it. Delete the
multi-window code path. Keep `brow-classic` (servoshell) as fallback shell in
releases (R-04).

### B. CPU chrome + hand-rolled input
**Fix (Phase 2 + 3):** the servoshell pattern replaces all of it:
- Chrome = egui `EguiGlow` rendered directly on the window GL context (GPU).
- Content = Servo renders to `OffscreenRenderingContext`, composited into the egui
  scene as a background-layer `PaintCallback` (`render_to_parent`) — no CPU blit.
- Input = winit events routed by `is_in_egui_toolbar_rect()` (toolbar vs content),
  `keyboard_types::ShortcutMatcher` for bindings, `notify_input_event` per event
  kind (§3.2).
- IME = `EmbedderControl::InputMethod` → `set_ime_allowed(true)` +
  `set_ime_cursor_area(...)`; `WindowEvent::Ime` (Enabled/Preedit/Commit/Disabled)
  → `CompositionEvent` stream to the engine (§3.3). This is a *real* IME stack —
  replaces the 200-line hand keymap entirely. Slint's IME limitations (R-05)
  become moot: we drop Slint for the product chrome.
- DPI = `ScaleFactorChanged` → egui zoom factor + `hidpi_scale_factor_changed()`
  propagation (§3.4).
- Fonts: bundle Noto Sans CJK subset + Arabic-capable face (Phase 3; size budget
  inside D-004 gate; R-11).

### C. Engine speed profile
**Fix (Phase 4, informed by the running bisect):**
1. Land the best provably-stable fast profile from the bisect matrix. Prior
   expectation: O3 + thin LTO passes (upstream-servo-like, Firefox/Chrome-like)
   and is 1.5–2.5× faster than "s" on page loads; fat LTO×O3 is the crash suspect.
2. PGO on top (two-stage CI build with `-Cprofile-generate`/`-Cprofile-use`, see
   `docs/BUILD_PGO.md`), gated on the same wikipedia + site-rotation smoke suite.
3. Memory: allocation audit (image cache limits, WebRender tile memory, per-frame
   buffers) against the 270–615 MB measured baseline; target <200 MB/tab after
   GPU-path work (R-06 requires re-validation on real hardware).

---

## 3. The servoshell single-window blueprint (fully read in Phase 1)

This is the pattern Phase 2 adopts. Files: `servo/ports/servoshell/desktop/`.

### 3.1 Window & rendering
- ONE `winit::window::Window` (`HeadedWindow::new`), created hidden, shown after
  accesskit setup; app icon set for Linux/Windows taskbar identity.
- `WindowRenderingContext` (onscreen; target of egui + final present) +
  `OffscreenRenderingContext` (Servo/WebRender target) — `headed_window.rs:76-93`.
- Compositing: `gui.update()` + `gui.paint()` on `RedrawRequested`/`Resized`;
  egui paints the toolbar; the webview framebuffer is drawn via an egui
  background-layer `PaintCallback` (`gui.rs:634-646`); `parent_context().present()`
  (`gui.rs:669-678`). Event-driven only (`ControlFlow::Wait`) — no busy loop.
- Resize: `Resized` → resize contexts → repaint; `request_resize` clamps to
  `MIN_WINDOW_INNER_SIZE` and handles synchronous resize results.

### 3.2 Input routing
- Mouse: `CursorMoved` tracked; events over the toolbar go to egui
  (`is_in_egui_toolbar_rect`), others to the active webview with
  `point.y -= toolbar_height * hidpi_scale_factor` offset; mouse-left-viewport
  synthesized on rect exit.
- Keyboard: non-overridable bindings first (`handle_intercepted_key_bindings`:
  Ctrl+W/X/C/V, Ctrl+1..9, Ctrl+T/Q, PageUp/Down tabs, Alt+arrows history),
  then `notify_input_event(Keyboard)`; page-overridable bindings (zoom, reload)
  applied only when the page did NOT consume the event
  (`notify_input_event_handled`, `pending_keyboard_events` map).
- Multi-tab = multiple `WebView`s in one window (`window.webviews()`,
  `activate_webview_by_index`) — never multiple OS windows.

### 3.3 IME (replaces the hand keymap)
- Engine-driven: `show_embedder_control(EmbedderControl::InputMethod)` →
  `set_ime_allowed(true)` + `set_ime_cursor_area(...)`; hide → `set_ime_allowed(false)`.
- `WindowEvent::Ime`: Enabled → `CompositionState::Start`; Preedit → `Update`;
  Commit → `End`; Disabled → `Dismissed` (only if we expected IME open — avoids
  blurring the newly focused element). `headed_window.rs:714-753`.

### 3.4 DPI
- `ScaleFactorChanged` intercepted: egui zoom = desired/winit scale; then
  `window.hidpi_scale_factor_changed()` reflows content; redraw requested.
  `device_pixel_ratio_override` supported for testing.

### 3.5 What brow adds on top (Phase 2 scope)
- Privacy state in the toolbar (blocked counter from brow-privacy engine), brow
  homepage, branding/icon — as egui widgets, ported from brow-shell's logic.
- Tab strip with real tab management (servoshell has Ctrl+1..9 switching; needs a
  visible tab bar widget) — egui widget work, straightforward.
- Keep brow's `settings.json` profile dir + homepage plumbing.

---

## 4. O2/O3 SIGSEGV bisect — design and state (R-01)

**Hypothesis space.** The crashing configs changed opt-level ("s"→2/3) while
keeping `lto=true` (fat) + `codegen-units=1`. The default release profile (O3,
no LTO, 16 CGU) builds green in gated CI and its smoke passes. Firefox/Chrome
ship ThinLTO-class LTO + PGO; fat LTO is unusual. Prime suspect: **fat-LTO ×
high-opt-level codegen** (possibly localized to mozjs/SpiderMonkey bindings).

**Experiment E-001** (branch `experiment/o2o3-codegen`, workflow
`experiment-codegen-matrix.yml`): builds brow-shell under `[profile.production]`
with single-knob env overrides, then loads the wikipedia repro under Xvfb +
software GL for 150 s. Variants:

| Variant | Config | Expected | Question answered |
|---|---|---|---|
| s-control | "s"+fat (shipped) | PASS | harness sanity |
| o3-fat-repro | 3+fat | CRASH | does the crash reproduce in CI at all |
| o3-nolto | 3, LTO off, 16 CGU | PASS? | is fat LTO the trigger |
| o3-thin | 3, thin LTO | PASS? | upstream-like landing point |
| o3-fat-mozjsO1 | 3+fat, mozjs crates @ O1 | PASS? | is mozjs the localization |

**Anchor rule (D-007):** all verdicts are trusted only when s-control PASSES and
o3-fat-repro CRASHES. If the repro does not reproduce under software GL, the
experiment moves to hardware (owner machine) with the same matrix before any
conclusion. Results → `docs/EXPERIMENTS.md`; fix lands on `v0.7-rebuild` only
after that.

**Fallback ladder** (if no variant is both fast and stable): O2+thin → O1+PGO →
ship best stable + honest report (D-001 accepted risk).

---

## 5. Target metrics and how we measure

| Metric | v0.6.1 baseline | v0.7.0 target | Method |
|---|---|---|---|
| Site usability (20-site rotation) | 6/10 usable here | ≥90% verdict "usable" | Phase 5 protocol, owner hardware |
| Page-load time (owner hardware) | "slower than every browser" | ≤2× Chrome | owner-run timing protocol (Phase 5) |
| Single-tab tree RSS | 270–615 MB (software GL) | <200 MB steady-state | same harness + real-GPU re-measure |
| Crash-free 20-site run | yes (opt-"s") | yes (opt-3+PGO config) | Phase 5 run + CI smoke suite |
| Binary size | 104.7 MiB prod / 147 MiB release | ≤160 MiB gate | CI gate (D-006) |
| CJK/Arabic input | broken (no IME) | IME commit works in address bar + form fields | Phase 3 owner validation |

---

## 6. Phase order (each phase: build → verify → evidence → owner "continue")

1. **Phase 1 (this document):** research + measurement + experiment kickoff. Done
   except: E-001 results landing in `EXPERIMENTS.md` (builds run in CI ~2–3 h).
2. **Phase 2:** shell rebuild on the §3 blueprint, feature branch, Linux CI green
   + smoke, then owner Windows validation. Deliverable: single-window brow with
   visible GPU-drawn toolbar, tabs, no focus steals.
3. **Phase 3:** IME/CJK/RTL/DPI — font bundles, egui bidi, input-method testing
   with Arabic/Chinese/Japanese/Hebrew.
4. **Phase 4:** engine profile fix (bisect outcome → opt-3/thin + PGO), memory
   work, icon-font/glyph fix (re-scoped R-09), UA policy option (old.reddit).
5. **Phase 5:** 20-site acceptance run with per-site verdicts + evidence.
6. **Phase 6:** release v0.7.0 (tag, packages, release notes with honest gaps).

## 7. Open questions (tracked, do not block Phase 2)

- E-001 outcome (running) — decides the Phase 4 landing profile.
- Whether the bbc.com dead grey region is an iframe/compositing gap (low priority;
  record in Phase 5 if it persists).
- egui tab-bar design for parity with browser tab UX (drag-reorder etc. is
  explicitly out of v0.7.0 scope; basic strip only).
