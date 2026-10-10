/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use script_bindings::root::DomRoot;

use crate::dom::document::Document;
use crate::dom::html::document_structure::htmlheadelement::HTMLHeadElement;
use crate::dom::node::NodeTraits;
use crate::dom::window::Window;
use crate::realms::enter_auto_realm;

/// brow (v0.6.1 reassessment, fix 3.2): the anti-fingerprinting defense
/// payload is now generated **inside the engine** and evaluated before any
/// shell-provided userscript on every document.
///
/// v0.6.0 shipped this defense as a file that (a) was only written when a
/// config dir existed and (b) was only executed when a shell passed
/// `--userscripts` — no shell did, so fingerprinting protection was dead
/// code in every real-world session. Embedding the generation in the script
/// crate removes both failure modes: no file IO, no shell cooperation, and
/// the `network_privacy_fingerprint_level` pref ("off" | "standard" |
/// "strict", default standard) is the single switch.
///
/// The payload is cached per level string (the settings panel can cycle the
/// level at runtime; a new level regenerates a new session key for that
/// level while repeated documents reuse the cache).
fn builtin_fingerprint_defense() -> String {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static DEFENSE_CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let cache = DEFENSE_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let level = servo_config::pref!(network_privacy_fingerprint_level);
    if let Some(cached) = cache.lock().unwrap().get(&level) {
        return cached.clone();
    }
    use brow_privacy::fingerprint::{DefenseLevel, FingerprintConfig, SessionKey};
    let level_kind = match level.as_str() {
        "off" => DefenseLevel::Off,
        "strict" => DefenseLevel::Strict,
        _ => DefenseLevel::Standard,
    };
    let payload = brow_privacy::fingerprint::build_payload(
        level_kind,
        &SessionKey::from_entropy(),
        &FingerprintConfig::default(),
    );
    if !payload.script.is_empty() {
        log::info!(
            "brow privacy: fingerprint defenses active (level {:?}, {})",
            payload.level,
            payload.active.join(", ")
        );
    }
    let mut guard = cache.lock().unwrap();
    // Re-check under the lock (two documents racing the same new level).
    guard.entry(level).or_insert_with(|| payload.script).clone()
}

pub(crate) fn load_script(head: &HTMLHeadElement) {
    let doc = head.owner_document();
    // brow (v0.6.1): builtin defense first, then shell userscripts.
    let builtin = builtin_fingerprint_defense();
    let userscripts = doc.window().userscripts().to_owned();
    if builtin.is_empty() && userscripts.is_empty() {
        return;
    }
    // brow (phase5.3): remember WHICH document the userscripts were queued
    // for. The delayed task runs after parsing completes, but a navigation
    // may have replaced this document in the browsing context by then
    // (iframe about:blank swap, site redirects). Owner-hardware evidence:
    // `assertion failed: self.can_run_script()` (globalscope.rs) — the
    // script-thread panic that shows up as "page crashed" — and
    // `SecurityError: Location's relevant Document is not same
    // origin-domain` from the payload's `location.origin` read. Both are
    // this one race: evaluate against a window whose ACTIVE document is
    // dead or belongs to another origin.
    let queued_doc = DomRoot::from_ref(&*doc);
    let win = DomRoot::from_ref(doc.window());
    doc.add_delayed_task(task!(UserScriptExecute: move |cx, win: DomRoot<Window>| {
        // brow (phase5.3): "check if we can run script" (HTML spec §8.1.5).
        // The spec REQUIRES this gate before evaluating; upstream's
        // `assert!(can_run_script())` inside `evaluate_js_on_global`
        // assumes every caller ran it. Skip when the document was replaced
        // (identity mismatch — its replacement gets its own userscript run)
        // or when it is not fully active / scripting is sandboxed.
        let same_document = std::ptr::eq::<Document>(&*win.Document(), &*queued_doc);
        if !same_document {
            return;
        }
        let global_scope = win.as_global_scope();
        if !global_scope.can_run_script() {
            return;
        }
        let mut realm = enter_auto_realm(cx, global_scope);
        let cx = &mut realm.current_realm();

        let builtin = builtin_fingerprint_defense();
        if !builtin.is_empty() {
            _ = global_scope.evaluate_js_on_global(
                cx,
                builtin.into(),
                "brow:fingerprint-defense",
                None,
                None,
            );
        }
        for user_script in userscripts {
            _ = global_scope.evaluate_js_on_global(
                cx,
                user_script.script().into(),
                &user_script.source_file().map(|path| path.to_string_lossy().to_string()).unwrap_or_default(),
                None,
                None,
            );
        }
    }));
}
