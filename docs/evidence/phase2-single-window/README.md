# Phase 2 evidence — single-window rebuild (v0.7, D-010/D-011)

## brow-single-window-example.com.png

Headed Xvfb capture (ImageMagick `import -window root`) of the rebuilt
product shell — thin bin over the libified servoshell desktop shell —
running `https://example.com` with `block_ads: true`, from CI run
37830095914 (commit 39ce1ee0e, job "Build Servo from source (Linux x86_64,
release)", step "Evidence — headed single-window shell screenshot").

What the capture proves (vs the v0.6.1 two-window failure class):

- ONE OS window: the toolbar and tab strip are INSIDE the same window as
  the content — no 1240×88 chrome strip window, no per-tab OS windows, so
  the toolbar-covered-by-content, focus-steal and one-taskbar-icon-per-tab
  bug classes are structurally gone.
- GPU-drawn egui chrome: back/forward/stop, URL bar (showing
  `https://example.com/`), the brow privacy status item ("0 blocked" —
  correct for example.com, which serves no ads; D-011 counter path live
  end-to-end: engine record_block → global atomics → provider → toolbar),
  the ☢ experimental toggle, and the tab strip (page title "Example
  Domain" + close, "+", new-window buttons).
- The webview area is composited via the servoshell PaintCallback path
  (offscreen WebRender target → egui background layer → present); the page
  loaded per the engine log in the same run (HTTP 200, both privacy
  markers, tab title extracted from the DOM).

Known observation for owner validation: the content area paints white in
this software-GL capture; example.com is a near-white page and its text
level is not verifiable at this capture scale. Text-level rendering is
checked on owner hardware (Phase 2 owner validation + the Phase 5 20-site
run on a real GPU).

Same-run gate numbers: size 149 MiB ≤ 160 gate; smoke
`signals=1 alive=yes bs_pid=56454 rss_kb=376864` (368 MiB software-GL
< 600 MB guardrail); 7/7 jobs green.
