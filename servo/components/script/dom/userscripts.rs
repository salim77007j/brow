/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use script_bindings::root::DomRoot;

use crate::dom::bindings::codegen::Bindings::DocumentBinding::DocumentMethods;
use crate::dom::bindings::codegen::Bindings::NodeBinding::NodeMethods;
use crate::dom::bindings::codegen::UnionTypes::StringOrElementCreationOptions;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::str::DOMString;
use crate::dom::html::document_structure::htmlheadelement::HTMLHeadElement;
use crate::dom::node::{Node, NodeTraits};
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

pub(crate) fn load_script(cx: &mut js::context::JSContext, head: &HTMLHeadElement) {
    // brow (7.3): cosmetic element hiding FIRST — synchronous, at head
    // bind-to-tree, i.e. before the parser produces any body content. Ads
    // matching a filter rule are hidden before first paint: no flash.
    inject_cosmetic_style(cx, head);

    let doc = head.owner_document();
    // brow (v0.6.1): builtin defense first, then shell userscripts.
    let builtin = builtin_fingerprint_defense();
    let userscripts = doc.window().userscripts().to_owned();
    if builtin.is_empty() && userscripts.is_empty() {
        return;
    }
    let win = DomRoot::from_ref(doc.window());
    doc.add_delayed_task(task!(UserScriptExecute: |cx, win: DomRoot<Window>| {
        // brow (phase5.3): "check if we can run script" (HTML spec §8.1.5).
        // The task runs after parsing completes, but a navigation may have
        // replaced this document in the browsing context by then (iframe
        // about:blank swap, site redirects). Owner-hardware evidence: both
        // `assertion failed: self.can_run_script()` (the script-thread
        // panic behind "page crashed") AND `SecurityError: Location's
        // relevant Document is not same origin-domain` (the payload's
        // location read against the browsing context's NEW active
        // document) come from this one missing gate.
        //
        // The spec REQUIRES this check before evaluating; upstream's
        // `assert!(can_run_script())` inside `evaluate_js_on_global`
        // assumes every caller ran it. `can_run_script()` is
        // `is_fully_active() && !sandboxed`, and "fully active" already
        // implies the document IS its browsing context's active document —
        // so this single check covers the identity race too; no separate
        // document capture is needed (and the task! macro only traces its
        // declared fields, so extra DomRoot captures in the closure would
        // be a GC hazard — keep captures JS-free).
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

/// brow (7.3): the per-document cosmetic element hiding — the DOMAIN-SCOPED
/// plane of the filter engine (`site_scoped_result`). The generic plane
/// ships once as a global user stylesheet (Origin::User, shared by every
/// document — see servoshell App::init); re-injecting it per document would
/// duplicate hundreds of KB of CSS per page.
///
/// Applied synchronously at head bind-to-tree (before any body content is
/// parsed): selectors matching this document's host get `display:none
/// !important` before first paint — ads never flash.
///
/// Pref: `network_privacy_cosmetic_filter_enabled` (default on; the single
/// off switch, mirroring the network filter's own pref).
///
/// Safety: the selectors were sanity-checked at list parse time
/// (`selector_is_sane`) — a hostile list cannot inject CSS beyond
/// element-hiding selectors; the payload is plain CSS text in a <style>
/// element, never evaluated as script.
fn inject_cosmetic_style(cx: &mut js::context::JSContext, head: &HTMLHeadElement) {
    if !servo_config::pref!(network_privacy_cosmetic_filter_enabled) {
        return;
    }
    let doc = head.owner_document();
    // about:blank / opaque-origin documents: no host, nothing scoped.
    let Some(host) = doc.url().host_str().map(str::to_owned) else {
        return;
    };
    let css = brow_privacy::lists::site_cosmetic_css(&host);
    if css.is_empty() {
        return;
    }
    let Ok(style_elem) = doc.CreateElement(
        cx,
        DOMString::from("style"),
        StringOrElementCreationOptions::String(DOMString::new()),
    ) else {
        return;
    };
    let text = doc.CreateTextNode(cx, DOMString::from(css));
    if style_elem
        .upcast::<Node>()
        .AppendChild(cx, text.upcast::<Node>())
        .is_err()
    {
        return;
    }
    if head
        .upcast::<Node>()
        .AppendChild(cx, style_elem.upcast::<Node>())
        .is_err()
    {
        return;
    }
    log::debug!("brow privacy: cosmetic stylesheet injected for {host}");
}
