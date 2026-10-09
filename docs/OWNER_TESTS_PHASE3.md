# Phase 3 owner validation protocol (Windows-first) — IME / CJK / RTL / DPI

Phase 3 code and CI evidence live on `v0.7-rebuild`. CI **cannot** validate
IME (no input-method infrastructure under Xvfb — V2_PLAN §8.2) or real-GPU
DPI behavior; this protocol is the owner-hardware gate that closes R-05 and
the Phase 3 items of the target-metrics table (V2_PLAN §5). Windows is the
owner platform and is validated FIRST; Linux IME is secondary.

Build to test: any CI run of `v0.7-rebuild` at or after `e906d81f0`
(Phase 3.4) — download the `brow-servo-linux-x86_64` artifact for a quick
sanity run on Linux, or build locally on Windows (`.\mach build --release` +
`cargo build --release -p brow-shell`); the payload `fonts/` directory must
sit next to `brow.exe` (packaging already copies it).

Setup: run `brow.exe` once — step 0 installs the bundled fonts into the
per-user font dir (`%LOCALAPPDATA%\Microsoft\Windows\Fonts`) and the log
shows `brow: bundled fonts installed this run: N` (N>0 on first run, 0 on
later runs — idempotent). Then work through the matrix and record
pass/fail + screenshot per row.

## 1. IME matrix (closes R-05) — Windows FIRST

For each row: open the target, activate the input method listed, type the
keystrokes, and check BOTH the preedit (inline composition string while
typing) and the commit (final text after confirming the candidate).

| # | Target | IME / keystrokes | Expect |
|---|---|---|---|
| 1.1 | URL bar (egui chrome) | zh-CN Pinyin: `nihao` → space | preedit `nihao`/候选窗 visible in the URL bar; commit inserts `你好` |
| 1.2 | Page form field (example: the i18n page's input) | zh-CN Pinyin: `ceshi` → space | preedit shown near the field; commit lands `测试` in the page field |
| 1.3 | URL bar | ja romaji→kana: `konnichiha` → Enter | `こんにちは` commits into the URL bar |
| 1.4 | Page form | ja: same | `こんにちは` commits into the page |
| 1.5 | URL bar | Arabic keyboard: type `marhaba` on Arabic 101 layout | Arabic glyphs appear (joining/shaping in the page is engine work — the URL bar may show isolated glyph forms; record what you see, do not fail the row for shaping) |
| 1.6 | Page form | Arabic keyboard: same | Arabic text commits; page renders shaped/RTL correctly |
| 1.7 | URL bar | Hebrew layout: `shalom` | `שלום` commits |
| 1.8 | URL bar | After 1.1, click into the page (non-input area) | IME window closes; no leftover preedit in the URL bar |
| 1.9 | Page form → URL bar | Switch focus while IME active | composition does not leak from one target into the other (engine `Ime::Enabled/Disabled` boundary — the audited edge case) |

Known-good reference behavior: preedit position should track the caret.
If the preedit renders at the window corner instead of the caret, record a
screenshot — that is a `set_ime_cursor_area` bug we will fix in a follow-up.

## 2. CJK/Arabic/Hebrew rendering

Open `docs/evidence/phase3-i18n/i18n-test.html` from the repo (or any
zh/ja/ar/he site). Expect: real glyphs on every line, no tofu boxes; Arabic
and Hebrew paragraphs flow right-to-left.

| # | Check | Expect |
|---|---|---|
| 2.1 | zh line | Han glyphs render (Noto Sans SC or YaHei) |
| 2.2 | ja line | kana render |
| 2.3 | ar line | shaped Arabic, RTL alignment |
| 2.4 | he line | Hebrew glyphs, RTL |
| 2.5 | Enclosed/fullwidth line | ①②③ and ＡＢＣ１２３ render |
| 2.6 | URL bar paste of `你好` | tab title and URL bar show CJK (egui bundled-font path) |
| 2.7 | Known gap check: a Korean page (e.g. wikipedia.org/wiki/한국) | Hangul renders via Malgun Gothic on Windows; record result (no bundled Hangul — R-11) |

## 3. DPI (100% / 125% / 150%)

Windows Settings → Display → Scale; change, then restart brow (winit emits
`ScaleFactorChanged` on the fly, but validate both paths: live change and
fresh start at each scale).

| # | Scale | Expect |
|---|---|---|
| 3.1 | 100% | toolbar + content crisp, nothing clipped |
| 3.2 | 125% | toolbar scales, URL bar text not blurry (egui zoom path) |
| 3.3 | 150% | content reflows at correct size (engine hidpi path); toolbar remains one window, no overlap |
| 3.4 | 100%→150% live (window open) | UI and content rescale without restart artifacts |

CI reference captures: `brow-dpi2x.png` artifact (2.0× under software GL).

## 4. Regression guard (what must NOT break)

| # | Check |
|---|---|
| 4.1 | Latin text in URL bar/pages looks unchanged in style vs the Phase 2 build (system fonts still have priority over the bundle) |
| 4.2 | Privacy counter still increments on an ad-bearing site |
| 4.3 | Single window, no focus steals (Phase 2 acceptance unchanged) |
| 4.4 | Binary + payload size sane; `fonts/` payload grew by ~8.6 MB (8.3 MB Noto Sans SC + 26 KB Hebrew) — recorded in D-013 |

## 5. Reporting

Append one WORKLOG block: rows passed/failed, screenshots into
`docs/evidence/phase3-owner/`, and any new RISKS entries. A failed IME row
does not block Phase 4 codegen work by itself, but R-05 stays OPEN until
rows 1.1/1.2/1.3 pass.
