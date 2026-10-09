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

## D-006 — Size gate tightened 200 → 160 MiB on measured baseline

**Date:** 2026-10-08 · **Amends:** D-004 amendment

**Context.** The D-004 amendment set a deliberately loose 200 MiB blind value and
promised to tighten it once the real release-profile baseline was measured. The
first gated run (`37652829943`, job `build-servo-linux`) measured **147 MiB**
stripped, default-release-profile `brow-shell`.

**Decision.** `SIZE_GATE_MIB = 160` in `ci.yml` (source of truth unchanged).
Headroom: +13 MiB over measured baseline, covering the expected production-profile
move (opt-3/thin is smaller than default release; PGO historically shrinks or is
neutral) plus CJK font bundling in the payload (Phase 3). Production-stripped
ships at ~105 MiB today; the shipped artifact stays well under the gate.

**Rejected.** Keeping 200 (no longer a real constraint); 155 (too tight for
PGO/LTO variance across toolchain bumps); measuring the gate on
`production-stripped` instead (would rebuild the world twice per push for no
extra safety — the default release is the superset config).

**Risks accepted.** A toolchain bump that inflates codegen ≥9% would false-trip
the gate; that friction is intentional — it forces a DECISIONS entry, not a
silent gate raise.

---

## D-007 — Bisect anchor rule for codegen experiments

**Date:** 2026-10-08

**Context.** E-001 (O2/O3 SIGSEGV bisect) runs on CI under Xvfb + software GL.
The original v0.6.x crash repro was observed on real hardware. If the crash does
not reproduce under software GL, negative results (variant "passes") would be
meaningless.

**Decision.** E-001's matrix contains two anchors: `s-control` (must PASS) and
`o3-fat-repro` (must CRASH). All other variant verdicts are trusted only when
both anchors behave as expected. If `o3-fat-repro` does not crash, the same
matrix is re-run on the owner's hardware before any conclusion or fix.

**Rejected.** Trusting a matrix without anchors (risk: silently "fixing" a crash
that still exists on hardware, shipping the exact v0.6.x failure mode again);
skipping the control (saves ~1 build, loses harness sanity).

**Risks accepted.** Two extra full builds (~4–6 runner-hours) per matrix run on
a public repo (free runners) — cheap insurance against a wrong engine-profile
decision.

---

## D-008 — Guard verdicts come from observable process state, never from wrapper PIDs or unguarded `wait`

**Date:** 2026-10-08

**Context.** Two independent harness defects surfaced the same day. (1) The
product smoke's RSS guardrail read `/proc/$PID/status` where `$PID` was the
background `timeout(1)` process — always ~2 MB — so the 600 MB guardrail could
never fail regardless of the browser's footprint. (2) E-001 round 1 (run
37660240086) aborted inside `wait "$PID"` under `bash -e` in all five matrix
jobs: the wait's non-zero exit (timeout-kill 124 or a crash signal 13x) is
*data*, but `set -e` treats it as failure and killed the step before any
verdict or evidence could be produced. Both gates reported — or would have
reported — numbers disconnected from reality.

**Decision.** In any CI gate or experiment harness: (a) sample process state
from the process that actually owns it (`pgrep -x brow-shell`, not the
wrapper's PID); (b) capture exit codes of meaningful-failure commands via
guarded forms (`cmd || RC=$?`, `set +e` windows) and classify explicitly;
(c) every verdict path must emit its evidence (summary lines, log copies,
artifact uploads) before the step's exit status is decided. Applied in
`c362e09da` (smoke RSS) and experiment-branch `90d74a22f` (E-001 round-2
harness).

**Rejected.** Removing the RSS guardrail until Phase 4 (loses the only
memory signal on every push); keeping round-1 results as "no crash observed"
(verdicts without load evidence violate D-007 and could have written a false
conclusion into the engine-profile decision).

**Risks accepted.** `pgrep -x` depends on procps (standard on runners); the
guarded-capture style is slightly more verbose. The hardened gates' first run
must produce a *real* RSS number for example.com — if it trips 600 MB, that is
signal, not noise, and gets recorded in RISKS/WORKLOG rather than tuned away.


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

---

## D-013 — Font bundle: Noto Sans SC (full) + Noto Sans Hebrew, installed per-user + loaded by-path in egui

**Date:** 2026-10-09

**Context.** Phase 1 evidence: content CJK = tofu everywhere on a system
without CJK fonts, while DejaVu-covered scripts (Arabic) render — the
engine's script-aware fallback tables already reference CJK families
(`components/fonts/platform/mod.rs` pushes "Noto Sans SC"; Windows table
maps CJK→YaHei/Yu Gothic, Hangul→Malgun, Arabic→Uighur). The missing piece
is font *availability* on minimal systems, the CI runner, and the egui
chrome (its default fonts lack CJK/Arabic/Hebrew entirely). D-012(b) defers
the v0.6.x `install_bundled_fonts` installer to this phase.

**Decision.** Bundle four families in the release payload `fonts/` (Latin +
Arabic already shipped): **Noto Sans SC** (v2.004 SubsetOTF from noto-cjk —
full URO 99.9%, kana, fullwidth; 8.3 MB; family string matches the engine
table) and **Noto Sans Hebrew** (v3.001, 26 KB). Delivery is two-channel:
(a) `brow-shell-core::fonts::install_bundled_fonts` copies payload → per-user
font dir (idempotent by size) as step 0 of brow-shell main, before any font
stack initializes — engine/fontconfig path; (b) egui `configure_fonts` loads
the same files by exe-relative path with **system candidates keeping
priority** (bundled fonts are lowest-priority fallbacks in the egui family
list). Fonts are payload, not binary: the 160 MiB binary gate (D-004/D-006)
is unaffected; payload grows ~8.6 MB.

**Rejected.** (1) Subsetting NotoSansCJKsc to ~11-13 MB — worse coverage
trade than the 8.3 MB regional build for no real win. (2) Renaming the
bundled family to "Noto Sans CJK SC" to hit the table without a diff — the
table already contains "Noto Sans SC"; faking identity risks collisions with
a real install. (3) Engine font-loading API work (registering fonts from
bytes) — much larger surface than the installer for the same outcome.
(4) Hangul face — deferred: Windows covers Korean via Malgun Gothic
(fallback table); bundling KR adds MBs for a secondary locale (R-11 keeps
the gap recorded).

---

## D-014 — UI Arabic strings (D-012(d) revisit) stay deferred: egui lacks proven Arabic shaping

**Date:** 2026-10-09

**Context.** D-012(d) deferred Slint L10n (En/Ar) UI strings to Phase 3 with
a re-visit requirement. egui 0.34.3's CHANGELOG shows real IME work
(#4358/#4794/#4896) and an RTL TextEdit fix (#5547), but no evidence of
Arabic joining/shaping for arbitrary labels (epaint 0.34 uses
skrifa+vello_cpu; no shaping feature is announced).

**Decision.** The egui chrome stays English-only for v0.7.0. Arabic UI
strings would render with isolated glyph forms today — a visible quality
regression vs not shipping them. Page-content Arabic is unaffected (engine
shaping verified correct in Phase 1 evidence). Re-open when egui ships
proven complex-script shaping, or when a shell-side shaping pass is
justified against v0.7.0's scope.

**Rejected.** Shipping Arabic strings behind a settings flag — same
rendering quality problem, now user-visible.

## D-015 — CI artifacts build `production-stripped` (LTO), not plain `--release`

**Date:** 2026-10-09

**Context.** Owner-hardware validation of the CI artifact (a7c59a49e, run
37939128861) showed intermittent crashes during fast scrolling. Research
(Phase 4, docs/PHASE4_PLAN.md §1.1) identified the build configuration as
the root cause: ci.yml built plain `--release` while `[profile.release]` is
NOT defined in servo/Cargo.toml — i.e. opt-level 3, no LTO. That is exactly
upstream servo/servo #48109's segfault class (0xC0000005 on Windows,
reproducible on pure upstream, independent of opt-level, prevented by LTO).
It also explains brow's own E-001 mystery: the O2/O3 wikipedia segfault
ladder matched the no-LTO axis, not the optimization level. release.yml
already builds `production-stripped` (LTO, codegen-units=1, opt-level="s")
— the family every v0.6.x release artifact used — so CI artifacts were the
only builds sitting in the crashing configuration.

**Decision.** ci.yml (Linux + Windows jobs) now builds
`--profile production-stripped` for both mach (engine) and cargo
(brow-shell); every `target/release` path becomes
`target/production-stripped`. The 160 MiB size gate (D-006) is unchanged:
LTO+opt-level="s" binaries are historically the smaller ones.

**Consequences.** CI artifacts and release artifacts share one codegen
identity, so owner validation results transfer to tagged releases. E-001 is
CLOSED-EXPLAINED (missing LTO, not opt-level). The O3+LTO performance
experiment is deliberately NOT adopted now — it is a new untested
configuration (recorded as optional E-002 for owner A/B later); v0.7
optimizes for the proven-stable codegen. Raw `--release` builds stay
available locally for experiments but are no longer shipped or gated.

## D-016 — HTTP/3 ships but stays opt-in for v0.7 (`network_http3_enabled=false`)

**Date:** 2026-10-09

**Context.** brow's custom h3 path (support/brow-net-core) defaults ON and
YouTube advertises Alt-Svc: h3. Research (PHASE4_PLAN §1.3) showed the h3
pump amplifies truncation: per-chunk 30 s timeouts, pooled-connection
eviction that can close QUIC connections under in-flight streams, and —
before 4.4 — early FIN treated as clean EOF with no Content-Length check
anywhere. The owner's reproducible YouTube parse errors came through this
path.

**Decision.** For v0.7, `network_http3_enabled` defaults to **false**
(prefs.rs). The h3 path ships and stays testable (loopback harness), and
4.4's truncation fixes make its failure modes loud (body error → failed
resource) instead of silent, but h3 is opt-in until it survives owner A/B
validation (h3 on vs off on real hardware).

**Consequences.** First-load paths use HTTP/1.1→h2 (Alt-Svc upgrade is not
advertised by default), removing a whole failure class from owner
validation. Re-enable by default only after a clean owner A/B.

## D-017 — Content-Length verification lives at the WIRE layer, not at cache serve

**Date:** 2026-10-09

**Context.** 4.4(b) planned CL verification "at body completion and at
cache serve". The stored body is POST-DECOMPRESSION (http_loader wraps the
stream in `Decoder`), while Content-Length is the WIRE (compressed) length —
a serve-time equality check would reject every compressed response. The
sound verification points are: hyper's CL/chunked framing enforcement
(client-side, surfaces as a mid-body error — previously swallowed into
`Data::Done(partial)`, now `Data::Error`), and brow's h3 pump where data
frames are counted pre-decompression (4.4 adds an exact CL cross-check on
clean FIN).

**Decision.** No decompressed-vs-wire length check anywhere. Cache-side
hygiene instead uses: (1) `aborted` flag set on every mid-body failure —
rejected by serve (existing), revalidate candidate selection (existing), and
disk flush (4.4); (2) the cache-format namespace (`brow-cache-v2`), which
makes pre-4.4 disk entries — including already-poisoned ones on owner
hardware — unreachable without a migration (they age out under the size
cap). Owner diagnostic for poisoned installs: fresh profile or first 4.4
run naturally bypasses old entries.
