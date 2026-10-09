# Phase 3 evidence — i18n fonts / RTL / DPI (D-013)

## brow-i18n.png (1×) and brow-dpi2x.png (2× device-pixel-ratio)

Headed Xvfb captures (ImageMagick `import -window root`) of the product
shell loading the checked-in test page `i18n-test.html` via `file://`,
from CI run **37869415068** (commit 136fa8d1a, job "Build Servo from
source (Linux x86_64, release)", step "Evidence — i18n fonts + 2× DPI
(Phase 3, D-013)"; run conclusion **success** — fast gates 5/5, Linux
engine build + smoke, and the Windows build gate all green in the same
run). `brow-dpi2x.png` was taken from the same binary with
`--device-pixel-ratio 2.0`.

What the captures prove (vs the v0.6.x tofu failure class):

- **CJK Hanzi render** — zh-CN line reads "你好，世界。浏览器应显示汉字
  而不是方框（中文测试）。" and the characters are glyphs, not 方框
  (tofu): the bundled Noto Sans SC payload + installer (3.1/3.2) +
  egui-chrome fallback wiring (3.3) closed the font-availability gap the
  Phase 1 audit identified. Hiragana/Katakana (ja line) render too.
- **Arabic shaping + RTL** — the ar line shows connected Arabic glyph
  forms (not isolated codepoints) right-aligned; the he line renders
  right-aligned Hebrew. Engine content bidi was already correct per
  Phase 1 evidence; these lines confirm the bundled Arabic/Hebrew faces
  also reach the shell path.
- **Mixed-script text in one field** — the form input contains
  "你好 مرحبا こんにちは" with all three scripts rendering; the tab
  title "brow i18n evidence —…" also renders its em-dash and CJK
  correctly.
- **Enclosed/fullwidth forms** — ①②③ and fullwidth "ＡＢＣ１２３、句读
  符号：「测试」" render (SC coverage notes in fonts/README.md).
- **2× DPI scaling** — `brow-dpi2x.png` shows the same page at 2×:
  doubled glyph raster with correct layout reflow (text wraps at the
  halved CSS viewport), no double-render or blur artifacts. Chrome
  (toolbar/tab strip) scales with the page.
- **Privacy chrome intact** — "0 blocked" status item visible in both
  captures (correct for a file:// page; D-011 counter path live).

Known gaps recorded honestly (not visible in these captures):

- No Hangul face is bundled (R-11 / D-013): bare-Linux Korean stays tofu;
  Windows covers Korean via Malgun Gothic system fallback.
- IME composition (preedit/commit) is NOT exercised by these captures —
  it cannot be driven from Xvfb headless input; the owner IME matrix in
  docs/OWNER_TESTS_PHASE3.md §1 is the closer for R-05.
- UI-chrome localization strings remain deferred (D-014) — egui 0.34
  complex-script shaping for UI text is unproven; page content RTL is
  unaffected.

Same-run gate numbers: binary size gate green (160 MiB), smoke markers
"filter engine loaded" + "fingerprint defenses active" green, 7/7 jobs
success (run 37869415068, read from the GitHub Actions API 2026-10-09).
