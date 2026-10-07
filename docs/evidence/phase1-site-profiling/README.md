# Phase 1 — brow v0.6.1 site profiling (10 real sites)

**Date:** 2026-10-08 · **Binary:** released v0.6.1 Linux build (`brow`, production-stripped,
opt-level="s" + fat LTO) · **Environment:** headless sandbox, Xvfb 1280×840 + Mesa
software GL (llvmpipe), 2 cores / 3.9 GB RAM.

## Method

Released binary launched per site with `BROW_DATA_DIR` pointed at a fresh profile and
`settings.json` → `{"home_page": <url>, "block_ads": true}`. Process-tree RSS (main +
all children) sampled at 200 ms. Screenshots grabbed from the X root window via
ffmpeg/x11grab at t=25 s and t=50 s (where dwell allows). Fixed dwell per site
(30–75 s), then teardown. Harness: `scripts/profile_sites.py` (sandbox copy; run log
`measure/phase1-sites-run.log`).

## Caveats (read before comparing numbers)

1. **Software GL, 2-core VM.** Absolute load times are not comparable to real
   hardware, so this pass records *structural* signals (renders at all / crash /
   memory), not speed. Load-time benchmarking on real hardware is the owner's
   validation step (Phase 5 protocol).
2. **RSS here includes llvmpipe-side buffers** in the engine's address space;
   software rasterization inflates the number vs a GPU run.
3. **The "×" glyph seen mid-screen in many screenshots is the X11 root cursor**
   (ffmpeg captures it; Xvfb places it near screen center). It is NOT a browser
   rendering bug. This was initially suspected as a cross-site artifact — treat any
   future headless screenshot evidence accordingly (move the pointer or ignore).

## Verdict table

| # | Site | Dwell | Peak tree RSS | Verdict | Evidence notes |
|---|------|-------|--------------:|---------|----------------|
| 01 | example.com | 30 s | 307.1 MB | **GOOD** | Multi-language page renders; **Arabic shaping + RTL correct**; CJK line = tofu boxes; no browser chrome visible (content window covers it) |
| 02 | wikipedia.org (article) | 45 s | 387.1 MB | **GOOD** | Full article layout, infobox + logo images render; **icon glyphs = solid black squares**; small tofu remnants |
| 03 | news.ycombinator.com | 45 s | 305.6 MB | **EXCELLENT** | Pixel-perfect (simple server-rendered HTML) |
| 04 | docs.rs/servo | 40 s | 383.6 MB | **GOOD** | Nav, tabs, sidebar, tables render; release-badge icon = black square |
| 05 | github.com/servo/servo | 60 s | 506.8 MB | **EXCELLENT** | Full app layout incl. nav, file listing, About sidebar, buttons; minor commit-message text overflow |
| 06 | developer.mozilla.org (CSS) | 60 s | 380.4 MB | **GOOD (content) / BROKEN (icons)** | Article renders well; **every icon-font glyph = solid black/blue square** |
| 07 | bbc.com/news | 60 s | 496.3 MB | **GOOD** | Full nav + headlines + images; one unpainted grey region mid-page |
| 08 | old.reddit.com | 60 s | 269.6 MB | **SERVER-BLOCKED** | Reddit network-policy block page ("whoa there, pardner!") — **UA-dependent**, not a rendering failure |
| 09 | x.com profile | 75 s | 616.2 MB | **PARTIAL** | Profile, tabs, buttons, QR widget render; **CJK tofu in sign-in dialog**; avatar/layout glitches |
| 10 | youtube.com | 75 s | 353.3 MB | **POOR** | Skeleton only (header + grey placeholder grid); JS-rendered content never paints |

All 10 sites: **zero crashes, zero hangs** (every run survived its full dwell; exit
via harness teardown only). This confirms the shipped opt-"s" profile's stability.

## Cross-site findings

1. **The engine's real-world baseline is far better than v0.6.x's reputation.**
   GitHub, BBC, Wikipedia, MDN, docs.rs, HN render genuinely usable layouts. The
   v0.6.1 product failure was dominated by the *shell* (no visible toolbar, two-window
   focus chaos) — not by page rendering on these six sites.
2. **CJK glyphs = tofu everywhere** (example.com ja/zh line, x.com dialogs) → no CJK
   font bundled (Phase 3 scope). Arabic shaping itself works — the earlier "Arabic
   broken" assessment applies to *input* (no IME in the shell), not display.
3. **Icon fonts render as solid black squares** (MDN: all icons; wikipedia: logo
   area; docs.rs: badge). Repro-friendly, high-visibility, Phase 3/4 target.
   (Re-scope of the old "wikipedia black artifact" risk R-09: the globe "black box"
   is this glyph/image failure, not a WebRender rasterization bug.)
4. **Memory scales with site weight**: 270–390 MB for static/simple sites,
   495–615 MB for heavy SPAs (x.com, github, bbc) — single tab, tree RSS.
   Consistent with the <200 MB/tab target being a Phase 4 re-architecture problem
   (R-06), not a tuning problem.
5. **old.reddit.com blocks the Servo UA** → a User-Agent policy (spoof-to-Chrome
   option behind privacy pref) is a cheap compatibility win for the plan.

## Raw data

- `summary.json`, per-site `result.json` (verdict, RSS, exit code, log excerpts)
- `0X-*/root_t25.png`, `0X-*/root_t50.png` — X root screenshots
- Per-site `run.log` retained in the sandbox `measure/` dir (engine stdout; available
  on request — not committed to keep the repo lean)
