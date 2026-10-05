# Phase 2 — Web API Gap Matrix (evidence-based)

**Scope:** brow engine = vendored Servo v0.6.0 (LTS), commit `c78d2c206`.
**Method:** every claim below was verified against the vendored source tree
(file paths cited) — not against documentation or prior knowledge. The purpose
is to give Phases 3–5 an honest map of what the engine can and cannot do, and
to triage each gap into *engine work*, *pref/flag work*, or *out of scope*.

Legend: ✅ implemented · 🟡 partially implemented / flagged · ❌ absent

| API | Status | Evidence | Triage |
|---|---|---|---|
| WebAssembly | ❌ | No `wasmi`/`wasmtime` dependency in `components/script/Cargo.toml`; no `WebAssembly` namespace in `components/script/dom/` | Engine work (large). Phase 2 does **not** attempt it; revisit after Phase 3 |
| WebGPU | 🟡 | `wgpu-core`/`wgpu-types` optional deps wired in `components/script/Cargo.toml` (feature-gated); DOM surface present: `dom/gpucanvascontext.rs`, `dom/webgpu/` | Pref/flag work — verify the `webgpu` feature builds on CI; off by default in servoshell |
| Service Workers | 🟡 | `dom/serviceworker/` (~4.7k LOC incl. `serviceworkercontainer.rs`), `ScopeThings` plumbing exists; fetch hook in `components/net/http_loader.rs` still a `TODO: Substep 1` (`http_fetch` step 3) | Engine work — registration surfaces exist, interception does not |
| WebRTC | 🟡 | `dom/webrtc/rtcpeerconnection.rs` + `dom/webrtc/` present; data-channel oriented; no media stack parity with Chrome | Out of scope for brow's core mission |
| View Transitions | ❌/🟡 | Only name-level references in `dom/document/document.rs`; no transition pipeline | Engine work, post-Phase 3 |
| OffscreenCanvas | ❌ | No `offscreencanvas*` file in `components/script/dom/` | Engine work |
| WebTransport | ❌ | Absent. Note: brow's Phase 2 QUIC stack (`support/brow-net-core`) gives the engine a transport primitive this could later build on | Engine work (later phase) |
| WebCodecs | ❌ | Absent; media decodes via GStreamer pipeline (`components/media/`) | Out of scope |
| Push API | ❌ | Absent | Out of scope |
| Credential Management | 🟡 | `dom/credential/` present (1 file) | Evaluate in Phase 4 (privacy context) |
| WebShare / Idle Detection | ❌ | Absent | Out of scope (also privacy-relevant: good that they're absent) |
| HTTP/2 | ✅ | `connector.rs::create_http_client` — `enable_http2()` (hyper) | — |
| HTTP/3 (brow, Phase 2) | ✅ | `support/brow-net-core/src/h3.rs` + `components/net/h3_loader.rs` (opportunistic, Alt-Svc driven) | brow addition |
| DoH (brow, Phase 2) | ✅ | `support/brow-net-core/src/dns/` + `connector.rs::create_dns_resolver` | brow addition |
| WebSockets | ✅ | `components/net/websocket_loader.rs` (h1 ALPN pinned) | — |
| zstd/brotli/gzip decode | ✅ | `components/net/decoder.rs` + `async-compression` features in `components/net/Cargo.toml` | — |

## Phase 2 additions to the net stack (this phase's deliverable)

* `support/brow-net-core` — HTTP/3 client (quinn 0.11 + h3 0.0.8), RFC 8484
  DoH resolver with bootstrap addresses, RFC 7838 Alt-Svc cache,
  COOP/COEP/CORP policy engine, TLS version policy. 35 tests incl. two full
  loopback integration tests.
* `components/net/h3_loader.rs` — eligibility + fallback matrix wiring.
* `components/net/connector.rs` — DoH-backed resolver for all classic fetches;
  TLS min-version pref.
* `components/net/decoder.rs` — error-type generalization (`DecoderBodyError`)
  so HTTP/3 and classic bodies decompress through one pipeline.
* Prefs: `network.dns-over-https.enabled`, `network.dns-over-https.templates`,
  `network.dns.bootstrap-addresses`, `network.http3.enabled`,
  `network.tls.min-version` (see `components/config/prefs.rs`).

## Notes for later phases

1. The Service Worker fetch interception TODO in `http_fetch` is the single
   biggest functional gap for modern web apps (Phase 3+ decision).
2. WebGPU builds behind a feature; enabling it by default is a Phase 3
   perf/stability trade-off to be measured, not assumed.
3. Phase 4 (privacy) will find the *absence* of Idle Detection / WebShare /
   Battery-style surface area helpful: fewer fingerprintable APIs exist at all.
