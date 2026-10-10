# Bundled fonts (release payload `fonts/`)

Installed into the per-user font directory on first run by
`brow-shell-core::fonts::install_bundled_fonts` (idempotent, size-checked) so
the ENGINE (font-kit/fontconfig on Linux, and any font-scanning stack) finds
these families even on minimal systems. The egui chrome additionally loads
these files directly by path (`servoshell desktop/gui.rs configure_fonts`).

| File | Family | Covers | Provenance |
|---|---|---|---|
| NotoSans-Regular.ttf / NotoSans-Bold.ttf | Noto Sans | Latin | notofonts.github.io (hinted TTF) |
| NotoSansArabic-Regular.ttf / NotoSansArabic-Bold.ttf | Noto Sans Arabic | Arabic | notofonts.github.io (hinted TTF) |
| NotoSansHebrew-Regular.ttf | Noto Sans Hebrew | Hebrew | notofonts.github.io (hinted TTF, v3.001) |
| NotoSansSC-Regular.otf | Noto Sans SC | Simplified Chinese, Japanese kana, full CJK URO (99.9%), fullwidth, CJK punctuation | noto-cjk repo, Sans/SubsetOTF/SC (v2.004) |
| NotoSansKR-Regular.otf | Noto Sans KR | Hangul syllables + jamo (brow 8.4: closes the bare-Linux Korean tofu gap, R-11) | noto-cjk repo, Sans/SubsetOTF/KR |

Family names are matched by the engine's script-aware fallback tables
(`components/fonts/platform/{windows,macos}/font_list.rs` and
`components/fonts/platform/mod.rs` — "Noto Sans SC" is listed there for
freetype platforms).

Known gaps (honest reporting, R-11):
- No Hangul syllables in the bundle: Korean pages rely on system fonts
  (Windows: Malgun Gothic via the engine fallback table; bare Linux boxes
  without Korean fonts will show tofu for Hangul until a KR face is added).
- Noto Sans SC v2.004 misses 16 of 20,992 CJK Unified Ideographs
  (U+9FF0..U+9FFF, Unicode 15.1 additions).

All fonts are SIL OFL 1.1 (see OFL-LICENSE.txt); reserve font names are not
modified.
