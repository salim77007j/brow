# Servo v0.6.0 Architecture Analysis

**Status:** Phase 1 deliverable · **Analyzed tree:** `servo/` (vendored upstream
**v0.6.0 LTS**, commit `c78d2c206f80a1c8b67eefa97f773bba513205d3`, published
2026-09-29) · **Method:** direct source inspection of the vendored release, plus a
verified run of the official release binary (headless render of `example.com`,
see `docs/assets/phase1_servo_headless_example.png`).

This document maps how the 2026 Servo engine actually works, where its strengths
and weaknesses are, and exactly which extension points **brow** will build on in
phases 2–4. Every structural claim below was verified against the vendored tree
(file paths are given so future phases can jump straight to the code).

---

## 1. Executive summary

Servo v0.6.0 is a substantially re-organized engine compared to the 2023-era code
base that most public literature describes. The defining structural changes we
verified:

1. **Stylo, WebRender and the shared trait crates are now versioned crates.io
   dependencies** (`stylo 0.21`, `webrender 0.70`, `servo-*-traits =0.6.0`), not
   in-tree modules. The engine is assembled like a product, from published parts.
2. **The old `components/compositing` and `components/gfx` crates are gone.**
   Rendering is driven by the embedder through the `paint` crate
   (`components/shared/paint`) on top of `surfman 0.13`; fonts live in
   `components/fonts`; the compositor role has been absorbed by servoshell +
   WebRender's render notifier loop.
3. **A first-class embedder API exists**: `components/servo` exposes `Servo`,
   `ServoBuilder`, `WebView`, `WebViewBuilder`, `WebViewDelegate`,
   `ServoDelegate` and — critically for brow — `WebResourceLoad::intercept()`,
   a clean network-resource interception hook.
4. **TLS is already rustls** (`rustls 0.23` + `aws-lc-rs`, verified via
   `hyper-rustls`/`tokio-rustls`/`rustls-platform-verifier` in
   `components/net/Cargo.toml`) — the Phase 2 task "upgrade TLS to rustls" is
   already satisfied upstream; our work shifts to HTTP/3, DoH/DoT and hardening.
5. **WPT is in-tree** (`tests/wpt/`, ~1.2 GB, no git submodules remain), so
   conformance testing works from a plain clone of brow.
6. **Multiprocess mode exists** (`run_content_process()` +
   `content_process_sandbox_profile()` in `components/servo/servo.rs:1320,1399`)
   but is opt-in; the default remains one-process/many-threads.

Scale: `components/` alone is **1,518 Rust files / 524,538 LOC**; the two
heaviest subsystems are `script` (898 files, 296,226 LOC, including generated
bindings) and `layout` (67 files, 47,761 LOC).

---

## 2. Upstream snapshot

| Field | Value |
|---|---|
| Release | Servo **v0.6.0 (LTS)**, latest stable at project start |
| Published | 2026-09-29 |
| Tag commit | `c78d2c206f80a1c8b67eefa97f773bba513205d3` |
| Rust toolchain | **pinned stable 1.97.1** (`servo/rust-toolchain.toml`); edition 2024; `rust-version = 1.88.0` |
| SpiderMonkey | `mozjs 0.26.3` / `mozjs_sys 153.3.0-0` → **SpiderMonkey ESR 153** |
| WebRender | `webrender 0.70.0` + `webrender_api 0.70.0` (crates.io) |
| Stylo | `stylo 0.21.0`, `stylo_traits`, `stylo_dom`, `stylo_atoms`, `stylo_malloc_size_of`, `stylo_static_prefs` (crates.io) |
| HTTP stack | `hyper 1.11.0`, `tokio 1.53.1`, `rustls 0.23.45`, `hyper-rustls`, `tokio-rustls` |
| WebGPU | `wgpu 29.0.4` (feature-gated) |
| Windowing / UI | `winit 0.30.13`, `egui 0.34.3` |
| GL context | `surfman 0.13.0` (features `chains`) |
| Canvas | `vello_cpu` (CPU raster) with optional GPU `vello` |
| Workspace default build | `ports/servoshell` (default workspace member) |

Binary identity check: the official `servo-x86_64-linux-gnu.tar.gz` (SHA-256
`ad951ede…a60d0`, verified) reports `Version: Servo 0.6.0-c78d2c206` — the
vendored source and the released binary are the same code.

### Top-level layout

| Directory | Role |
|---|---|
| `components/` | The engine crates (see §3) |
| `ports/servoshell/` | The reference browser shell (winit + egui + CLI) |
| `support/` | `crown` (GC-safety lint), android/openharmony/windows/macos glue |
| `ffi/capi` | C API for embedding |
| `tests/` | WPT (in-tree), unit, capi, blink perf, power tests |
| `resources/` | Runtime resources: UA stylesheets, certs, platform metadata |
| `python/` + `mach` | Build/test orchestration (mach core + commands) |
| `docs/` | Upstream design docs |
| `.cargo/config.toml` | Per-target linkers (Android NDK, MSVC) — no vendored sources |

---

## 3. Crate and module map (`components/`)

### 3.1 Core engine crates

| Crate | LOC* | Role (verified) |
|---|---:|---|
| `script` | 296,226 | DOM + JS. Subsystems: `dom/` (WebIDL-generated + hand-written DOM), `engine/` (SpiderMonkey glue: `handle.rs`, `init.rs`, `mod.rs`), `layout_dom/` (layout-side DOM facade), `fetch/`, `css/`, `tasks/` (task queues), `event_loop/`, `realms.rs`, `navigation.rs`, `runtime/`, `xpath.rs` |
| `layout` | 47,761 | Layout 2020: `fragment_tree/`, `flow/`, `flexbox/`, `display_list/`, `construct_modern.rs`, `layout_impl.rs`, tables, positioning |
| `net` | 23,415 | Fetch/HTTP implementation (§6) |
| `servo` | 10,037 | **Embedder API**: `Servo`/`ServoBuilder` (`servo.rs:897,1429`), `WebView`/`WebViewBuilder` (`webview.rs:84,1093`), delegates, `NetworkManager`, `SiteDataManager`, `UserContentManager`, `JavascriptEvaluator`, resource responders |
| `servoshell` (ports) | 12,282 | Desktop/Android shell: `app.rs`, `gui.rs` (egui), `headed_window.rs` / `headless_window.rs`, `event_loop.rs`, `running_app_state.rs`, `cli.rs`, `webdriver.rs` |
| `constellation` | 8,766 | Session/pipeline orchestrator (§4) |
| `canvas` | 2,931 | 2D canvas over `vello_cpu` (optional GPU `vello`) |
| `webgpu` | 2,510 | WebGPU adapter over `wgpu 29` |
| `fonts`, `paint` (shared), `pixels`, `media`, `webgl`, `webxr`, `bluetooth`, `webdriver_server`, `devtools`, `storage`, `url`, `timers`, `metrics`, `config`, `profile`, `wakelock`, `webvtt`, `xpath`, `shared/*` | — | Support subsystems |

\* LOC counted on the vendored tree, `.rs` files only.

### 3.2 The `components/shared/*` trait layer

`embedder_traits` → `servo-embedder-traits` (`components/shared/embedder`),
`net_traits` → `components/shared/net`, `script_traits` →
`components/shared/script`, plus `shared/{base,canvas,constellation,devtools,
fonts,layout,paint,profile,storage,webgpu,webxr,bluetooth,background_hang_
monitor}`. These are published to crates.io pinned `=0.6.0`. They define the
message protocols between embedder, constellation, script, layout and net
(`RefreshDriver`, `RenderingContext`, fetch params, pipeline events…). brow will
consume these as the stable-ish contract between our shell and the engine.

### 3.3 What disappeared since the "classic" Servo papers

| Old module | 2026 state |
|---|---|
| `components/style` (Stylo in-tree) | **Extracted** to crates.io `stylo 0.21` |
| `components/compositing` | **Removed**; composition logic lives with the embedder + WebRender |
| `components/gfx` | **Removed**; fonts → `components/fonts`, text shaping done against stylo/font crates |
| `components/net_traits` etc. | Moved to `components/shared/*` as published crates |
| `layout_thread_2020` | Layout entry is `components/layout/layout_impl.rs`; layout work is driven from the script pipeline (see §4) |
| 2D canvas on raqote | Replaced by **Vello** (`vello_cpu` default, `vello` GPU variant) |

---

## 4. Process and threading architecture

Verified structure:

- **Constellation** (`components/constellation/constellation.rs`) is the master
  orchestrator: it owns browsing contexts (pipelines), session history, navigation
  and event routing, and runs on its own thread
  (`thread::Builder` spawn at `constellation.rs:610`). Every embedder-facing
  operation funnels through constellation messages defined in
  `components/shared/constellation`.
- **Pipelines**: one script execution context per frame/iframe. The `script`
  crate runs the DOM event loop (task queues in `script/tasks/`, event loop
  plumbing in `script/event_loop/`), owns the SpiderMonkey runtime per pipeline,
  and invokes layout via the layout traits (`components/shared/layout`,
  implemented by `components/layout/layout_impl.rs`).
- **Layout 2020** is a pure-ish computation over the DOM facade
  (`script/layout_dom/`), producing `FragmentTree` → display lists
  (`layout/display_list/`). Parallelism is expressed through the rayon-style
  work queues inside layout's flow/flexbox traversal, rather than a separate
  long-lived "layout thread" model of pre-2020 Servo.
- **Rendering**: display lists go to WebRender; the embedder (servoshell) owns
  the WebRender instance, receives `RenderNotifier` callbacks and presents via a
  `RenderingContext` (`components/shared/paint/rendering_context.rs` — trait at
  line 35, `SurfmanRenderingContext` at 98, software path
  `SoftwareRenderingContext::new` at 296 which calls `surfman::Connection::new`).
  The refresh driver abstraction (`embedder_traits::RefreshDriver`) lets the
  embedder drive vsync/timer cadence — an important lever for brow's Phase 3
  CPU throttling of background tabs.
- **Multiprocess**: `servo.rs:1320 run_content_process(token)` +
  `ChildSandbox::new(content_process_sandbox_profile())` (`servo.rs:1399`) —
  opt-in (`--multiprocess`), with a platform sandbox profile.servoshell prefs
  expose the switch. This is the substrate for brow's Phase 3 isolation work.
- **Background hang monitor** (`components/shared/background_hang_monitor`,
  `components/background_hang_monitor`) watches long tasks — useful telemetry
  for Phase 3 CPU budgets.

---

## 5. Rendering pipeline (end-to-end, verified)

```
servoshell (winit event loop, app.rs/event_loop.rs)
   │  ServoBuilder::build() → Servo  (components/servo/servo.rs:1429,897)
   │  WebViewBuilder → WebView       (webview.rs:1093,84)
   ▼
Constellation thread ── owns pipelines, history, navigation
   ▼
Script pipeline (per browsing context)
   SpiderMonkey event loop (mozjs 0.26 / SM ESR 153)
   DOM events → JS → DOM mutations
   ▼
Style recalc: stylo 0.21 (selectors 0.40, cssparser 0.37)
   ▼
Layout 2020 (components/layout): FragmentTree → DisplayList
   ▼
servoshell: WebRender 0.70 transaction (tiles, picture caching, GPU)
   ▼
RenderingContext (shared/paint): surfman 0.13 GL context → present
   (headed: winit window surface / headless: SoftwareRenderingContext)
```

Two verified headless details matter for CI and Phase 3:
`ports/servoshell/desktop/headless_window.rs:50` constructs a
`SoftwareRenderingContext` (pure software GL via surfman's software adapter),
and `-o <file>` writes `read_to_image()` output. Our Phase 1 smoke test
exercised exactly this path under Xvfb + llvmpipe.

## 6. Networking stack (`components/net`, 23.4k LOC)

Verified file map: `http_loader.rs` (HTTP via hyper 1.11), `connector.rs`
(connection/TLS: `hyper-rustls` + `tokio-rustls`, cert verification via
`rustls-platform-verifier`), `async_runtime.rs` (tokio runtime), `fetch/`
(fetch algorithm), `protocols/` (custom scheme handlers),
`http_cache.rs` + `disk_cache.rs` (HTTP + disk caching), `cookie.rs` +
`cookie_storage.rs`, `hsts.rs`, `hosts.rs`, `image_cache.rs`,
`filemanager_thread.rs`, `devtools.rs`, `embedder.rs` (embedder-visible
network manager hooks), `decoder.rs` (content decoding).

What is there: HTTP/1.1 + HTTP/2 (hyper), TLS 1.2/1.3 via rustls with
`aws-lc-rs` primitives, platform-rooted certificate verification, HSTS,
cookie jar, HTTP cache, WebSocket (via `async-tungstenite` with
`tokio-rustls-webpki-roots` feature — workspace `Cargo.toml`), multipart/file
handling, devtools network notifications.

Gaps we confirmed (Phase 2 targets):

1. **No HTTP/3 / QUIC** — no `quinn`/`s2n-quic`/`h3` anywhere in the tree.
2. **No DNS-over-HTTPS / DNS-over-TLS** — resolution falls through to the
   system resolver (hyper-util's default); no `hickory` dependency.
3. Cookie partitioning (CHIPS), tracking-parameter stripping, beacon
   filtering: not implemented (Phase 4).
4. The embedder interception surface is `NetworkManager` +
   `WebResourceLoad::intercept()` (embedder-side), which is request/response
   level — a network-level (pre-connect) filter will need a small in-net
   extension (Phase 4 design).

## 7. JavaScript engine integration

- Bindings crate `mozjs 0.26.3` wraps `mozjs_sys 153.3.0-0` — **SpiderMonkey
  ESR 153** (this also gives us WebAssembly support "for free" via SM's wasm).
- Glue lives in `components/script/engine/` (`init.rs`, `handle.rs`) and the
  generated bindings crate `script_bindings`; DOM interfaces are code-generated
  from WebIDL at build time (`components/script/build.rs`, `dom/bindings/`).
- GC safety is enforced at compile time by **crown** (`support/crown`) — a
  custom rustc driver/lint that enforces rooting discipline. Any brow code that
  touches `Dom`/`DomRoot` types must stay crown-clean; the CI toolchain pin
  includes `rustc-dev` + `llvm-tools` precisely for this.

## 8. Stylo (CSS engine)

Since Stylo ships as crates.io `stylo 0.21` (+ `selectors 0.40`,
`cssparser 0.37`, `stylo_static_prefs`), Servo inherits Firefox-grade CSS
parsing/selectors (including `:has()`, cascade layers, container queries at the
parser/computed-value level). brow's Phase 2 CSS work therefore splits into:
(a) layout-level support in `components/layout` for anything Stylo computes but
Layout 2020 can't yet place (tables are the classic weak spot), and (b) engine
config/prefs. Any deeper CSS work means bumping or patching the stylo crate —
a deliberate, documented operation per our vendoring policy.

## 9. Graphics, media, platform

| Area | 2026 state (verified) |
|---|---|
| GPU renderer | WebRender 0.70 (crates.io, servo-maintained) |
| GL context | surfman 0.13 (`chains` swap-chain feature); headed + headless + software paths |
| WebGL | `components/webgl` on surfman |
| WebGPU | `components/webgpu` + `wgpu 29.0.4`, feature-gated |
| 2D canvas | Vello (`vello_cpu` default; GPU `vello` optional feature `canvas` crate line 18) |
| Fonts | `components/fonts` + `components/shared/fonts` |
| Media | GStreamer (`components/media`, `gstreamer_plugins.rs` in `components/servo`); runtime sonames `libgstplay-1.0`, `libgstwebrtc-1.0`, `libgstgl-1.0` — packaged on Debian 13 inside `libgstreamer-plugins-bad1.0-0` / `libgstreamer-gl1.0-0` |
| Windowing | winit 0.30; egui 0.34 for shell chrome/widgets |
| XR / Bluetooth | `components/webxr` (servo-webxr), `btleplug` |

## 10. servoshell and the embedder API (brow's primary surface)

`components/servo` is the productized embedder API:

- `ServoBuilder::build()` → `Servo` (`servo.rs:1429,897,1448`) — engine handle
  + event pump.
- `WebViewBuilder` → `WebView` (`webview.rs:1093,84`) — one web view per tab,
  with URL/navigation/zoom/input APIs.
- **Delegates** (the embedder fills in browser policy):
  - `WebViewDelegate` (`webview_delegate.rs:918`) — navigation, alerts, file
    picker, permission prompts (allow/deny handles), authentication prompts,
    bluetooth device selection, **`WebResourceLoad::intercept()`**
    (`webview_delegate.rs:269`) — synchronous embedder interception of
    subresource loads, and `WebResourceRequest/Response` streaming APIs.
  - `ServoDelegate` (`servo_delegate.rs:21`) — errors, devtools server events,
    console messages, notifications, **`load_web_resource()`** (top-level
    resource loads).
- `NetworkManager`, `SiteDataManager` (cookies/site data clearing),
  `UserContentManager` (user scripts/styles) — directly reusable for brow's
  privacy engine and Phase 3 UI settings.
- servoshell itself (`ports/servoshell`): `app.rs` owns `RunningAppState`;
  `gui.rs` draws minimal chrome with egui 0.34; `cli.rs` parses flags
  (including `--headless`, `-o`, `--pref`, `--multiprocess`, `--webdriver`);
  `running_app_state.rs` coordinates webviews; `webdriver.rs` hosts the
  WebDriver server.

**brow strategy (Phase 3):** we keep `components/servo` as the engine boundary
and build our own shell (Slint- or egui-based decision deferred to Phase 3's
design step) as a new `ports/brow-shell` crate consuming the WebView API —
never patching servoshell's GUI for browser-level features. servoshell remains
our CI smoke target.

## 11. Testing infrastructure

- **WPT in-tree**: `tests/wpt/` (web-platform-tests suite + `meta/`
  expectations + `mozilla/` servo-specific tests). `./mach test-wpt` runs it;
  no submodule init needed.
- **WebDriver**: `components/webdriver_server` + servoshell `--webdriver`
  (automation entry point for Phase 6 comparisons).
- **Devtools**: `components/devtools` + net-side `devtools.rs`.
- **Unit tests**: workspace members `tests/unit/*`; crate-level
  `#[cfg(test)]` everywhere.
- **crown** (GC linter) and **tidy** (style/license lints) gate correctness
  and hygiene.

## 12. Strengths (2026 assessment)

1. **Memory-safe core across the board** — engine, CSS, layout, net, canvas,
   GPU stack are all Rust; the only C++ is SpiderMonkey, sandboxed behind
   mozjs + crown-checked bindings.
2. **Modern dependency hygiene**: edition 2024, pinned stable Rust 1.97.1,
   rustls+aws-lc-rs, hyper 1.x, wgpu 29, winit 0.30 — nothing abandoned.
3. **A real embedder API** with delegates and resource interception — the
   difference between "fork a browser" and "build a browser on an engine".
4. **Parallel Stylo style engine + Layout 2020** with a clean fragment-tree
   architecture that is far easier to reason about than the legacy layout.
5. **In-tree WPT** (huge for our conformance goals) and first-class mach
   tooling; multiprocess + sandbox profile already exist.
6. **Official releases with LTS channel** (v0.6.0 LTS) — a stable base to pin.

## 13. Weaknesses / gaps brow must address

| # | Gap (verified) | Impact | Phase |
|---|---|---|---|
| G1 | No HTTP/3, no QUIC, no DoH/DoT; DNS via system resolver | Privacy (DNS leaks), performance | 2 |
| G2 | No cookie partitioning / tracking-param stripping / beacon blocking | Privacy | 4 |
| G3 | Resource interception is embedder-level; no network-level filter engine (blocklists, Aho-Corasick) | Ads/tracking | 4 |
| G4 | No fingerprinting countermeasures (canvas/WebGL/audio/font APIs are vanilla) | Privacy | 4 |
| G5 | One process by default; multiprocess opt-in and not tab-isolated | RAM/robustness | 3 |
| G6 | No tab hibernation/discarding; no memory budgets; no idle throttling policy in the embedder | <100 MB/tab goal | 3 |
| G7 | Layout 2020 coverage (tables, some float/inline edge cases) lags Blink/Gecko | Compat | 2 (targeted) |
| G8 | WebGPU/WebXR/ServiceWorker are partial and feature-gated | Advanced sites | 2 (assess) |
| G9 | No media DRM; GStreamer-dependent media stack adds runtime deps | Video sites | 6 (document) |
| G10 | No UI beyond minimal servoshell chrome; no tabs/history/downloads UI | Product | 3 |

## 14. Extension points brow will use

| Phase | Mechanism (exact, verified) |
|---|---|
| 2 | New `components/net/protocols/h3.rs` + `quinn` transport alongside `http_loader.rs`; DoH/DoT resolver crate injected at `connector.rs` level; hardening in `fetch/` and `connector.rs`; feature flags in workspace `Cargo.toml` |
| 3 | New `ports/brow-shell` on `Servo`/`WebView`/delegates; `RefreshDriver` for idle throttling; `--multiprocess` + `content_process_sandbox_profile` for isolation; `RunningAppState`-style tab registry; prefs via `components/config` |
| 4 | `WebResourceLoad::intercept()` + `NetworkManager` for request filtering; `SiteDataManager` for cookie partitioning policy; `UserContentManager` for cosmetic-filter CSS/JS injection; JS-API shims via WebIDL-facing wrappers in `script` |
| 5 | servoshell/packaging precedent + `.cargo/config.toml` platform linkers; `mach` build variants for LTO/PGO |

## 15. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Upstream trait crates pin `=0.6.0` — engine-internal changes may require coordinated bumps | Vendor policy (SERVO_UPSTREAM.md): document every touched file; prefer additive crates (`brow-*`) over modifications |
| crown GC linter may reject naive DOM-touching code | Phase 2 onboarding step: run crown locally before committing script changes |
| stylo/webrender crate upgrades can shift behavior | Pin + changelog review + WPT diff before/after any bump |
| Local CI hardware cannot build the engine (2-core/4 GB sandbox) | GitHub Actions is the source of truth for build validity; local dev uses prebuilt release binary for smoke tests |
| WPT runtime is heavy | Run targeted subsets (`--include` paths) per feature in CI; full runs in scheduled jobs |

## 16. Quick reference — key source files

| Topic | Path |
|---|---|
| Engine handle / content process / sandbox | `components/servo/servo.rs` |
| WebView API | `components/servo/webview.rs`, `webview_delegate.rs`, `servo_delegate.rs` |
| Resource/site data/user content managers | `components/servo/{network_manager,site_data_manager,user_content_manager}.rs` |
| Constellation | `components/constellation/constellation.rs` |
| Rendering context / headless | `components/shared/paint/rendering_context.rs`, `ports/servoshell/desktop/{headed,headless}_window.rs` |
| Event loop integration | `ports/servoshell/{app,event_loop,running_app_state}.rs` |
| HTTP/TLS/DNS/cache | `components/net/{http_loader,connector,http_cache,hsts,cookie_storage}.rs` |
| SM glue | `components/script/engine/*`, `components/script_bindings` |
| Layout entry / display lists | `components/layout/{layout_impl.rs,display_list/}` |
| Prefs | `components/config/prefs.rs`, `resources/package-prefs.json` |
| GC linter | `support/crown` |
| Toolchain pin | `rust-toolchain.toml` |
