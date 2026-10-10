# Phase 5 Plan — Web Platform Completeness

Status: EXECUTING. Each item lands as its own commit; CI (run on
`v0.7-rebuild`) is the only verification source. Owner-hardware error
reports from the v0.7 test session are the seed list.

## 5.0 Owner-reported errors — root causes (researched, not guessed)

| # | Report | Root cause (source-verified) | Fix | Status |
|---|--------|------------------------------|-----|--------|
| 1 | `assertion failed: self.can_run_script()` → page crash | `components/script/dom/userscripts.rs` queues a delayed task holding only the `Window`. After a redirect / iframe about:blank swap the browsing context's active document is dead or replaced; `evaluate_js_on_global` then asserts `can_run_script()` (= `doc.is_fully_active() && !sandboxed`, `script_execution.rs:307`) and panics the script thread. The spec's "check if we can run script" step was simply not run by this caller. | Task now captures the queued `Document` and skips when the active document changed identity or `can_run_script()` is false. | FIXED (5.3) |
| 2 | `SecurityError: Location's relevant Document is not same origin-domain` at `brow:fingerprint-defense:10:24` | Same race, error path: `location.rs:document_if_same_origin()` compares the entry origin with the browsing context's NEW active document → cross-origin → throw. The throwing token is `location.origin` (line 9/10 col 24 of the payload). | (a) engine guard above, (b) payload reads `location.origin` inside try/catch and degrades to session seeds, (c) payload hardening below. | FIXED (5.4) |
| 3 | "IntersectionObserver is not defined", adoptedStyleSheets undefined, PerformanceObserver "No valid entry type" | The interfaces ARE implemented (`components/script/dom/intersectionobserver/`, `adoptedstylesheet`, ...) but gated `[Pref="dom_intersection_observer_enabled"]` etc., and `components/config/prefs.rs:const_default()` ships them **false**. brow looked like a 2015 browser to feature-detection code. | Flip 20 implemented-but-off dom_* prefs to true (5.1-a). PerformanceObserver: supported entry types are mark/measure/LCP/paint/navigation/resource/visibility-state; unknown types warn per spec — documented, not a crash. | FIXED (5.1-a) |
| 4 | `Expected int32 to be within [-2147483648, 2147483647], got 1791455494007` | Not in servo tree, mozjs 0.26.3, or mozjs_sys 153.3.0-0 (all grepped). Value = epoch-ms timestamp → a JS→Rust i32 conversion on a path ChatGPT exercises. | Reproduce on CI with `RUST_BACKTRACE=1` (site matrix captures panics with source locations) and fix the exact conversion. Blocking: exact site + backtrace. | REPRO-QUEUED (5.2) |
| 5 | more unenumerated | — | CI site matrix (5.5) is the systematic sweep: per-site console/RSS/screenshot evidence, panic gate. | HARNESS LIVE |

## 5.1 Web API audit — machine-readable

- `docs/evidence/phase5/api-audit.html`: probes ~60 surfaces (observers,
  CSS OM, performance timeline, animations, fetch, URL, storage, crypto,
  graphics, workers, files, i18n, clipboard, notifications, MSE, ...).
  Levels: **must** (absence = CI failure) vs **want** (recorded).
- CI step "Phase 5 — Web API audit + real-site headless matrix" runs it
  headless (`servoshell -z`), parses `BROW_API_AUDIT_JSON` from stdout,
  uploads `brow-phase5-evidence` artifact.

### 5.1-a Enabled by default this iteration (implemented upstream, was pref-off)

adoptedStyleSheets, IntersectionObserver(+Entry), Web Animations
(element.animate), WebGL2, IndexedDB, ServiceWorkerContainer,
Notification, async clipboard, Permissions, OffscreenCanvas,
CanvasCapture, CookieStore, StorageManager, visualViewport,
composition events, execCommand, FontFace, Entries API, WakeLock,
geolocation, window.close().

### 5.1-b Explicitly NOT enabled (honest reasons, D-018)

- **WebGPU** (`dom_webgpu_enabled`): experimental wgpu backend; enabling
  risks the GPU-crash class we just exited (D-015). Needs owner-hardware
  validation first.
- **WebRTC** (`dom_webrtc_enabled`): connection formation incomplete.
- **WebXR session**: needs hardware.
- **MSE/MediaSource**: NOT implemented in this engine (no webidl) —
  this is the YouTube pipeline blocker, tracked R-17. Porting MSE is an
  upstream-scale project; tracked as upstream PR candidate.
- **requestIdleCallback**: not implemented. Candidate for a local patch
  (scheduler idle task) — queued 5.1-c.

## 5.2 int32/int64 audit

In progress: CI matrix reproduces with backtrace → exact fix → boundary
unit tests (i32::MIN/MAX ± 1 in every affected binding path). The audit
will grep bindings for `as i32` on non-WebIDL-long values.

## 5.3 Script execution gating — DONE (see 5.0 #1)

Regression coverage: identity + can_run_script guard; payload try/catch;
unit tests in brow-privacy assert the guards; CI matrix crash gate.

## 5.4 Fingerprint defense rewrite — DONE

- Origin read failure-tolerant (no more page-visible SecurityError).
- **Native-code spoofing**: patched methods now route
  `Function.prototype.toString` through an interceptor reporting
  `[native code]` (WeakSet of patched fns, interceptor included) — the
  classic detection vector is closed.
- Unit tests assert all three properties of the payload.

## 5.5 Site testing matrix (CI, not owner)

12 sites this iteration (user's named set + stable controls): example,
wikipedia, github, duckduckgo, bing, stackoverflow, mdn, reddit,
hackernews, bbc, w3schools, xkcd. Per site: peak process-tree RSS,
panic count (gate: 0), JS console error count, screenshot. Evidence →
`docs/evidence/phase5/` + artifact. Scales to the 50-site list by
extending the `sites=` line; per-site console budgets tuned after the
first real data.

## Honest notes

- "99% of sites" is an asymptote: the matrix gives measured coverage,
  not a promise. Every panic is a P0 fix; every must-API absence is a
  pref/impl decision documented here.
- MSE/YouTube, WebGPU-on-owner-GPU, and requestIdleCallback are the
  three known upstream-scale gaps after this iteration.
