# brow Phase 10 — Final Verification Report (Phase 5–10)

**Head**: `163c229c2` · **Branch**: `v0.7-rebuild` · **Date**: 2026-10-11
**Verdict**: **ALL GATES GREEN** — engine panics 0/50 sites, JS console
errors 0/50 sites, sanitizer suite clean, privacy rubric ≥ 90/100,
Windows + Linux artifacts building on the final commit.

## 1. The four owner-reported critical errors — fixed and CI-verified

| # | Owner report | Root cause (source-verified) | Fix | Verified by |
|---|---|---|---|---|
| 1 | `assertion failed: self.can_run_script()` — pages all dead | userscript delayed task held only the Window; after redirect/iframe swaps the active document was dead/replaced | queued-document identity + can_run_script guard in the engine task queue | runs #88–#106: 0 panics across 50 real sites |
| 2 | `SecurityError: Location's relevant Document is not same origin-domain` (brow:fingerprint-defense) | same stale-document race on the error path; location read crossed origins after document swap | payload hardening: try/catch location reads, 11 wrapped call sites, unit tests | fingerprint sites in matrix (amiunique-class pages) — 0 js_errors |
| 3 | `Expected int32 … got 1791455494007` (ChatGPT dead) | engine `setTimeout` accepted page-supplied ms timestamps and truncated through i32 → instant-fire loop | `[Clamp]` on the timer binding | **chatgpt.com in the 50-site matrix: ok, 0 js_errors, 351 MB** |
| 4 | Missing Web APIs (IntersectionObserver, adoptedStyleSheets, PerformanceObserver…) | implemented upstream but pref-gated OFF (`const_default()=false`) | 20 `dom_*` prefs flipped ON (D-018 documents what stays off and why) + requestIdleCallback implemented | API audit 63/67, missing_must NONE, 10/10 functional probes |

## 2. The 50-site real-web matrix (Phase 10, run #2, 2026-10-11)

All 50 sites in one headless pass, PGID-scoped RSS, panic gate + JS
console error gate + screenshot. **Result: 50/50 ok — 0 panics, 0 JS
errors.** 30/50 screenshots (the `no-shot` sites are heavy SPAs whose
first paint exceeded the fixed snapshot window in CI; they still loaded
and executed with zero engine errors — recorded as evidence-quality
notes, not failures).

| class | sites (peak RSS MB, software GL) |
|---|---|
| search / reference | example 610, wikipedia 659, duckduckgo 764, bing 770, google 647, mdn 627, w3schools 503 |
| dev | github 1283, stackoverflow 535, hackernews 577 |
| news | bbc 845, nytimes 519, cnn 626, theguardian 698, reuters 715, bloomberg 1423, forbes 1000 |
| publishing / builders | medium 1172, wordpress 782, wix 592, squarespace 1380 |
| media | spotify 879, soundcloud 786, twitch 871, vimeo 1410, youtube 892 |
| design | dribbble 2223, behance 800, deviantart 351, figma 1116, canva 569 |
| social | pinterest 359, linkedin 819, x 861, instagram 792, facebook 673, reddit 360 |
| messaging | whatsapp 733, telegram 586, discord 1441, slack 501, zoom 543 |
| workspace | outlook 593, gmail 661, drive 655, notion 740 |
| AI | **chatgpt 351** (owner-reported int32 crash site — now clean) |

Aggregate: median ≈ 660 MB, spread 351 MB – 2223 MB under software GL
(the heavy tail is image/video-heavy sites whose decoded textures live
in host RAM under llvmpipe; see PHASE6_REPORT.md floor assessment).

## 3. Phase 7 — privacy: rubric-verified ≥ 90/100

10 line items / 100 points, every point a real assertion against the
production engine: 4-list filter stack (EasyList + EasyPrivacy + uBlock
filters + Peter Lowe's, ~13.6 k element-hiding selectors live), CHIPS
send/receive, CNAME-uncloaking detector, DoH pref defaults,
fingerprint payload tiers, session key hygiene, stats persistence.
CI gate fails below 90; the hide_only P0 engine fix (page-flag
exceptions never except network requests) closed the last gap beyond
the 85 → 90 step. Cosmetic filtering runs two-plane (generic user
stylesheet parsed once per thread + per-document domain-scoped sheet at
head-bind, pre-paint).

## 4. Phase 8 — platform: audit-gated

- Media: 11/13 canPlayType probes (H.265 + MSE documented gaps — R-17,
  upstream-scale), all must-tier green.
- Modern CSS: 9/15 (accent-color re-tiered want per D-021; :has(),
  container queries, subgrid, backdrop-filter, text-wrap:balance
  recorded as want-tier upstream gaps).
- IME: composition event machinery + real editing pipeline
  (`execCommand("insertText")` → engine-dispatched beforeinput/input →
  committed text) — PASS in CI (D-022). OS-level IME preedit remains
  owner-hardware validation (R-05, Phase 3).
- Fonts: Noto Sans KR bundled (R-11 Korean tofu closed); CJK/Arabic/
  Hebrew/i18n matrix green since Phase 3.

## 5. Phase 9 — hardening: zero-warning + sanitizer suite green

- **Zero warnings**: all 5 Linux-build warnings from the #102 era
  eliminated (crash_handler cfg gates keeping pure-function tests on
  every platform, idledeadline re-export, Call__ Result) — clippy/fmt
  + compile jobs enforce the zero bar on every push.
- **ASan**: 4/4 brow crates clean (heap/stack/UB instrumented,
  `-Zbuild-std` so std itself is instrumented).
- **TSan**: 4/4 clean with a scoped suppression for tokio's I/O-driver
  readiness bit-packing (third-party false-positive class, documented
  in the workflow + DECISIONS.md); brow code fully gated.
- **Valgrind**: definite-leak gate over every brow-privacy + brow-cache
  test binary — clean; full vg logs archived as artifacts.
- The sanitizer pipeline itself found and fixed: the `--skip`/`--tests`/
  build-std/toolchain-pin recipe chain (8 evidence-driven iterations),
  plus verification that DedupPool's locking is sound (the one TSan hit
  was proven a std-uninstrumented artifact, then eliminated by
  -Zbuild-std rather than suppressed).

## 6. Phase 6 — memory: honest floor, levers shipped

jemalloc residency tune (background_thread + 5 s decays), HTTP cache
5000→2000, JS heap 255 MB guard, image-cache LRU byte budget (64 MB,
compositor-key-releasing eviction), background-tab throttling verified.
5-tab soak: 0 panics / 0 errors, peak 700 MB, steady 681 MB under
software GL where the process floor alone is ~500 MB — marginal cost
≈ 40 MB/tab above floor. The <100 MB/tab owner target moves to
owner-hardware validation with this artifact (full analysis:
PHASE6_REPORT.md, R-06).

## 7. Deliverables (run #113+ artifacts)

- `brow-servo-windows-x86_64` — production build + smoke test + bundled
  fonts + privacy lists.
- `brow-servo-linux-x86_64` — production-stripped.
- `brow-phase10-evidence` — 50 site logs + 30 screenshots + matrix.md.
- `valgrind-logs` — full Valgrind logs (phase 9).
- `brow-phase5-evidence` / phase 8 audit JSONs — API + platform proof.

## 8. Known limits (documented, not hidden)

- MSE/MediaSource and H.265: absent upstream (R-17) — YouTube playback
  beyond limited paths is an upstream-scale gap; the site still loads,
  renders and runs (0 panics/errors in matrix).
- WebGPU/WebRTC/WebXR: off by pref with documented reasons (D-018).
- `accent-color`, `:has()`, container queries, subgrid,
  backdrop-filter, text-wrap: balance: stylo upstream gaps (want-tier,
  graceful degradation — SERVO_UPSTREAM.md).
- OS-IME preedit, GPU-composited RAM, and the <100 MB/tab target:
  owner-hardware validation items (R-05/R-06) — CI cannot manufacture
  a GPU or an IME; everything CI-verifiable is green.
