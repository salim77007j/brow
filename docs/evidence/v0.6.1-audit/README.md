# v0.6.1 real-binary audit evidence (2026-10-08)

Source: the **released** `brow-0.6.1-x86_64-linux-gnu.tar.gz` (not a local
build), executed under Xvfb (1280x840x24) with software GL (`LIBGL_ALWAYS_SOFTWARE=1`,
llvmpipe). Sandbox caveats: software GL inflates RSS vs a real iGPU, network
is MITM-proxied (DoH to dns.google fails TLS, engine falls back to the system
resolver), so absolute wall-times are network-dominated and not comparable to
real machines. The **structural** findings are environment-independent.

## Files

| File | What it shows |
|---|---|
| `brow-shell-example.com-t12s.png` | Full X root framebuffer 12 s after launch. **No toolbar visible anywhere** — the content window covers the 88 px chrome window (unmanaged stacking). |
| `brow-shell-wikipedia-t12s.png` | Same for wikipedia.org: no toolbar; **CJK tofu boxes** (中文/日本語/한국어 render as boxes; only Latin+Arabic fonts shipped); **black rendering artifact** where the globe puzzle image should be. |
| `brow-shell-example.com.log` | Engine log: two winit windows created before content load; privacy engine active (`filter engine loaded 54934 network rules`, fingerprint defenses). |
| `brow-shell-wikipedia.log` | Engine log for wikipedia run. |

## Measured numbers (single tab, software-GL VM)

| Metric | example.com | wikipedia.org |
|---|---|---|
| Peak RSS (main process) | **385 MB** | **423 MB** |
| Main document 200 received | +1 s from launch | +1 s from launch |
| Subresources flowing | +11 s (s.js) | +1 s |

Baseline reference points for v0.7 targets: Chrome/Edge-class cold-start RSS
on a real desktop is roughly 300–450 MB with several tabs (public teardowns);
brow v0.6.1 matches that with **one** tab and no real GPU work.

## Structural findings these files prove

1. **Two-window architecture**: chrome (1240x88) and each tab are separate
   top-level windows; no positioning code exists in the shell
   (`grep -rn set_outer_position servo/ports/brow-shell/src` → 0 hits), so
   stacking/z-order decides what the user sees. Screenshots: toolbar invisible.
2. **CPU-drawn chrome**: Slint `MinimalSoftwareWindow` → CPU buffer →
   softbuffer per-pixel blit (`ports/brow-shell/src/platform.rs:252-274`).
3. **No IME handling**: no `WindowEvent::Ime` arm in either window's event
   router → CJK/Arabic IME input cannot work.
4. **Font coverage**: only Noto Sans (Latin) + Noto Sans Arabic bundled →
   CJK tofu (screenshot).
5. **Engine profile**: shipped at `opt-level = "s"` (size-optimized) because
   opt-2/opt-3 builds SIGSEGV on wikipedia.org (see
   `servo/Cargo.toml` profile comment); no PGO. Black-globe artifact suggests
   an additional raster bug to investigate in Phase 4.
6. **Privacy engine works**: rule engine + fingerprint defenses active in
   both logs — confirmed NOT the current failure.
