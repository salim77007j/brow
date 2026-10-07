# brow v0.6.1 — Reassessment Report

**Companion to:** `docs/REASSESSMENT_PLAN.md` (root causes and fix design).
**Release:** v0.6.1 (tag → commit `ae1593e08`, release run `37618286086`, all 8 assets published).
**Method:** every claim below was verified at runtime, on the actual released
binaries, on a Linux Xvfb harness (software GL) with the v0.6.0 release as the
simultaneous baseline. Evidence artifacts: screenshots (`*.png`) and engine
logs (`*.log`) referenced throughout; harness scripts reproduce every run.

---

## 1. What the audit found (summary)

The v0.6.0 release shipped **stock servoshell renamed to `brow.exe`** — none
of the brow UI, privacy plumbing, allocator or tab-lifecycle work was in the
payload. Independently, five privacy defects made the engine passive even
where it was present. Full evidence trail in the plan document. All of it is
fixed in v0.6.1.

## 2. Privacy verification (P0 — was "not blocking anything")

Harness: local two-origin test (`http://localhost:8765` top page,
`http://127.0.0.1:8766` third-party), known tracker URLs, and real-site runs.
"Installed mode" = launched from a working directory **different** from the
binary's directory — the exact condition under which v0.6.0 silently died.

### 2.1 Tracker blocking (network filter)

| | v0.6.0 | v0.6.1 |
|---|---|---|
| Filter list in installed mode | `brow privacy: no filter list found at ["resources/easylist.txt"]; network filtering inactive` | `brow privacy: filter engine loaded 54934 network rules` (embedded snapshot fallback) |
| `pagead2.googlesyndication.com/pagead/js/adsbygoogle.js` | **LOADED** | **blocked** via rule `||googlesyndication.com/pagead/` |
| `www.google-analytics.com/analytics.js` + `__utm.gif` | **LOADED** | **blocked** |
| Dev-mode (CWD = exe dir), same binary | blocked (proves the v0.6.0 dev/installed split) | blocked |

Evidence: `logs/v060-tracker-installed.log` vs `logs/v061-final-tracker.log`
(the v0.6.1 blocked-subresource log line appears twice — the page retries —
and the load-event quirk in §6.3 keeps the page from "completing"; the
blocking decision itself is the engine's network gate).

### 2.2 Anti-fingerprinting (was 100% dead code)

In-engine defense payload (`canvas2d-noise, webgl-spoof+noise,
audiocontext-noise, navigator-reduction`), evaluated on **every document**
without any shell cooperation:

| Probe | v0.6.0 | v0.6.1 |
|---|---|---|
| Canvas double-read hash | `HASH1 == HASH2` → "SAME (no noise = undefended)" (637111216 twice) | `HASH1: 244288453 / HASH2: -1428645920` → "DIFFERENT (noise = defended)" |
| `navigator.hardwareConcurrency` | 2 (real value) | 4 (per-session reduced value) |
| Activation log | none — payload never written, never injected | `fingerprint defenses active (level Standard, …)` on every page, in both shells |

Evidence: `shots/v060-fp.png` vs `shots/v061-fp-classic.png` /
`shots/v061-final-fp.png`.

### 2.3 Third-party cookies (was hardcoded off) + CHIPS

| Probe | v0.6.0 | v0.6.1 |
|---|---|---|
| Third-party `Set-Cookie: track=1` (receive) then credentialed echo (send) | `COOKIE: SENT (track=1)` | `COOKIE: NOT-SENT (third-party cookies blocked)` |
| Real site (wikipedia.org, third-party context in the harness) | — | `brow privacy: dropped unpartitioned third-party cookie for www.wikipedia.org` |
| CHIPS | only `Partitioned`-without-`Secure` check | full send/receive policy; partitioned third-party cookies keyed to top site (`ServoCookie.partition_key`) |

Evidence: `shots/v060-cookie3.png` vs `shots/v061-final-cookie.png`, and
`logs/v061-ar2.log` for the live wikipedia.org drop.

### 2.4 Coverage holes closed

- **WebSockets**: `start_websocket` now runs the same network-filter gate
  before any TCP connect (v0.6.0: total bypass).
- **Redirects**: `http_redirect_fetch` re-runs the filter on every redirect
  target (v0.6.0: checked once per fetch; tracker 30x evasion).
- **CNAME cloaking**: chase now (a) only runs when a rules-capable engine
  exists, (b) caches **negative** verdicts — one DoH lookup per host per
  session instead of one per connection.
- **Settings ↔ engine**: brow-shell's privacy switches now drive live engine
  prefs (`network_privacy_filter_enabled`,
  `network_privacy_block_third_party_cookies`,
  `network_privacy_fingerprint_level`, `network_privacy_cname_detection_enabled`).

### 2.5 What is still not blocking (honesty list)

- **Cosmetic / element-hiding** filtering (the `brow-privacy` cosmetic
  module) remains unwired — blocked tracker *requests* leave empty boxes on
  some pages. Designed for v0.6.2 (net-thread → document CSS channel).
- Fingerprint defense timing: the payload evaluates with the document's
  userscript batch (head-parse time), so an inline `<head>` script running
  before that point can touch an undefended API. Same guarantee level the
  userscript mechanism always had; strict mode is the future hook.
- The tracker test page exposes a load-event quirk: when a parser-referenced
  script is blocked, servo's `load` event does not fire (§6.3).

## 3. Performance (P0)

VM caveat first: this harness runs software GL (llvmpipe) with a broken-DoH
network (sandbox MITM → system-resolver fallback); absolute wall times are
network/VM-dominated and the same for both binaries. The relevant findings:

| Page (identical conditions) | v0.6.0 | v0.6.1 | note |
|---|---|---|---|
| example.com | 10.7 s / 264.5 MB peak | 10.9 s / 314.2 MB | no regression |
| wikipedia.org | 1.5 s / 304.9 MB | 1.3 s / 351.3 MB | **no SIGSEGV, completes** |
| github.com | >100 s (no shot) | >100 s (no shot) | heavy JS + software GL; sandbox-bound |
| local fp-test | 0.30 s / 217.6 MB | 0.70 s / 275.6 MB | includes font install on first run |

What actually changes user-perceived speed in v0.6.1:

1. **brow-shell ships with the SpiderMonkey JIT enabled.** The old brow-shell
   config built `servo` without `js_jit` — interpreter-only JS. The released
   product previously could not have used this path at all; now the product
   binary runs the JIT (feature parity with servoshell, verified by the
   shared dependency graph in the release build).
2. **CNAME chase cost** removed from the hot path (engine-gated + negative
   cache) — v0.6.0 paid a DoH round-trip on the first https connection to
   every host even with no filter list loaded, and re-paid it on repeats.
3. **`layout_threads` 3 → 6** for desktop-class parallelism.
4. **PGO remains NOT applied** (honest carry-over): a PGO training build
   doubles release CI time and risks the 330-minute Windows timeout. It is
   the first item of the v0.6.2 performance board, together with the O2/O3
   codegen investigation (§6.1) — shipping a speed profile that segfaults on
   a top-10 site was not an option (see §6.1: opt-3 AND opt-2 builds were
   built, tested and rejected this phase; the release keeps the v0.6.0-proven
   `opt-level="s"` codegen).

## 4. Memory (P0)

| Item | v0.6.0 | v0.6.1 |
|---|---|---|
| RSS governor on Windows | **inert** (Linux-only `/proc` sampler) | `GetProcessMemoryInfo` (K32GetProcessMemoryInfo) implementation — compiles, ships, exercised on Windows CI |
| Tab sleep/discard + RSS budgets | not shipped (brow-shell didn't ship) | ships in the product binary (settings: per-tab 100 MB, total 2048 MB, auto-sleep) |
| Allocator | servoshell = jemalloc; mimalloc existed only in the unshipped shell and is hard-gated **off** on Windows | jemalloc default (proven on all three runtime environments); **mimalloc demoted to opt-in** after it aborted the engine at startup in its first-ever runtime test (`memory allocation of 512 bytes failed` right after reserving 1 GiB, mimalloc v3.3.2) — re-evaluate with a real-machine A/B in v0.6.2 |
| Peak RSS in the harness | 218–343 MB across pages | 228–392 MB across pages (software GL + first-run font install inflate v0.6.1 numbers; no regression signal, no claim of a win without real-machine data) |

## 5. UI / Arabic / RTL (P1)

Shipped in v0.6.1 (`brow` = brow-shell; `brow-classic` = the old stock shell
kept as an engineering fallback):

- Product UI is the Slint chrome: tabs, address bar, bookmarks/history/
  downloads/settings panels, memory dashboard — dark theme, hover states.
- **Bundled typography**: Noto Sans + Noto Sans Arabic (OFL, in
  `ports/brow-shell/assets/fonts/`), installed into the platform user-font
  directory at startup before Slint initializes
  (`brow: installed font …/NotoSansArabic-Bold.ttf` in the release log);
  `default-font-family: "Noto Sans"` with per-glyph Arabic fallback.
- **i18n**: EN + AR dictionaries (compile-time parity test), new privacy
  strings localized in both languages; locale switchable in settings without
  restart.
- **RTL semantics**: back/forward glyphs swap with locale (`rtl ? "▶" : "◀"`),
  tab strip reversal retained, panel label alignment mirrors.
- **Runtime evidence**: the released `brow` browses wikipedia.org with full
  page rendering (`shots/v061-ar2-ui.png`, `shots/v061-ar3-ui.png`) and the
  chrome rendered correctly when it was the only mapped window
  (observed live in the first headed run of the release artifact).

**Honest limitation:** the *Arabic-chrome screenshot* could not be captured
in this harness. Without a window manager, X11 window stacking/expose
artifacts garble any second window that is moved or repositioned (the
content window and chrome window overlap at (0,0); `xdotool` moves leave
stale framebuffer regions). The chrome rendered correctly when it was the
only window; the Arabic strings/RTL mirroring are covered by the i18n parity
tests + code, but the final visual check on Windows is part of §7.

## 6. New issues found by actually running the product (the audit's payoff)

This phase was the first time brow-shell ever executed. Five defects were
found and fixed by runtime testing after CI green:

| # | Defect | Fix |
|---|---|---|
| 6.1 | **opt-level 2/3 codegen SIGSEGV** (Script thread) on wikipedia.org, reproducible, absent at `"s"`, independent of all brow privacy features (prefs-off matrix) and layout threading | Ship `"s"` (v0.6.0-proven codegen). O2/O3 + minimized repro → v0.6.2 board. **This is the one fix-1.1 regression: the speed profile stays size-optimized for now** — correctness first, documented decision in `servo/Cargo.toml`. |
| 6.2 | mimalloc v3.3.2 aborts at engine init (first runtime test) | jemalloc default; mimalloc opt-in |
| 6.3 | `Servo::setup_logging` `.expect()` aborted embedders that own a logger | best-effort install + level raise |
| 6.4 | Startup never applied tab lifecycle events → **content window blank forever** (phase 6 fixed compilation only) | `resumed()` synthesizes `Created`+`Activated` for the active tab |
| 6.5 | `pump()` held a `RefCell` borrow across `spin_event_loop` → "RefCell already borrowed" panic on the first navigation | engine handle cloned out before spinning |

Also found (not blocking release): a page whose parser-referenced script is
network-blocked never fires `load` (affects the headless-screenshot flow and
the tracker test page's completion, not correctness of blocking).

## 7. Windows verification plan (AMD iGPU machine — 15 minutes)

The sandbox cannot run a Windows GUI session; the tester performs:

1. **Install** `brow-0.6.1-x64.msi` (or unzip the portable). Launch from the
   Start Menu — **this exact launch mode was broken in v0.6.0** (CWD-relative
   filter resolution); it must now show the dark Slint chrome, not the egui
   strip. `brow-classic.exe` is the old shell for comparison.
2. **Privacy (real sites):** open a tracker-heavy news site; then
   `https://coveryourtracks.eff.org` — expect "unique fingerprint" pressure
   *reduced* (canvas/WebGL/audio noise + navigator reduction are active;
   randomization-per-session means re-testing shows different hashes).
   Logs: `%LOCALAPPDATA%\brow\engine\…` + `RUST_LOG=info` console run shows
   `brow privacy: blocked … via rule: …` lines for tracker requests.
3. **Cookies:** visit any site that sets third-party cookies; with
   `RUST_LOG=info`, expect `dropped unpartitioned third-party cookie …`
   lines. Settings → "Block third-party cookies" toggles it live.
4. **Arabic/RTL:** Settings → Language → العربية. Expect mirrored nav glyphs
   (back points right), Arabic labels, no tofu boxes (bundled Noto Sans
   Arabic). Set as default locale and restart — must persist.
5. **Performance:** same tabs as the v0.6.0 session; compare page-load feel
   (JIT now active) and GPU behavior (ANGLE/D3D path — same as v0.6.0's
   working rendering). If anything renders black, run `brow-classic.exe` and
   report — that would isolate brow-shell's Windows window path.
6. **Memory:** Task Manager with 10+ tabs; background tabs sleep after 10 min
   (settings) and their WebView is discarded (~2 KiB payload each). The RSS
   governor now samples on Windows.
7. **Report back:** the §6.1 codegen question (O2/O3) and PGO remain the
   v0.6.2 speed work; any wikipedia-class crash on Windows at `"s"` would be
   new information.

## 8. Release engineering changes shipped in v0.6.1

- `release.yml` builds **both** shells: `brow-shell` → `brow`/`brow.exe`
  (product), `servoshell` → `brow-classic`/`brow-classic.exe`; mach's
  manifest-path injection prevents a second `-p`, so brow-shell builds via a
  direct cargo invocation into the shared target dir (dep graph reused).
- Fonts + brow-classic added to nfpm (.deb/.rpm), AppImage, WiX MSI and the
  portable zip; payload verified by the deb cross-check patterns.
- Version 0.6.1 across the workspace (68 internal `=0.6.0` pins bumped).
- CI gates all green on `ae1593e08`: brow-privacy (69), brow-shell-core (57),
  brow-net-core, clippy/fmt, brow-shell-check (full engine features), full
  engine build + headless smoke.

## 9. Remaining-issues board (v0.6.2)

1. O2/O3 codegen SIGSEGV (wikipedia repro + minimizer) — then re-land the
   speed profile and PGO (two-stage release pipeline, Windows timeout review).
2. Cosmetic (element-hiding) filtering: net-thread → document CSS channel.
3. mimalloc Windows A/B (allocator feature is compile-time; needs a
   real-machine RSS comparison before any default change).
4. Arabic chrome visual verification on a real desktop (§5 limitation) +
   full layout mirroring (Slint lacks first-class layout direction).
5. Blocked-subresource load-event quirk (upstream-style investigation).
6. Fingerprint defense timing (document-start injection ahead of inline
   head scripts).
