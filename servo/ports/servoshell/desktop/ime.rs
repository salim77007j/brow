/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! IME session tracking for the headed window (Phase 4.3).
//!
//! Windows/winit sends `Ime::Disabled` at the end of *every* composition
//! session — including a successful commit — but servoshell must only treat
//! `Disabled` as a user dismissal (which blurs the focused editable) when no
//! commit is pending. Mapping every `Disabled` to `Dismissed` made typing die
//! after each commit: the editable lost focus, and winit gated off all
//! further `WM_IME_*` delivery, so CJK/Arabic input produced "separated
//! words, no preedit" on owner hardware.
//!
//! This module is a pure state machine (no winit types) so the mapping table
//! is unit-testable on every platform.

use servo::CompositionEvent;
use servo::CompositionState;

/// What the window should do for a given raw winit IME event.
#[derive(Debug, PartialEq)]
pub(crate) enum ImeAction {
    /// Forward a Composition event (Start/Update/End) to the engine.
    Composition(CompositionEvent),
    /// The IME session was genuinely dismissed: blur the focused editable
    /// (and clear the page's IME ownership).
    Dismissed,
    /// Windows session-end signal right after a successful commit. Forward
    /// nothing: the editable must stay focused so the user can compose the
    /// next word without re-clicking.
    SessionEnd,
    /// Nothing to do (e.g. engine-initiated hide already handled the blur).
    Ignore,
}

/// Tracker state: whether a composition is in flight, and whether a commit
/// already ended it (so the trailing winit `Ime::Disabled` is a session end).
#[derive(Debug, Default, PartialEq)]
enum SessionState {
    /// No composition in flight.
    #[default]
    Idle,
    /// `Enabled`/`Preedit` seen, no `Commit` yet.
    Composing,
    /// `Commit` seen; expect the platform's trailing `Ime::Disabled`.
    CommitPendingEnd,
}

#[derive(Debug)]
pub(crate) struct ImeSessionTracker {
    state: SessionState,
    /// Whether this platform sends `Ime::Disabled` at the end of every
    /// composition session — including successful commits. True on Windows
    /// (winit's `WM_IME_*` delivery); false on macOS/Linux, where a commit
    /// ends the session silently and a later `Disabled` always means
    /// dismissal. Constructed with `cfg!(target_os = "windows")`.
    commit_sends_disabled: bool,
}

impl ImeSessionTracker {
    pub(crate) fn new(commit_sends_disabled: bool) -> Self {
        Self {
            state: SessionState::Idle,
            commit_sends_disabled,
        }
    }

    /// `winit::event::Ime::Enabled`
    pub(crate) fn on_enabled(&mut self) -> ImeAction {
        // Start a fresh session. winit may re-send Enabled when the user
        // re-arms the IME after a dismissal; restarting cleanly is correct.
        self.state = SessionState::Composing;
        ImeAction::Composition(CompositionEvent {
            state: CompositionState::Start,
            data: String::new(),
        })
    }

    /// `winit::event::Ime::Preedit(text, caret)`
    pub(crate) fn on_preedit(&mut self, text: String) -> ImeAction {
        match self.state {
            SessionState::Idle => {
                // Missed Enabled (platform-dependent delivery). Treat this as
                // an implicit start, but forward only the Update: a synthetic
                // Start here would emit a spurious compositionstart to pages.
                self.state = SessionState::Composing;
                ImeAction::Composition(CompositionEvent {
                    state: CompositionState::Update,
                    data: text,
                })
            },
            SessionState::Composing | SessionState::CommitPendingEnd => {
                self.state = SessionState::Composing;
                ImeAction::Composition(CompositionEvent {
                    state: CompositionState::Update,
                    data: text,
                })
            },
        }
    }

    /// `winit::event::Ime::Commit(text)`
    pub(crate) fn on_commit(&mut self, text: String) -> ImeAction {
        // Windows sends Ime::Disabled after every commit; suppress the blur
        // for that trailing event regardless of whether we saw the full
        // Preedit stream (winit can fast-path a commit without Preedit).
        self.state = if self.commit_sends_disabled {
            SessionState::CommitPendingEnd
        } else {
            // macOS/Linux: no trailing Disabled — the session is over. A
            // later Disabled here means a genuine dismissal.
            SessionState::Idle
        };
        ImeAction::Composition(CompositionEvent {
            state: CompositionState::End,
            data: text,
        })
    }

    /// `winit::event::Ime::Disabled`. `page_owns_ime` is true while the page
    /// (not the URL bar) requested an IME (`show_ime` ran, `hide` not yet).
    pub(crate) fn on_disabled(&mut self, page_owns_ime: bool) -> ImeAction {
        match self.state {
            SessionState::CommitPendingEnd => {
                self.state = SessionState::Idle;
                ImeAction::SessionEnd
            },
            SessionState::Composing => {
                self.state = SessionState::Idle;
                if page_owns_ime {
                    // The user canceled an in-flight composition (no commit):
                    // genuine dismissal — blur the editable.
                    ImeAction::Dismissed
                } else {
                    // The engine hid the IME (focus moved elsewhere): the
                    // newly focused element must not be blurred.
                    ImeAction::Ignore
                }
            },
            SessionState::Idle => {
                if page_owns_ime {
                    // Dismissal before any composition (user turned the IME
                    // off): blur, matching upstream's intent.
                    ImeAction::Dismissed
                } else {
                    // Engine-initiated hide already handled focus (the hide
                    // path cleared IME ownership before this event arrived).
                    ImeAction::Ignore
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start() -> ImeAction {
        ImeAction::Composition(CompositionEvent {
            state: CompositionState::Start,
            data: String::new(),
        })
    }

    fn update(text: &str) -> ImeAction {
        ImeAction::Composition(CompositionEvent {
            state: CompositionState::Update,
            data: text.to_string(),
        })
    }

    fn end(text: &str) -> ImeAction {
        ImeAction::Composition(CompositionEvent {
            state: CompositionState::End,
            data: text.to_string(),
        })
    }

    fn windows_mode() -> ImeSessionTracker {
        ImeSessionTracker::new(true)
    }

    fn non_windows_mode() -> ImeSessionTracker {
        ImeSessionTracker::new(false)
    }

    #[test]
    fn committed_session_must_not_blur_on_windows() {
        // The owner bug: zh pinyin commit → winit Ime::Disabled → blur →
        // "typing dies after each commit".
        let mut t = windows_mode();
        assert_eq!(t.on_enabled(), start());
        assert_eq!(t.on_preedit("ni hao".into()), update("ni hao"));
        assert_eq!(t.on_commit("你好".into()), end("你好"));
        assert_eq!(t.on_disabled(true), ImeAction::SessionEnd);
        assert_eq!(t.on_disabled(true), ImeAction::Dismissed); // next real dismissal blurs
    }

    #[test]
    fn committed_session_without_trailing_disabled_blurs_on_macos_linux() {
        // macOS/Linux end the session silently; a later Disabled is a
        // genuine dismissal and must blur like upstream did.
        let mut t = non_windows_mode();
        assert_eq!(t.on_enabled(), start());
        assert_eq!(t.on_preedit("ni hao".into()), update("ni hao"));
        assert_eq!(t.on_commit("你好".into()), end("你好"));
        assert_eq!(t.on_disabled(true), ImeAction::Dismissed);
    }

    #[test]
    fn canceled_composition_blurs() {
        for mut t in [windows_mode(), non_windows_mode()] {
            assert_eq!(t.on_enabled(), start());
            assert_eq!(t.on_preedit("abc".into()), update("abc"));
            assert_eq!(t.on_disabled(true), ImeAction::Dismissed);
        }
    }

    #[test]
    fn fast_commit_without_preedit_still_suppresses_the_trailing_disabled_on_windows() {
        let mut t = windows_mode();
        assert_eq!(t.on_commit("x".into()), end("x"));
        assert_eq!(t.on_disabled(true), ImeAction::SessionEnd);
    }

    #[test]
    fn preedit_without_enabled_is_an_implicit_start() {
        let mut t = windows_mode();
        assert_eq!(t.on_preedit("あ".into()), update("あ"));
        assert_eq!(t.on_commit("あ".into()), end("あ"));
        assert_eq!(t.on_disabled(true), ImeAction::SessionEnd);
    }

    #[test]
    fn engine_initiated_hide_disables_without_blur() {
        // Focus moved (e.g. to the URL bar): the hide path already cleared
        // ownership, so the Disabled that winit emits must not blur anything.
        for mut t in [windows_mode(), non_windows_mode()] {
            assert_eq!(t.on_enabled(), start());
            assert_eq!(t.on_preedit("a".into()), update("a"));
            assert_eq!(t.on_disabled(false), ImeAction::Ignore);
        }
    }

    #[test]
    fn dismissal_before_any_composition_blurs_when_page_owns_ime() {
        for mut t in [windows_mode(), non_windows_mode()] {
            assert_eq!(t.on_disabled(true), ImeAction::Dismissed);
            let mut t2 = ImeSessionTracker::new(false);
            assert_eq!(t2.on_disabled(false), ImeAction::Ignore);
        }
    }

    #[test]
    fn re_enabled_mid_session_restarts_cleanly() {
        let mut t = windows_mode();
        assert_eq!(t.on_enabled(), start());
        assert_eq!(t.on_preedit("a".into()), update("a"));
        // winit re-arms the IME without an intervening Disabled.
        assert_eq!(t.on_enabled(), start());
        assert_eq!(t.on_commit("b".into()), end("b"));
        assert_eq!(t.on_disabled(true), ImeAction::SessionEnd);
    }
}
