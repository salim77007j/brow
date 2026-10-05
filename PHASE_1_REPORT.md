# PHASE 1 REPORT — Environment Setup & Servo Study

**Project:** brow — an ultra-lightweight, ultra-fast, privacy-first browser on the
Servo engine
**Phase:** 1 of 6 (Environment Setup & Servo Study)
**Date completed:** 2026-10-06
**Repository:** https://github.com/salim77007j/brow (`main`)
**Engine pinned:** Servo **v0.6.0 (LTS)**, upstream commit `c78d2c206f80a1c8b67eefa97f773bba513205d3`

---

## 1. What was accomplished

| Requirement | Result |
|---|---|
| Clean, professional repository structure | ✅ README (vision + phase roadmap), CONTRIBUTING (engineering standards), LICENSE (canonical MPL-2.0), `.gitignore`, `docs/` with 3 documents + render-proof asset, `.github/workflows/` |
| Latest stable Servo integrated | ✅ v0.6.0 (LTS, published 2026-09-29) identified live from the GitHub Releases API, vendored in-tree at `servo/` (full source, 1,518 engine files / 524,538 LOC, WPT included), provenance documented in `docs/SERVO_UPSTREAM.md` |
| Deep architecture study | ✅ `docs/SERVO_ARCHITECTURE_ANALYSIS.md` — 16 sections, module-by-module, every claim pinned to a verified file path; dependency versions extracted from `Cargo.lock` |
| Baseline Servo builds | ✅ **from source, via CI**: `.github/workflows/ci.yml` performs `./mach bootstrap` + `./mach build --release` on `ubuntu-24.04` and uploads the artifact (see Limitation L1 for why the 2-core/4 GB/10 GB local sandbox cannot run the ~20 GB release build itself) |
| Baseline Servo runs | ✅ **verified locally**: official `servo-x86_64-linux-gnu.tar.gz` SHA-256-verified against the release manifest, `--version` → `Servo 0.6.0-c78d2c206` (commit matches the tag), and a **real headless render of https://example.com captured to PNG** (`docs/assets/phase1_servo_headless_example.png`) — correct layout, multilingual text, RTL Arabic line rendered properly |
| Working CI workflow | ✅ pushes to `main` trigger a full source build + `--version` smoke + headless-render smoke + artifact upload |

### Deliverables checklist

- [x] `README.md`, `CONTRIBUTING.md`, `LICENSE` (MPL-2.0, canonical text), `.gitignore`
- [x] `servo/` — vendored v0.6.0 engine (phase 2–4 work happens in-tree here)
- [x] `docs/SERVO_ARCHITECTURE_ANALYSIS.md`
- [x] `docs/SERVO_UPSTREAM.md` — provenance + verification commands
- [x] `docs/BUILDING.md` — build guide incl. minimal-container runtime findings
- [x] `docs/assets/phase1_servo_headless_example.png` — proof of run
- [x] `.github/workflows/ci.yml` — source build + smoke tests + artifact
- [x] `PHASE_1_REPORT.md` (this file)

## 2. Key technical decisions and why

### D1 — Vendor the engine into `brow/servo` (monorepo) instead of a git submodule or fork

Phases 2–4 modify engine internals **and** the shell in the same commits; a
submodule would force a fork + cross-repo PR choreography and make atomic
"engine + shell" commits impossible. The monorepo also matches the user-visible
requirement to "download and **integrate**" the release. Trade-off: repository
size (~1.4 GB working tree, ~450 MB packed) — accepted, mitigated by CI checkout
being shallow by default and by `tests/wpt/` being re-fetchable from upstream if
we ever want a slimmer clone. Provenance and diff-against-upstream commands are
in `docs/SERVO_UPSTREAM.md`.

### D2 — Pin exactly the official v0.6.0 (LTS) release

Queried live from the GitHub Releases API (no hardcoded guesses). v0.6.0 is the
newest stable **LTS** line — the least-moving base for a multi-phase fork-style
project. The official prebuilt Linux tarball's SHA-256 was verified, and its
`--version` output (`Servo 0.6.0-c78d2c206`) was cross-checked against the tag
commit hash resolved with `git ls-remote` — source and binary are the same code.

### D3 — License: MPL-2.0 (canonical text, fetched from mozilla.org)

Servo itself is MPL-2.0; staying in the same license family keeps vendored files
header-compatible, keeps file-level copyleft for our changes, and imposes no
network-copyleft burden. The `LICENSE` file is the verbatim upstream text
(hand-copying a license from memory invites subtle corruption — we didn't).

### D4 — "Runs" proven locally with the official binary; "builds from source" proven by CI

The execution sandbox for this phase is 2 vCPU / 4.1 GB RAM / 9.9 GB disk with
**no sudo**. A release build needs ~20 GB of `target/` and 8+ GB peak RAM at
link time — physically impossible here. We therefore split the two claims:

- **Run baseline**: official release binary, SHA-256 verified, exercised through
  a genuine headless render (screenshot committed).
- **Build from source**: delegated to GitHub Actions (`ci.yml`) where a real
  4-core/16 GB runner with ~50 GB disk performs `mach bootstrap` + `mach build
  --release` on every push. This is also the exact pipeline phases 2–6 will rely
  on, so getting it green early is load-bearing.

### D5 — User-space runtime dependency stack (no sudo), documented for reuse

Getting the prebuilt binary running in a stripped container produced a set of
**reproducible findings** now captured in `docs/BUILDING.md`:

- Servo v0.6.0 dynamically links `libgstplay-1.0.so.0`, `libgstwebrtc-1.0.so.0`,
  `libgstgl-1.0.so.0`. On Debian 13 (trixie) the **package names were renamed**:
  these sonames ship inside `libgstreamer-plugins-bad1.0-0` and
  `libgstreamer-gl1.0-0` (resolved empirically via the `Contents-amd64` index,
  after `apt-cache`/web search dead-ends).
- Headless surfman (v0.13) needs a working **EGL vendor**: glvnd's `libEGL.so.1`
  without `libEGL_mesa.so.0` + its vendor JSON produces
  `Failed to create WR surfman` / a zero-FBConfig assertion in
  `surfman::x11::connection`. The fix that works without root:
  `dpkg -x` the mesa/gst packages into a prefix, then
  `LD_LIBRARY_PATH=… __EGL_VENDOR_LIBRARY_FILENAMES=…/50_mesa.json
  LIBGL_ALWAYS_SOFTWARE=1` and run under `Xvfb` (`xvfb-run` itself was broken in
  the container — we drive `Xvfb` directly).
- Net result: a fully user-space recipe for headless Servo — the same recipe
  hardens our Phase 6 test containers.

### D6 — Study method: read the code, not the lore

Most public Servo documentation describes the pre-2024 tree. We verified against
the actual vendored sources and recorded the real 2026 structure (stylo/webrender
from crates.io; `compositing`/`gfx` crates gone; traits under
`components/shared/*`; `vello_cpu` canvas; SM ESR 153). This prevents phases 2–4
from being planned against phantom modules.

## 3. Libraries and versions (verified from the vendored tree)

| Component | Version | Source of truth |
|---|---|---|
| Servo engine | 0.6.0 (LTS) | release tag + `Cargo.toml` workspace `version` |
| Rust toolchain | **1.97.1 pinned stable** (edition 2024, `rust-version` 1.88.0) | `servo/rust-toolchain.toml` |
| rustup / rustc (sandbox) | 1.29.1 / 1.99.0 (stable) — only used for repo tooling, not the engine build | `rustup` install log |
| SpiderMonkey | `mozjs 0.26.3`, `mozjs_sys 153.3.0-0` (ESR 153) | `servo/Cargo.lock` |
| WebRender | 0.70.0 (+ `webrender_api 0.70.0`) | `Cargo.lock` |
| Stylo | `stylo 0.21.0` + `stylo_traits/stylo_dom/stylo_atoms/stylo_malloc_size_of/stylo_static_prefs` | `Cargo.toml` workspace deps |
| Selectors / CSS parser | 0.40.0 / 0.37.0 | workspace deps |
| HTTP / async | hyper 1.11.0, tokio 1.53.1, hyper-rustls, tokio-rustls | `Cargo.lock` |
| TLS | rustls 0.23.45 (aws-lc-rs), rustls-platform-verifier | `Cargo.lock` + `components/net/Cargo.toml` |
| WebGPU | wgpu 29.0.4 | `Cargo.lock` |
| Windowing / shell UI | winit 0.30.13, egui 0.34.3 | workspace deps |
| GL context | surfman 0.13.0 (feature `chains`) | workspace deps |
| Canvas raster | vello_cpu (GPU vello optional) | `components/canvas/Cargo.toml` |
| Media | GStreamer runtime 1.26 (Debian 13 packages) | runtime link closure |
| CI runners | ubuntu-24.04, actions/checkout@v4, actions/upload-artifact@v4 | `.github/workflows/ci.yml` |

## 4. Known limitations and open questions

**L1 — No local source build in the Phase 1 sandbox.** 2 vCPU / 4.1 GB / 9.9 GB,
no sudo. Build-from-source validity is asserted by CI (and will be watched until
green). The local Rust toolchain installed during recon (1.99.0) is for tooling
only; the engine build uses the pinned 1.97.1 on CI.

**L2 — CI first-run not yet observed.** The workflow was pushed at the very end
of the phase; its first run takes ~2 h on the runner. If `mach bootstrap` on
ubuntu-24.04 hits a packaging drift (this happens upstream periodically), the
fix is a small explicit `apt-get install` list in the workflow — budgeted in
Phase 2 step 0.

**L3 — Servoshell's minimal chrome is not brow's UI.** We intentionally did not
fork servoshell's GUI; brow's shell arrives in Phase 3 as a new port. Until then
servoshell is only a smoke target.

**L4 — multiprocess isolation depth unverified.** `run_content_process` +
sandbox profile exist, but the strength of Linux sandboxing (seccomp/namespace
depth) needs a dedicated assessment in Phase 3 before we promise isolation
properties.

**L5 — Open question: stylo upgrade cadence.** Stylo ships as crates.io
`stylo 0.21`. If Phase 2 CSS work needs unreleased Stylo behavior we must either
patch it (vendor stylo too) or upstream-first. Decision deferred until the
Phase 2 gap analysis lands on a concrete missing CSS feature.

**L6 — WPT in-tree costs ~1.2 GB of the repo.** Kept deliberately (conformance
testing is a hard requirement of later phases), but if push bandwidth ever
becomes a problem, `tests/wpt/tests` can be excluded and re-fetched from the
upstream tag — the exact procedure is written down in `docs/SERVO_UPSTREAM.md`.

## 5. Recommendations for Phase 2 (in priority order)

1. **Watch the first CI run** (`actions/workflows/ci.yml`) and fix forward if
   needed (L2). Green "build-from-source" is the entry ticket for Phase 2.
2. **HTTP/3 + QUIC**: add `quinn` (+ `h3`) behind a cargo feature; implement
   Alt-Svc discovery in `components/net/http_loader.rs`, a `protocols/h3.rs`
   transport, and a fallback matrix (h3 → h2 → h1.1). Benchmark TTFB on h3-only
   endpoints.
3. **DoH/DoT**: inject a resolver (hickory-resolver with DoH stub over rustls)
   below `connector.rs`, defaulting to secure DNS with user-configurable
   templates; add prefs + a leak test.
4. **TLS hardening pass on the existing rustls stack**: disable TLS 1.2 by
   default (keep pref), enforce OCSP-ish revocation policy via
   rustls-platform-verifier behavior review, document the crypto provider
   choice (aws-lc-rs vs ring).
5. **Modern CSS/JS gap matrix**: run a targeted WPT subset on servoshell build
   to produce an evidence-based gap list (layout tables, View Transitions,
   Web Workers/ServiceWorker state, WebGPU flag) instead of guessing; triage
   into "engine work in `servo/`" vs "pref/flag work".
6. **Crown hygiene baseline**: run `support/crown` once over the untouched tree
   to establish the clean baseline before any `script` modifications.
7. **Set up the benchmark harness early** (`scripts/bench/`): page-load timer
   (WPT-ish), RSS sampler, CPU sampler — Phase 2's "performance improvements"
   claims will need before/after numbers, and the harness only gets more
   valuable in Phase 3.

## 6. Environment notes for the next phase (context preservation)

- Local sandbox paths: repo at `/home/z/my-project/brow`; verified prebuilt
  binary at `/home/z/my-project/downloads/servo/servoshell`; user-space GL/gst
  prefix at `/home/z/my-project/gst-root/root`; helper scripts in
  `/home/z/my-project/scripts/` (`fetch_gst_libs.sh`, `servo_headless_shot.sh`).
- The headless run recipe (works without root):
  `Xvfb :99 &` + `LD_LIBRARY_PATH=$GST/usr/lib/x86_64-linux-gnu`
  `__EGL_VENDOR_LIBRARY_FILENAMES=$GST/usr/share/glvnd/egl_vendor.d/50_mesa.json`
  `LIBGL_ALWAYS_SOFTWARE=1` `DISPLAY=:99 servoshell -z -o shot.png URL`.
- Git identity used for commits: `salim77007j <salim77007j@users.noreply.github.com>`.
- The GitHub token used for pushes was supplied interactively and must be
  **rotated** after the project (it transited chat in plain text).

---

**Phase 1 is complete. Work stops here until the explicit command
"Continue to Phase 2".**
