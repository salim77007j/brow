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

**Amendment (2026-10-08, same day).** The CI size gate measures the stripped
**default-`release`** profile (what the every-push CI job builds), not
`production-stripped` (the shipped profile). The release-profile baseline was
unknown at gate-writing time, so the initial value is set to **200 MiB**
(deliberately loose, blind calibration) and the gate step prints the actual
size into the job summary on every run. Phase 1 records the real baseline and
tightens the gate; the amendment avoids blocking all work on a guessed number
while still catching gross bloat. Gate source of truth: `SIZE_GATE_MIB` in
`.github/workflows/ci.yml`.

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

## D-006 — Binary size gate tightened 200 → 160 MiB on measured baseline

**Date:** 2026-10-08 (recorded here for register completeness; originally logged
in WORKLOG Phase 1.5, commit 178080a0d).

**Decision.** After the first gated CI run measured the stripped default-release
`brow-shell` at 147 MiB, the every-push size gate was tightened from the blind
200 MiB (D-004 amendment) to **160 MiB**. The gate prints the measured size into
the job summary on every run.

---

## D-007 — E-001 verdicts are trusted only when the two anchors render

**Date:** 2026-10-08 (originally defined in docs/EXPERIMENTS.md E-001 round 2).

**Decision.** The codegen-matrix experiment has two anchor variants: `s-control`
must PASS (alive) and `o3-fat-repro` must CRASH before ANY variant verdict is
trusted. If the repro does not crash under CI software GL, the experiment moves
to owner hardware before any codegen change is proposed. A cancelled/timeout
anchor is neither PASS nor CRASH and yields no conclusion.

---

## D-008 — RSS guardrails must sample the browser process, not a wrapper

**Date:** 2026-10-08 (originally WORKLOG Phase 1.7, commit c362e09da).

**Decision.** Memory guardrails in CI must read `/proc/<brow-pid>/status` of the
actual browser process (evidenced by a `bs_pid=` field in the smoke output), not
the `timeout`/`xvfb-run` wrapper (whose ~2 MB VmRSS made the original guardrail
decorative). Applies to the product smoke and any future memory gates.

---

## D-009 — E-001 round 3: anchors-only re-dispatch, non-blocking for Phase 2

**Date:** 2026-10-08 (Phase 2 start check; WORKLOG Phase 2.0).

**Decision.** Round 2 ended with both anchors CANCELLED in a GHA `mach bootstrap`
flake (no verdicts, D-007 ⇒ no conclusion). Re-dispatch round 3 with anchors
only (`variant_set=anchors` input; bootstrap hardened with a 3×15-min retry) on
`experiment/o2o3-codegen`; run in the background, do NOT gate Phase 2 shell work
on it; re-check at the Phase 4 start check. The 3 PASS-alive variant results
from round 2 remain observations, not verdicts.

---

## D-010 — Phase 2 shell rebuild: brow-shell becomes a thin bin over a libified servoshell (Option B)

**Date:** 2026-10-08

**Context.** Phase 1's deep read (V2_PLAN §3) plus this phase's full integration
map of both shells established: servoshell already ships the single-window
pattern end-to-end (WindowRenderingContext + OffscreenRenderingContext, egui
GPU chrome, a visible egui tab strip at gui.rs:286-367/532-576, real IME at
headed_window.rs:714-753, DPI at 567-588, keyboard ShortcutMatcher, brow's
background-tab throttling already patched into WebViewCollection), and it
already has a lib target whose `desktop` module is `pub(crate)`. brow-shell's
2.2k LOC two-window Slint/softbuffer stack is the root cause of the v0.6.x
failure class and cannot be polished into correctness. Local full builds are
infeasible (sandbox: 2 CPU / 3 GB RAM); all engine verification runs in CI.

**Options considered.**
- **A. Fork servoshell desktop code into brow-shell** (copy-adapt ~4.4k LOC):
  duplicates hard-won logic; every servoshell fix must be re-ported; immediate
  divergence.
- **B. brow-shell = thin bin over libified servoshell (CHOSEN):** make
  `servoshell::desktop` reachable from the lib, parameterize identity (window
  title, app id, icon, homepage/newtab defaults), brow-shell keeps its name and
  CI contract, loads brow-shell-core Settings → ServoShellPreferences + engine
  prefs, registers a status provider for the privacy counter, and calls the
  facade. Smallest diff, free inheritance of future servoshell fixes, egui adds
  ~1–2 MiB against the 160 MiB gate.
- **C. Extend servoshell in place and ship it as the product bin:** pollutes the
  vendored upstream tree with product identity; complicates every upstream
  merge; rewrites CI packaging.

**Decision.** Option B. Concretely: (1) `pub(crate) mod desktop` → `pub mod
desktop` in servoshell lib.rs; (2) identity parameterization with servo
defaults; (3) brow-shell/src/main.rs rebuilt as the thin bin (old app.rs /
platform.rs / chrome.rs / keymap.rs / delegate.rs / state.rs / waker.rs /
ui/browser.slint / build.rs Slint wiring / tests/ui_smoke.rs deleted);
(4) feature union: brow-shell keeps a direct `servo` dep carrying the features
servoshell's defaults lack (clipboard, brotli-compression-stream, webcrypto);
(5) CI contract preserved: bin name `brow-shell`, settings.json schema, the two
engine log markers, size gate.

**Rejected.** Option A (duplication); Option C (upstream pollution).

**Risks accepted.** servoshell's extra deps (webdriver_server, tokio, bpaf,
gilrs) enter the product link graph in Phase 2 — trimming is a Phase 4
size/memory item. brows' bookmark/history/download stores and session restore
are idle in Phase 2 (see D-012).

**Rollback anchor.** `6856c1110` (pre-rebuild tip of `v0.7-rebuild`); the Slint
stack can be restored from git history if the rebuild fails its gates.

---

## D-011 — Privacy blocked-counter: process-global atomics + GUI status provider

**Date:** 2026-10-08

**Context.** No privacy counter UI exists in any shell today. The engine's
`PrivacyStats` (brow-privacy/src/stats.rs) holds five AtomicU64 counters
incremented on the net thread (record_block / record_cname / record_cookie_*),
persisted only at shutdown; no embedder-facing API exists. brow is a
single-process embedder (libservo), so no IPC is needed to read counters.

**Decision.** (1) brow-privacy gains process-global mirror atomics incremented
inside the existing `record_*` methods (zero changes to components/net call
sites) plus a lock-free `global_totals()` snapshot reader. (2) servoshell's GUI
gains a neutral extension point — a settable status-item provider callback
(default None, servo behavior unchanged); brow's bin registers a provider that
formats the blocked total (🛡 N) from brow-privacy globals. servoshell stays
decoupled from brow-privacy.

**Rejected.** EmbedderEvent/IP plumb through constellation→embedder (invasive,
larger diff, not needed in a single-process embedder); polling
privacy_stats.json (only persisted at shutdown — useless).

---

## D-012 — Phase 2 keeps servoshell tab/throttle semantics; brow extras re-layer in Phase 3/4

**Date:** 2026-10-08

**Decision.** Phase 2 ships servoshell's WebViewCollection semantics (activate/
throttle/hide) as-is. Deferred with a re-layer plan: (a) brow TabManager
sleep/discard (memory lever) → Phase 4 (R-06 memory work) layered on
WebViewCollection — background-tab CPU throttling is retained via servoshell;
(b) bundled Noto CJK/Arabic font installer (platform.rs) → Phase 3 font bundle
work (servoshell uses system fonts meanwhile); (c) session.json restore →
re-layered on servoshell after Phase 2 validation (settings key retained);
(d) Slint L10n (En/Ar) UI strings → Phase 3 (the egui chrome ships English-only
in Phase 2; page-content RTL is an engine concern, unaffected). Each deferral
is recorded in RISKS.md and re-visited at its phase start.
