# Phase 2 — Content-Process Sandbox Assessment

**Question (carried from Phase 1, limitation L4):** how deep is brow's
multiprocess isolation on our shipping targets, and what must Phase 3 promise
(or fix) before we talk about security publicly?

**Method:** read of the actual code paths — `components/constellation/sandboxing.rs`
(298 lines), `components/servo/lib.rs::run_content_process`, and the
`gaol` dependency (`Cargo.lock` — `gaol 0.0.0` workspace entry resolved from
upstream). No claims below come from documentation.

## Findings

### 1. Multiprocess is opt-in per architecture, and split by target

`components/constellation/sandboxing.rs` has **three** `spawn_multiprocess`
implementations selected by `cfg`:

| Target | Implementation | Sandboxed? |
|---|---|---|
| macOS | gaol `Sandbox` (profile at line 56) | ✅ |
| x86_64 Linux (our CI/shipping target) | gaol `Sandbox` (line 190) | ✅ |
| Windows / iOS / Android / OHOS / **ARM & AArch64 & RISC-V Linux** | `process::Command` spawn, comment literally reads *"Failed to start unsandboxed child process!"* (line 178) | ❌ |

**Impact for brow:** on `x86_64-unknown-linux-gnu` (Phase 5's AppImage/deb/rpm
target) content processes are sandboxed. The **AArch64 Linux path is
unsandboxed** — relevant the day we ship ARM builds, and flagged now so Phase 5
does not silently inherit it.

### 2. What the x86_64 Linux profile actually restricts

`content_process_sandbox_profile()` (lines 108–140) builds a gaol `Profile`:

* `Operation::FileReadAll` for `/dev/urandom` plus the embedder's declared
  resource files (`resources::sandbox_access_files()`) and directory subpaths
  (`resources::sandbox_access_files_dirs()`).
* gaol on Linux realizes profiles through **namespaces + seccomp** (upstream
  `gaol` crate — resolved in our tree), i.e. filesystem-view restriction plus
  syscall filtering; it is not a Chromium-grade broker, but it is real
  isolation, not a no-op.

### 3. Where brow's Phase 2 work interacts with the sandbox

* The DoH transport in `support/brow-net-core/src/dns/doh.rs` dials
  **bootstrap IPs directly** (`TcpStream::connect((bootstrap_ip, 443))`). Inside
  the content-process sandbox this is fine (network syscalls are not the
  restricted class in the gaol profile), but note the *architecture*: in servo,
  network fetches happen in the **net process**, not content processes — so DoH
  and QUIC sockets live outside the most restricted processes entirely.
* The HTTP/3 UDP endpoint (`quinn::Endpoint::client`) is likewise bound in the
  net process (`HttpState`), one socket per public/private context.
* No new file access was introduced by Phase 2; the only persisted artifact is
  `alt_svc_cache.json`, written by the existing `Exit` persistence path next to
  `hsts_list.json` (`components/net/resource_thread.rs`) — same trust domain,
  no new sandbox exceptions needed.

### 4. Gaps and recommended actions

1. **AArch64 Linux unsandboxed path** — before any ARM packaging (Phase 5),
   either implement a namespace-based profile for that target or ship ARM
   builds with single-process mode documented as *not* offering content-process
   isolation. Tracked for Phase 5.
2. **`Process::Unsandboxed` naming is honest but easy to misread** — the enum
   variant is used on all the ❌ targets above; our Phase 3 shell must surface
   "isolation status" from this, not assume it.
3. **Sandbox profile is file-system-centric** — gaol does not give us a
   Chromium-style per-site process model. Browsing-context isolation
   improvements (site isolation) are a Phase 3+ architectural decision and must
   not be promised in marketing copy.
4. **No Windows sandbox on Windows targets** — relevant to Phase 5's MSI story:
   `windows` target currently maps to the unsandboxed spawn path; our Phase 5
   Windows artifact inherits exactly the isolation servo 0.6 ships, no more.

## Bottom line

* x86_64 Linux (Phase 5's primary target): multiprocess with a real
  namespace/seccomp sandbox via gaol — **usable and testable now**.
* All other desktop targets: process separation without OS-level sandboxing.
* brow's Phase 2 network modernization required **zero** sandbox-profile
  changes and deliberately keeps its sockets in the net process.
