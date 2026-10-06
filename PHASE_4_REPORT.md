# brow — Phase 4 Report: Stealth Ad Blocker & Privacy Engine

**Status: COMPLETE** · Branch `main` · Date: 2026-10-06

Phase gate respected: Phases 1–3 are complete and pushed (`da7af9610`,
`8e59cc7c4…1b630b41c`, `4539a498f…a1e1637ac`); this phase was executed only
after the explicit "Continue to Phase 4" instruction. No deviations from the
phase protocol in this cycle.

---

## 1. Scope delivered

| Deliverable (tasking) | Delivered as |
|---|---|
| EasyList ad/tracker filtering | `support/brow-privacy` — full ABP-syntax parser + two-stage Aho-Corasick engine, verified against a **real vendored EasyList snapshot** (79,855 lines, 2026-10-06, commit `cd705aa5`, CC BY-SA 3.0, attribution in `assets/EASYLIST_LICENCE.md`) |
| Cosmetic (element-hiding) filtering | `filter/cosmetic.rs` — generic + domain-scoped + `#@#` unhide + entity domains |
| CNAME-cloaking detection | `brow_privacy::cname` verdict engine + new DoH capability `resolve_cname_chain()` in `brow-net-core` (CNAME-type query, wire parsing, loop guard) + session verdict cache |
| Anti-fingerprinting (Canvas/WebGL/AudioContext/fonts/Navigator) | `brow_privacy::fingerprint` payload generator (Off/Standard/Strict), per-origin runtime seeds, shipped as a session userscript (`userscripts.rs`) through Servo's userscript mechanism |
| CHIPS (Partitioned cookies) | `brow_privacy::chips` policy engine + receive-side enforcement wired into `components/net::set_cookie_for_url` |
| In-engine integration | fetch-pipeline gate, `NetworkError::BlockedByPrivacyFilter`, `HttpState.privacy`, 5 new prefs, stats persisted on Exit |
| Benchmarks & tests | criterion suite (`benches/match_bench.rs`) on the real list; 77 privacy tests + 4 new DNS tests, all green |
| CI | new `brow-privacy` gate job (tests + benchmark smoke) in `.github/workflows/ci.yml` |
| Docs | `docs/PHASE4_PRIVACY.md` (architecture), this report, README updates |

## 2. Engine numbers (measured, not estimated)

Environment: 2-core sandbox, Rust 1.97.1 pinned by `rust-toolchain.toml`.

### 2.1 Decision throughput (`cargo bench -p brow-privacy`, release)

Full real-list engine (54,934 network rules after dedup/badfilter):

| Request shape | Decision | p50 latency |
|---|---|---|
| `pagead2.googlesyndication.com/pagead/js/adsbygoogle.js` | Block (`||pagead2.googlesyndication.com^`) | **4.24 µs** |
| `securepubads.g.doubleclick.net/tag/js/gpt.js` | Block (`||g.doubleclick.net^`) | **4.06 µs** |
| `google-analytics.com/analytics.js` | Allow (not in EasyList — see §3.2) | 4.85 µs |
| `connect.facebook.net/…/fbevents.js` | Allow | 2.77 µs |
| `cdn.taboola.com/libtrc/loader.js` | Allow | 3.22 µs |

⇒ ~200k–360k decisions/s per core; filter cost per subresource request is
in the low single-digit microseconds — invisible next to network latency.

### 2.2 Engine build

Full EasyList parse + automata build + badfilter cancellation:
**~318 ms** (criterion: 316–325 ms across two runs). One-time cost at
startup; list loads are lazy (first fetch triggers build).

### 2.3 Engine memory

`engine_memory_budget` test (`/proc/self/status` VmRSS delta, debug build):
**75 MiB** for 54,934 network rules + 13,592 generic cosmetic +
9,869 domain cosmetic + 327 unhides + 287 procedural + 22 regex rules.
Debug builds inflate `Vec<NetworkRule>` metadata; release is smaller. The
CI guardrail asserts < 300 MiB.

## 3. Correctness evidence

### 3.1 Test totals — all green

| Suite | Result |
|---|---|
| `brow-privacy` unit | **63/63** |
| `brow-privacy` real-EasyList integration (`tests/engine_tests.rs`) | **8/8** |
| `brow-privacy` cross-module integration (`tests/integration_tests.rs`) | **6/6** |
| `brow-net-core` (incl. 2 new CNAME wire tests) | **37/37** |
| `cargo check -p servo-net --tests` (engine glue) | clean |
| `cargo clippy -p brow-privacy --all-targets` | clean (0 warnings) |

### 3.2 Real-corpus acceptance (`tests/engine_tests.rs`)

* **Blocks** (site `https://www.nytimes.com/`): GPT/securepubads,
  `googleads.g.doubleclick.net/pagead/id`, `ad.doubleclick.net/ddm/…`,
  `adsbygoogle.js`, `tpc.googlesyndication.com` loader, `ads-twitter/uwt.js`
  — every block's matched rule is surfaced (e.g. `||g.doubleclick.net^`).
* **Allows**: first-party CSS/JS, Wikipedia, Google Fonts — no false
  positives on the corpus.
* **Scope honesty test**: `googletagmanager.com` has **zero rules** in
  EasyList (it lives in EasyPrivacy). The test asserts the allow decision,
  then appends `||googletagmanager.com^` + `||google-analytics.com^` and
  asserts both block — proving the engine handles privacy-list rules
  identically.
* **Exception semantics**: appended `@@||securepubads…$domain=trusted.example`
  wins only on `trusted.example`; `$important` block beats a matching
  exception; `$badfilter` cancels its twin.
* **Parse quality**: 54,183 network blocks + 755 exceptions + 13,592
  generic + 9,869 domain cosmetic + 327 unhides + 287 procedural + 22
  regex rules parsed from 79,855 lines; only the ~40 genuinely malformed
  upstream `##ref^=` lines are rejected (counted, asserted < 100).
  Unsupported behavioural options (`$csp=`, `$replace=`, `$redirect`, …)
  are dropped and counted — never half-applied.

### 3.3 Bugs found and fixed during verification (kept for the record)

1. **Aho-Corasick candidate shadowing** — `MatchKind::LeftmostFirst`/`Standard`
   silently swallow matches contained inside other matches, so
   `/gpt.js$script` never fired under the full list. Fixed with
   `find_overlapping_iter` (candidate generators need every hit).
2. **Anchor pinning** — the first pattern literal now must match exactly at
   the `||` label boundary / AC hit position (previously a substring hit
   inside `notads.example.com` could satisfy `||ads.example.com^`).
3. **Entity wildcard overmatch** — `example.*` matched `example.com.evil`;
   now restricted to base + one (possibly two-level) TLD, consistent with
   the registrable-domain approximation.
4. **`~third-party` negation** was inverted at parse; fixed.
5. **Empty positive type masks** (`$~image,~xhr`) were rejected; now mean
   "all types except the negated ones".

## 4. In-engine integration (compile-verified)

| Site | Change |
|---|---|
| `components/net/privacy.rs` (new, ~260 lines) | `PrivacyState`: lazy engine build (default `resources/easylist.txt`, pref-overridable), `check_request`, `check_canonical`, async `check_cname`, CHIPS gate, userscript generation, stats |
| `components/net/fetch/methods.rs` | gate at the top of `fetch_with_cors_cache`: filter decision, then (https) CNAME inspection with canonical-host re-evaluation; blocks return `NetworkError::BlockedByPrivacyFilter` |
| `components/net/resource_thread.rs` | `HttpState.privacy` (public+private), CHIPS receive gate in `set_cookie_for_url`, stats persistence on `CoreResourceMsg::Exit` (`privacy_stats.json`) |
| `components/shared/net/lib.rs` | `NetworkError::BlockedByPrivacyFilter` (+ Debug arm) |
| `components/net/tests/main.rs` | test `HttpState` literal updated |
| `components/config/prefs.rs` | `network_privacy_filter_enabled` (true), `network_privacy_filter_list_path` (""), `network_privacy_cname_detection_enabled` (true), `network_privacy_chips_require_secure_partitioned` (true), `network_privacy_fingerprint_level` ("standard") |
| `support/brow-net-core/src/dns/{mod,wire}.rs` | `resolve_cname_chain` + `parse_cname_chain` (CNAME-type DoH query, owner→target walk, 16-hop loop guard, question-section spoof validation) with round-trip unit tests |
| `resources/easylist.txt` (new, 2.1 MB) | runtime list; attribution header embedded at the top of the file |
| `.github/workflows/ci.yml` | `brow-privacy` gate job (tests + bench smoke) added |

The fingerprint userscript is written by the engine into
`<config_dir>/userscripts/brow-fingerprint-defense.js`; the shell passes
`--userscripts <config_dir>/userscripts` (Servo's native userscript
mechanism) to activate it at document load.

## 5. Sandboxing / environment constraints encountered

* Disk-capped sandbox (10 GiB, ~95% occupied by phase 2/3 caches + this
  phase's artifacts) forced `cargo clean`-style hygiene: bench binaries and
  incremental caches were pruned after measurement. The full-engine
  `cargo check` of `brow-shell`/`servoshell` remains CI-enforced (unchanged
  from phase 3 policy); every phase 4 touch-point inside `components/net`,
  `components/shared/net` and `components/config` **is** locally
  compile-verified.
* `rustc-dev`/`llvm-tools` toolchain components (crown-only) were dropped
  locally to fit the check; CI installs the full pinned toolchain.

## 6. Limitations (honest accounting)

* **L1 — list freshness**: the shipped list is a dated snapshot (2026-10-06);
  automatic list updating is deferred (Phase 5+). The pref
  `network_privacy_filter_list_path` accepts any user-supplied list.
* **L2 — `$sitekey=`** recorded on rules, not evaluated (no ABP signing).
* **L3 — procedural cosmetic filters** (`#?#`) are counted and surfaced in
  stats but not applied.
* **L4 — top-level-site plumbing**: partition keys and first/third-party
  evaluation use the client origin (correct for subresource fetches) and a
  registrable-domain approximation (two-level public suffix table), not the
  full PSL; full partition-key storage in `CookieStorage` is future work.
  The receive-side CHIPS MUST rule (`Partitioned` ⇒ `Secure`) is complete
  and exact.
* **L5 — userscript-based fingerprint defenses** wrap prototypes in page
  JS; engine-internal IDL-level enforcement (defense-in-depth) is a
  candidate for a later phase.

## 7. Commits

1. `phase(4): brow-privacy crate — EasyList/ABP engine, cosmetic filtering, CNAME/CHIPS/fingerprint modules (77 tests)`
2. `phase(4): engine integration — fetch gate, CNAME uncloaking via DoH, CHIPS cookie enforcement, fingerprint userscript`
3. `phase(4): CI privacy gate, docs, report`

All pushed to `origin/main` after this report was written.
