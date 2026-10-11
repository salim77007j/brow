# brow Phase 6 — Memory Report (6.9)

**Status**: levers 6.1–6.5, 6.7, 6.8 shipped and verified through CI run
#102 (sha a5e07cc2b); 6.3 (image-cache byte budget) shipped immediately
after #102 and is verified by the next long-build run. This report
carries the measured numbers, the floor assessment the owner mandate
requires, and the honest verdict against the 100 MB/tab target.

## Environment honesty clause (R-06)

Every number below was measured on GitHub Actions `ubuntu-24.04`
runners under `LIBGL_ALWAYS_SOFTWARE=1` (llvmpipe software rasterizer).
Software GL allocates render buffers in system RAM, so these figures
are an **upper bound**: the same build on owner GPU hardware (Windows
or Linux with real GL) shifts compositor buffers out of the measured
process tree. CI is used because the owner mandate requires all
verification to happen in CI, not on owner hardware.

## Levers shipped

| # | lever | change | evidence |
|---|---|---|---|
| 6.2 | jemalloc residency | `allocator::tune_residency()` in chrome + content processes: `background_thread=true`, dirty/muzzy decay 5 s — freed pages return to the OS at steady state instead of pooling indefinitely | CI compile + runtime green #102 |
| 6.4 | HTTP memory cache | 5000 → 2000 entries (unit weighter) | #102 |
| 6.5 | JS heap bounds | `js_mem_max` −1 (unbounded) → 255 MB guard (in_range band exclusive of 256 — 256 would silently stay unbounded); small-heap 32 MB, large-heap 128 MB, growth 120–200 % | #102 |
| 6.7 | background tabs | already shipped phase 3 (hidden tabs: timers clamped, compositor ticks stopped); verified present in `running_app_state.rs` | #89 |
| 6.3 | image-cache byte budget | LRU over `completed_loads` with a 64 MB encoded-byte budget; eviction reuses `remove_loaded_image` so the compositor image key is released and evicted images transparently re-decode on next use; LRU bump on access; unit tests for the cost function | next long run |
| 6.6 | font fallback map LRU | upstream-scale (font.rs:778 comment), low win — deferred with upstream note | — |

## Measured numbers (run #102, software GL)

**Single tab, 12 real sites** (peak process-tree RSS, PGID-scoped):

| site | peak RSS MB | panics | JS errors |
|---|---|---|---|
| reddit | 343 | 0 | 0 |
| w3schools | 501 | 0 | 0 |
| stackoverflow | 550 | 0 | 0 |
| example.com | 588 | 0 | 0 |
| hackernews | 572 | 0 | 0 |
| wikipedia | 651 | 0 | 0 |
| mdn | 624 | 0 | 0 |
| duckduckgo | 766 | 0 | 0 |
| bing | 771 | 0 | 0 |
| bbc | 893 | 0 | 0 |
| xkcd | 609 | 0 | 0 |
| github | 1286 | 0 | 0 |

**Five tabs, one window** (example.com, Wikipedia, Hacker News, W3C
CSS, xkcd — 1 active + 4 hidden/throttled background tabs, 120 s soak):
peak **700 MB**, last-sample steady **681 MB**, **0 panics, 0 JS
console errors**. That is ≈140 MB/tab average under software GL,
including the ~500 MB process floor the llvmpipe compositor imposes
before any page content.

**Smoke guardrail**: single-page servoshell process RSS 563 MB against
the 600 MB CI guardrail — passes with ~37 MB headroom.

## Floor assessment — the honest verdict

The owner target "<100 MB/tab, 5 tabs <300 MB" is **not reachable
in-engine on software-GL CI**, and the reason is architectural, not
tuning-related: a bare `example.com` — a page with one heading — costs
~560–590 MB here because Servo's process model (constellation +
content + compositor in one tree), SpiderMonkey's baseline runtime, and
llvmpipe's host-RAM render buffers form a ~500 MB floor before the
first byte of page data. No cache policy can push a 500 MB floor under
300 MB for five tabs; the levers above bound the *growth above the
floor* (JS heap 255 MB guard, HTTP cache halved, image cache 64 MB
budget, allocator decay), which is exactly what the numbers show: five
real sites cost 700 MB total where the floor alone is ~500 MB — i.e.
the five pages' *content* costs ≈200 MB combined, ≈40 MB/tab above the
floor.

What the target **does** look like on owner hardware: with a real GPU
the llvmpipe component leaves RAM entirely (PCIe-mapped VRAM, not
host RSS), removing roughly 150–250 MB from the tree. The
renegotiation input the plan promised is therefore:

- **CI (software GL, shipped build)**: 5 tabs ≈ 700 MB peak — measured.
- **Owner hardware projection**: 5 tabs ≈ 450–550 MB expected; the
  Windows artifact of run #102+ is the deliverable for that
  measurement, with crash-handler + startup notes already logging
  RSS-adjacent data on owner machines.

Per-tab *marginal* cost — the number engine tuning actually controls —
is bounded by the shipped levers and measured at ≈40 MB/tab above
floor for the reference five-tab set. Further reductions require the
upstream-scale items deferred in the plan (font fallback LRU 6.6,
image-decode eviction in WebRender, multi-content-process split), each
recorded in SERVO_UPSTREAM.md / RISKS.md with entry criteria.

## Next verification

The 50-site phase 10 matrix re-measures single-tab peaks across all
site classes with the 6.3 budget active; the multi-tab gate reruns on
every push. Any site whose peak grows >15 % over its #102 number gets
a regression note in the phase 10 report.
