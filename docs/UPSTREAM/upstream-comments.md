# Upstream comment drafts

## 1. servo/servo#46936 — IME `isComposing` is hardcoded false

Context: brow owner-hardware validation (Windows 11, zh pinyin / ja / ar
IME) failed in the servoshell IME relay, independently of the engine-side
composition plumbing. We fixed the shell side (egui-winit allowance fight +
Windows `Ime::Disabled`-after-commit mapping to `Dismissed` that blurred the
editable after every commit — see
`ports/servoshell/desktop/ime.rs`, a unit-tested state machine) and can
confirm from real-hardware traces that the engine's `CompositionEvent`
dispatch works end to end once the shell stops dropping sessions.

On the `isComposing` gap specifically: the shell relays
`Ime::Enabled/Preedit/Commit` faithfully, but `KeyboardEvent.isComposing`
stays false for keys typed during composition, which breaks IME-aware
pages (e.g. Japanese editor widgets that suppress Enter during
composition). The blueprint in this issue matches what we need on the
embedder side as well: the shell must know whether a `KeyboardEvent` was
produced while a composition session was open (our session tracker holds
exactly that state and could pass it through `InputEvent::Keyboard` if the
engine wants it). Happy to test a patch on Windows with CJK/Arabic IMEs.

## 2. servo/servo#42593 / #45668 — stale scroll-node panics

Context: while triaging owner-reported fast-scroll crashes on Windows
(later attributed to the no-LTO release configuration, #48109), we
catalogued the Rust-panic candidates in the paint/scroll path that would
kill the process under fast scrolling even after the codegen fix:

- `make_current` expects in the egui present path
  (`ports/servoshell/desktop/gui.rs`, `desktop/window.rs`),
- painter-shutdown race `expect("painter_id not found")`
  (`ports/servoshell/egui/paint/paint.rs`),
- stale-webview `panic!` (`ports/servoshell/running_app_state.rs`),
- pinch-transform inversion expects (`paint/pinch_zoom.rs`),
- `pending_frames` underflow (`paint/painter.rs`).

These overlap the stale-scroll-node panic classes discussed in #42593 and
#45668. Once our minidump capture lands upstream (see our crash-handler PR
draft), reproducing these becomes actionable on end-user hardware. We are
also shipping per-burst wheel coalescing in the shell, which reduces the
event pressure that aggravates these races; happy to provide traces from
AMD iGPU hardware if useful.

## 3. servo/servo#38072 — wheel scroll distance

We bumped servoshell's `LINE_HEIGHT`/`LINE_WIDTH` from 76 px to 100 px in
brow after owner-hardware feedback that scrolling undershot Firefox/Windows
Notepad on the same hardware. No issues observed with 100 px over a
validation pass on Windows (LineDelta mice); reinforces the upstream
direction.
