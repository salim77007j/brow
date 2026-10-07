# brow v0.6.1 — Reassessment & Fix Plan

**Status:** ACTIVE — this document drives the v0.6.1 fix phase.
**Trigger:** Real-world testing of v0.6.0 on a Windows machine with an AMD iGPU
found four P0/P1 failures: unacceptable performance, abnormally high memory
use, non-functional privacy protections, and a poor UI with no Arabic support.
**Method:** Full code audit of the shipped binary, the engine integration
points, the build profiles and the packaging pipeline. Every root cause below
is backed by file/line evidence in this repository.

---

## 0. The headline finding

**The binary we shipped as v0.6.0 is not brow.**

`release.yml` runs `./mach build`, which builds the workspace default member
`ports/servoshell` (`servo/Cargo.toml:19`,
`default-members = ["ports/servoshell"]`), then renames the output:

```
release.yml:101   cp servo/target/production-stripped/servoshell packaging/payload/brow
release.yml:276   Copy-Item servo\target\production-stripped\servoshell.exe packaging\payload\brow.exe
```

Everything the project actually built in phases 2–4 — the Slint chrome with
tabs/settings/Arabic i18n (`ports/brow-shell`), the privacy engine UI bridges,
the mimalloc allocator, the tab sleep/discard lifecycle, the memory governor —
**ships nowhere**. The tester received stock servoshell: an egui debug chrome,
no privacy plumbing beyond dead engine defaults, no Arabic support, and the
plain `production-stripped` engine profile.

Three compounding defects mean brow-shell itself could not have shipped either
(all fixed in this phase):

1. **brow-shell built SpiderMonkey without the JIT.** Its `servo` dependency
   used `default-features = false, features = ["background_hang_monitor"]`
   (`ports/brow-shell/Cargo.toml:48`), which drops `js_jit` — the JS engine
   would have run interpreter-only, i.e. an order of magnitude slower.
2. **brow-shell built without embedded resources** (`bundled` feature off), so
   the engine's resource reader would panic at first fetch.
3. **brow-shell passed no `Opts`/`Preferences` to the engine**
   (`app.rs:175-179`, `ServoBuilder::default()`), so no config dir, no privacy
   stats path, and the settings panel toggles wrote JSON files that the engine
   never read.

---

## 1. Issue-by-issue root causes and fixes

### ISSUE 1 (P0) — Performance is unacceptable

| # | Root cause (evidence) | Fix (v0.6.1) |
|---|---|---|
| 1.1 | Shipped engine profile is **size-optimized**: `[profile.production]` uses `opt-level="s"` (`servo/Cargo.toml:436-441`); release CI builds `--profile production-stripped` (`release.yml:93/264`). Size-optimal ≠ speed-optimal for a browser. | Switch `production` to `opt-level = 3` (keep fat LTO, `codegen-units=1`, strip in the `-stripped` variant). |
| 1.2 | **No PGO** in the release pipeline (`release.yml` never uses `pgo-release`; `docs/BUILD_PGO.md` claims otherwise). | Deferred to v0.6.2: a PGO training build doubles CI time and risks the 330-min Windows timeout. Documented, not hidden. |
| 1.3 | **CNAME-cloaking check awaits a full DoH chain resolution inline before the first https connection to every host** (`components/net/fetch/methods.rs:231-232` → `privacy.rs:208-247`), even when no filter list is loaded, and **negative results are never cached** — only cloaked hosts enter `cloaked_hosts`. | (a) Skip the chase entirely when the filter engine is absent; (b) cache negative verdicts so each host is chased at most once per session; (c) the engine is now always present (embedded list, fix 3.1), so the chase is gated by a rules-capable engine + cached. |
| 1.4 | (Latent, brow-shell) JS ran **without JIT** — see §0. | brow-shell now enables the same feature set as servoshell (`js_jit`, `bundled`, `clipboard`, `webgl`). |
| 1.5 | `layout_threads: 3` default (`components/config/prefs.rs:610`) under-parallelizes layout on desktop-class CPUs. | Raise to 6. |

**Targets:** page load time on the reference set (Wikipedia, GitHub, a news
site) within 2× of Firefox on the same VM by v0.7; the v0.6.1 measurement
baseline is recorded in `docs/REASSESSMENT_REPORT.md` (software-rendered VM —
absolute numbers will differ from the tester's iGPU machine; the comparison
is v0.6.0 vs v0.6.1 under identical conditions).

### ISSUE 2 (P0) — Memory consumption is abnormally high

| # | Root cause (evidence) | Fix (v0.6.1) |
|---|---|---|
| 2.1 | The shipped servoshell never had any of the phase-3 memory work (mimalloc, tab sleep/discard, RSS governor) — see §0. | **Ship brow-shell**, which carries all of it. |
| 2.2 | The RSS sampler behind the memory governor is **Linux-only** (`support/brow-shell-core/src/memwatch.rs:16-34` returns `None` off-Linux) — the governor is inert on Windows, the primary shipping platform. | Implement `GetProcessMemoryInfo`-based sampling on Windows (`windows-sys`, `Win32_System_ProcessStatus`). |
| 2.3 | mimalloc is feature-gated off on Windows by upstream's allocator choice (`components/allocator/lib.rs:160-236`, gated `not(windows)`); servoshell uses jemalloc. | Keep jemalloc on Windows for v0.6.1 (proven in the shipped build); re-evaluate mimalloc-on-Windows in v0.6.2 with an A/B RSS measurement. |
| 2.4 | Background tabs tick at 1 FPS and timers clamp, but discarded tabs are the real win — the sleep/discard lifecycle only existed in the unshipped shell. | Ships with brow-shell; settings expose per-tab / total budgets. |

**Targets:** cold start ≤ 450 MB RSS on the Linux VM test harness (v0.6.0
baseline measured in the report); RSS governor holds steady-state growth under
a 20-tab session via sleep/discard (functional test, screenshots in report).

### ISSUE 3 (P0) — Privacy protections are not working

The privacy engine reported "active" while blocking nothing measurable. Five
independent root causes:

| # | Root cause (evidence) | Fix (v0.6.1) |
|---|---|---|
| 3.1 | **The EasyList filter list is resolved relative to the process CWD** (`components/net/privacy.rs:107`, `resources/easylist.txt`), not the executable. Packaged builds launch with a different CWD (Start Menu, `.desktop`, symlinked `/usr/bin/brow`) → `existing.is_empty()` → engine = `None`, with only a `log::info`. Works in dev (`mach run`), dies when installed — a classic packaging split. | Multi-stage resolution: (1) `network_privacy_filter_list_path` pref, (2) exe-relative `resources/easylist.txt` (walk ancestors like servoshell's own resource lookup), (3) CWD (dev), (4) **embedded snapshot** (`include_str!` of `support/brow-privacy/assets/easylist-snapshot.txt`, ~80k rules) — the engine can no longer silently no-op in any install mode. |
| 3.2 | **Anti-fingerprinting is 100% dead by default.** The defense userscript is only *written* when a config dir exists (`privacy.rs:58-82`; brow-shell passed none) and only *executed* when a shell passes `--userscripts` (`ports/servoshell/prefs.rs:353-356` — no shell does). | Embed the payload in the engine: `components/script/dom/userscripts.rs` builds the defense script once per process (level from the `network_privacy_fingerprint_level` pref, per-session key from entropy) and evaluates it before any shell userscripts on every document. No file IO, no shell cooperation required. |
| 3.3 | **Third-party cookies are hardcoded to NOT be blocked** (`privacy.rs:92`, `block_third_party_unpartitioned: false`), and the full CHIPS send/receive policy (`support/brow-privacy/src/chips.rs:123/151`) is never called — only the `Partitioned`-without-`Secure` attribute check runs (`resource_thread.rs:803-817`). | New pref `network_privacy_block_third_party_cookies` (default **true**). Receive side: unpartitioned third-party cookies are dropped; `Partitioned` third-party cookies are stored with a CHIPS partition key (new `ServoCookie.partition_key` field). Send side: `set_request_cookies` filters the jar through `chips::send_policy` (first-party include; partitioned only on key match; unpartitioned third-party omitted). |
| 3.4 | **WebSockets bypass the filter entirely** (`components/net/websocket_loader.rs` has zero privacy references) — tracker beacons over WS are unfiltered. | Privacy gate at the top of `start_websocket` (same `check_request`, destination mapped to `OTHER`/websocket mask), before any TCP connect. |
| 3.5 | **Redirects are not re-checked**: the filter runs once per fetch (`methods.rs:217`); a 30x hop onto a blocked host is not evaluated. | Re-run `check_request` on each redirect target inside the `http_fetch` redirect loop. |
| 3.6 | The settings-panel privacy toggles in brow-shell were **cosmetic** (`app.rs:453-465` writes JSON the engine never reads). | `apply_engine_prefs` now bridges every privacy setting into live engine prefs: `block_ads` → `network_privacy_filter_enabled`, `block_third_party_cookies` → the new pref, `fingerprint_defense` → `network_privacy_fingerprint_level` (`off`/`standard`/`strict`), `block_cname_tracking` → `network_privacy_cname_detection_enabled`, `doh_template` → `network_dns_over_https_templates`, `min_tls_version` → `network_tls_min_version`. |
| 3.7 | Cosmetic (element-hiding) filtering is implemented in `brow-privacy` but wired to nothing. | **Deferred to v0.6.2** with a proper design (cosmetic CSS delivered from the net thread to the document via a constellation message). Network-level blocking already removes the tracker/ads *requests*; hiding residual placeholder boxes is cosmetic polish, tracked in the report's remaining-issues list. |

**Targets (measured in `docs/REASSESSMENT_REPORT.md` on the test harness):**
- Tracker scripts from the EasyList snapshot (doubleclick, googletagmanager,
  google-analytics, facebook) blocked 100% on a controlled local test page.
- `document.cookie` never receives a third-party unpartitioned cookie (engine
  unit tests + live check page).
- Canvas fingerprint hash differs across two loads (per-session noise).
- Zero silent degradations: any filter-engine failure now logs a visible
  warning and the engine falls back to the embedded list.

### ISSUE 4 (P1) — UI/UX is poor, no Arabic support

| # | Root cause (evidence) | Fix (v0.6.1) |
|---|---|---|
| 4.1 | The tester saw servoshell's egui chrome — the Slint chrome (tabs, panels, settings, L10n) ships nowhere (§0). | Ship brow-shell as **the** brow UI (servoshell remains in the payload as `brow-classic` as an engineering fallback). |
| 4.2 | **No Arabic-capable font is bundled**; Arabic rendering depends entirely on whatever system fonts fontdb finds (no `font` reference anywhere in brow-shell). On minimal systems Arabic shows as boxes. | Bundle Noto Sans + Noto Sans Arabic (OFL) in `ports/brow-shell/assets/fonts/`, installed at startup into the platform user-font directory before Slint initializes; `default-font-family: "Noto Sans"` with per-glyph fallback to Noto Sans Arabic. |
| 4.3 | RTL is string-level only: alignments flip but **toolbar order does not mirror and nav glyphs don't swap** (`ui/browser.slint:229-230` — back is always `◀`). | Nav glyph semantics now follow `rtl` (back/forward arrows swap), mirrored paddings in the address bar and panel rows, tab strip reversal kept (`chrome.rs:113-116`). Full layout mirroring (Slint lacks first-class layout direction) is tracked as next-step work. |
| 4.4 | No privacy settings surfaced in the UI beyond two toggles. | Settings panel gains: fingerprint defense level (off/standard/strict), block third-party cookies, block CNAME tracking, DoH template — all wired to engine prefs (fix 3.6), all EN+AR localized via `brow-shell-core::i18n`. |

**Target:** every chrome string renders correctly in `ar` (screenshot
evidence in the report), `rtl` mode mirrors nav semantics, and switching
locales requires no restart.

---

## 2. Architectural changes

1. **One product binary.** `release.yml` builds both shells; `brow-shell`
   becomes `brow`/`brow.exe`, servoshell ships as `brow-classic` for
   comparison/fallback. mach's cargo passthrough (`-p`) keeps one shared
   dependency build (no double LTO cost beyond the second link).
2. **Embedded privacy data.** The filter list is embedded in the engine
   binary (`include_str!`), with file-based overrides for updates. The
   fingerprint payload is generated in-process; the write-to-disk userscript
   path remains for shell-managed user scripts only.
3. **CHIPS in the cookie jar.** `ServoCookie` gains a `partition_key`
   (serde-defaulted for storage compatibility); receive and send paths
   consult `brow-privacy::chips` policy — the phase-4 module finally
   connected end-to-end.
4. **Speed profile.** `[profile.production]` = opt-level 3 + fat LTO +
   codegen-units 1 (+ strip in `production-stripped`).
5. **Engine preferences are builder-time + live.** brow-shell constructs
   `Preferences` from its settings store before `ServoBuilder::build()` and
   re-bridges on toggle via `Servo::set_preference`.

## 3. Timeline (executed in this phase)

| Step | Scope | Status |
|---|---|---|
| A | Audit + this document | done |
| B | Engine privacy fixes (3.1–3.5, prefs) | this push |
| C | brow-shell shipability (features, Opts/prefs bridge) + memory (Windows RSS) | this push |
| D | UI: fonts, RTL semantics, settings panel, L10n | this push |
| E | Profiles + release pipeline + version 0.6.1 | this push |
| F | Self-test: `cargo test` gates in CI; on-machine test of the released Linux build under Xvfb (screenshots, load times, RSS, tracker/canvas/cookie checks); v0.6.0 vs v0.6.1 comparison | report |
| G | `docs/REASSESSMENT_REPORT.md` with measurements + remaining issues | after F |

**Honest limitation:** this environment cannot run a Windows GUI session, so
the AMD-iGPU-specific numbers must come from the tester; the report provides a
step-by-step verification plan for that machine. The Linux test harness here
runs the *actual released binaries* (v0.6.0 tar.gz and v0.6.1 tar.gz) under
Xvfb with software GL, so relative before/after results are meaningful.

## 4. Explicit non-goals for v0.6.1

- PGO/LTO train in release CI (v0.6.2 — needs a two-stage pipeline and a
  Windows timeout review).
- Cosmetic filtering injection (v0.6.2 — needs a net→script CSS channel).
- mimalloc on Windows (v0.6.2 — requires A/B RSS data).
- Multi-process sandbox hardening (tracked in `docs/PHASE2_SANDBOX_ASSESSMENT.md`).
