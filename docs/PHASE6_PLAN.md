# Phase 6 — Resident memory (target: <100 MB/tab, 5 tabs <300 MB)

Status: IN PROGRESS (started 2026-10-10). Owner rule: every fix is
commit+push+CI-verified; evidence-based renegotiation if the floor proves
higher (R-06 mitigation — measurements, not guesses).

## Baseline (run #88, CI software GL, process-tree peak RSS)

| site | peak RSS MB |
|---|---|
| example.com | 483 |
| wikipedia | 540 |
| github | 1188 |
| duckduckgo | 641 |
| bing | 655 |
| stackoverflow | 449 |
| mdn | 524 |
| reddit | 490 |
| hackernews | 464 |
| bbc | 769 |
| w3schools | 380 |
| xkcd | 508 |

Caveats recorded before acting on these numbers:

1. **Sampler fidelity**: the harness sampled `pgrep -f servoshell` across
   ALL servoshell processes — a site whose process outlived its 60 s
   timeout contaminates later sites' samples (github 1188 is suspect).
   Fixed in 6.8: each site runs in its own process group (setsid) and
   sampling walks the launched PGID only.
2. **Software GL**: llvmpipe render buffers inflate RSS vs the owner's
   GPU. CI numbers are an upper-bound environment; the multi-tab steady
   state (6.8) is the decision input.

## Research summary (source-verified 2026-10-10)

- **Allocator**: jemalloc (use-jemalloc, Linux) with ZERO runtime tuning —
  `background_thread` defaults false, dirty/muzzy decays default 10 s, so
  freed pages return to the OS only as a side effect of allocation churn.
  A mimalloc module exists (phase 3 strategy note) but is not active.
- **JS heap**: `js_mem_max: -1` → `JSGC_MAX_BYTES = u32::MAX` (unbounded);
  small-heap band 100 MB, large-heap band 500 MB, growth 150-300 %.
  Incremental GC cannot be enabled (pre-barriers broken, servo/servo#7621).
- **HTTP memory cache**: `network_http_cache_size: 5000` entries (unit
  weighter) — every response body ≤ the entry cap is retained in RAM.
- **Image cache** (`components/net/image_cache.rs`): `completed_loads`
  keeps encoded bytes (Arc<Vec<u8>>) AND decoded RasterImage per image;
  no byte budget, no decoded-data eviction (6.9 candidate — needs the
  6.8 numbers first; re-decode-from-bytes path exists).
- **Font fallbacks**: per-FontGroup fallback map grows unbounded
  (upstream comment, font.rs:778) — upstream-scale, low win, deferred.
- **Background tabs**: ALREADY SHIPPED phase 3 (running_app_state.rs
  activate_webview: hidden tabs throttled — timers clamp, compositor
  ticks stop). No work needed; verified present.

## Sub-items

| # | item | change | status |
|---|---|---|---|
| 6.1 | research (this doc) | source-verified lever list | DONE |
| 6.2 | jemalloc residency tune | `allocator::tune_residency()` — background_thread ON, dirty/muzzy decay 5 s; called in chrome AND content processes | COMMIT A |
| 6.4 | HTTP memory cache trim | 5000 → 2000 entries | COMMIT A |
| 6.5 | JS heap bounds | js_mem_max 255 MB (leak guard; in_range band is EXCLUSIVE of 256 — 256 would silently stay unbounded), small-heap 32 MB, large-heap 128 MB, growth 120-200 % | COMMIT A |
| 6.8 | CI multi-tab gate + sampler fix | N positional URLs open N tabs (browser-standard CLI behavior); 5-tab step with PGID-scoped sampling | COMMIT B |
| 6.3 | image-cache byte budget | decoded/encoded LRU — DECIDES on 6.8 numbers | PENDING |
| 6.6 | font fallback LRU | upstream-scale; deferred with note | DEFERRED |
| 6.7 | background-tab freeze | already shipped phase 3 | DONE (verified) |
| 6.9 | phase report | before/after table, floor assessment, honest verdict vs 100 MB target | PENDING |

## Honest framing (R-06)

The <100 MB/tab target was set against a Servo architecture whose CI
baseline floor (example.com, a near-empty page) is ~483 MB under software
GL. The levers above attack allocator residency, caches, and heap bounds;
the measured floor after 6.2-6.8 decides whether the target is reachable
in-engine or needs renegotiation with numbers (owner GPU vs CI software
GL included in the comparison).
