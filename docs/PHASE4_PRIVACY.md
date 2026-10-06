# brow Phase 4 — Privacy Protection Architecture

Status: implemented and tested (see `PHASE_4_REPORT.md` for numbers and
acceptance evidence). Owner: brow elite team (privacy / rendering / network).

Phase 4 gives brow its privacy identity: **network filtering, CNAME-cloaking
detection, CHIPS partitioned-cookie enforcement, and anti-fingerprinting
defenses**, all in Rust, all testable without a browser build.

## 1. Overview

```
┌────────────────────────────── fetch pipeline (components/net) ─────────────┐
│                                                                            │
│  fetch_with_cors_cache()                                                   │
│    ├── 1. PrivacyState::check_request()      ← network filter (EasyList)   │
│    │        blocked ⇒ NetworkError::BlockedByPrivacyFilter                 │
│    ├── 2. PrivacyState::check_cname()        ← CNAME-cloaking inspection   │
│    │        canonical host blocked ⇒ NetworkError::BlockedByPrivacyFilter  │
│    └── 3. main_fetch() → http_loader → connector (DoH, H3 — phase 2)       │
│                                                                            │
│  set_cookie_for_url()                                                      │
│    └── PrivacyState::chips_receive_allowed_attrs()                         │
│             Partitioned without Secure ⇒ cookie MUST be ignored            │
└────────────────────────────────────────────────────────────────────────────┘
```

The heavy lifting lives in a standalone crate, `support/brow-privacy`
(workspace member), so every rule of the engine is unit-testable and
bench-markable without compiling the browser.

## 2. Network filtering (`brow_privacy::filter`)

### 2.1 Parser (`filter/parser.rs`)

Full EasyList/ABP syntax support, verified against a **real vendored
EasyList snapshot** (79,855 lines, 2026-10-06, commit `cd705aa5`,
`support/brow-privacy/assets/`, CC BY-SA 3.0 — see
`assets/EASYLIST_LICENCE.md`):

| Feature | Handling |
|---|---|
| `||`, `|`, `^`, `*`, literals | typed pattern pieces, exact matcher |
| `@@` exceptions, `$important` | decision engine, important beats exception |
| `$third-party` / `$~third-party` / `$first-party` | party scope gate |
| `$domain=a|~b` (alias `$from=`), entity `example.*` | site evaluation incl. two-level public-suffix forms |
| 14 resource types (`$script`, `$image`, …) | destination bitmask from the fetch pipeline |
| `$badfilter` | signature-based mutual cancellation at load |
| `/regex/` patterns | compiled with the `regex` crate, case-sensitive, capped (512) |
| `##`, `example.com##`, `#@#`, `#?#` | cosmetic engine (procedural recorded, not applied) |
| hosts-file lines (`0.0.0.0 host`) | converted to `||host^` |
| `$csp=`, `$replace=`, `$redirect`, snippet filters, JS lookaheads | **dropped and counted** — never silently half-applied |

Known upstream breakage: EasyList ships ~40 malformed element-hide lines
(`##ref^="…"`) — the selector sanity check (no `{}`, `<`; sane charset)
rejects exactly those and counts them; nothing else in the corpus is
invalid (asserted in `tests/engine_tests.rs`).

### 2.2 Matching (`filter/matcher.rs`, `filter/mod.rs`)

Two-stage design (standard for production blockers):

1. **Prefilter** — two Aho-Corasick automata: host-hints of `||` rules and
   first literals of every other rule. Candidates come from
   `find_overlapping_iter` — *overlapping* iteration is essential; leftmost
   semantics silently swallow candidates (a real bug found and fixed
   during this phase: `/gpt.js$script` was shadowed by a shorter needle).
2. **Verify** — exact piece-wise matching with a bounded backtracking
   budget (8192 steps, DoS-safe), `^` separators accept `/?:=&.` and
   end-of-URL, `||` patterns are verified at authority label boundaries.

Decision semantics: exception beats block, `$important` block beats
exception; unknown destination types only match type-unqualified rules;
empty positive masks (`$~image,~xhr`) mean "all types except".

### 2.3 Cosmetic filtering (`filter/cosmetic.rs`)

Per-site selector resolution over label suffixes + entity keys
(`example.*`), with `#@#` unhide cancellation. Output feeds the
`CosmeticResult.hide` list the shell applies as CSS. Procedural filters
(`#?#`) are counted but not applied (documented limitation L3).

## 3. CNAME-cloaking detection (`brow_privacy::cname`, `brow-net-core` DNS)

Trackers alias first-party-looking hostnames to their infrastructure via
DNS CNAMEs. brow chases the chain at fetch time:

* `brow-net-core::dns::DohResolver::resolve_cname_chain()` — new phase 4
  DNS capability: CNAME-type query over the phase 2 DoH transport
  (RFC 8484, bootstrap IPs, TLS), wire-format parsing with loop guard
  (max 16 hops) and question-section spoof validation.
* `CnameDetector` — verdict rule: chain canonical name lands on a
  **different registrable domain** than the client-visible host ⇒ cloaked
  alias. Verdicts are cached per session.
* Enforcement: when the *canonical* host itself matches a filter rule, the
  request is dropped (`check_canonical` in the fetch hook). First-party
  same-registrable CDN aliases are never flagged.

## 4. CHIPS — `Partitioned` cookies (`brow_privacy::chips`)

Policy engine implementing the receive/send rules of
draft-ietf-httpbis-rfc6265bis:

* **Receive** (enforced in `components/net::set_cookie_for_url`):
  `Partitioned` without `Secure` ⇒ ignored (§5.6.3), counted in stats.
  Gate wired through the `cookie` crate's `partitioned()`/`secure()` parse.
* **Send** (policy engine, fully unit-tested): partition key =
  `scheme://host` of the top-level site; third-party unpartitioned cookies
  can be omitted (`block_third_party_unpartitioned`, default off); first
  party is registrable-domain based (consistent approximation, see L4).
* Parsing is independent of `cookie-rs` (raw `Set-Cookie` attribute scan,
  case-insensitive) so the policy is testable against spec text.

## 5. Anti-fingerprinting (`brow_privacy::fingerprint`, `userscripts`)

Defenses are **generated userscripts**: one self-contained IIFE per
session, written by `PrivacyState::new` into `<config_dir>/userscripts/`
and executed by the script engine at document load (Servo's userscript
mechanism — pass `--userscripts <dir>` in the shell, see README).

Noise model: xorshift128+ seeded **per-origin at runtime** (FNV-1a of
`location.origin` XOR per-session key words) — stable across reloads
inside a session, rotating on restart, distinct across sites. All noise
is applied at `±1` quantization so readings stay site-usable.

| Level | Defenses |
|---|---|
| Off | payload is empty (no file written) |
| Standard | Canvas 2D (`getImageData`/`toDataURL`/`toBlob` alpha noise), WebGL vendor/renderer spoof (UNMASKED_* 37445/37446) + `readPixels` noise, AudioContext `getChannelData` noise, `navigator.hardwareConcurrency`/`deviceMemory`/`platform` reduction |
| Strict | + font `measureText` jitter & `document.fonts.check` restriction, ClientRect jitter, `Intl` timezone freeze |

Per-site overrides and filter allowlists persist through
`exceptions::ExceptionsStore` (atomic JSON, same pattern as phase 2/3).

## 6. Statistics (`brow_privacy::stats`)

Lock-free counters (network blocked, CNAME flagged, cookies rejected,
fingerprint payloads) + per-host block counts (top-N). Persisted to
`<config_dir>/privacy_stats.json` on engine shutdown (`CoreResourceMsg::
Exit` handler), reloaded on start.

## 7. Engine integration points (all compile-verified)

| Site | Change |
|---|---|
| `components/net/privacy.rs` (new) | `PrivacyState` glue: lazy engine build from `resources/easylist.txt` (or pref path), request/canonical checks, CHIPS gate, userscript generation, stats |
| `components/net/fetch/methods.rs` | pre-fetch gate in `fetch_with_cors_cache` (filter + CNAME) returning `NetworkError::BlockedByPrivacyFilter` |
| `components/net/resource_thread.rs` | `HttpState.privacy` field (public+private states), CHIPS gate in `set_cookie_for_url`, stats persistence on `Exit` |
| `components/shared/net/lib.rs` | `NetworkError::BlockedByPrivacyFilter` variant (+ Debug arm) |
| `components/config/prefs.rs` | 5 prefs: `network_privacy_filter_enabled`, `network_privacy_filter_list_path`, `network_privacy_cname_detection_enabled`, `network_privacy_chips_require_secure_partitioned`, `network_privacy_fingerprint_level` |
| `support/brow-net-core/src/dns/{mod,wire}.rs` | `resolve_cname_chain()` + `parse_cname_chain()` with round-trip tests |
| `resources/easylist.txt` (new) | runtime filter list (real snapshot, attribution header inside) |

## 8. Testing & benchmarks

* **77 tests** in `brow-privacy` (62 unit + 8 real-EasyList integration +
  6 cross-module integration) + 4 new brow-net-core DNS tests.
* Real-corpus acceptance: known ad hosts (doubleclick/gpt,
  googlesyndication/adsbygoogle, ads-twitter) blocked; benign first-party
  and font/CSS hosts allowed; EasyList-vs-EasyPrivacy scope documented
  with an appended privacy-list test.
* Criterion benchmarks (`benches/match_bench.rs`): decision latency,
  full-list engine build, payload generation. Numbers in
  `PHASE_4_REPORT.md`.

## 9. Known limitations

* **L1** — no list auto-update yet (snapshot is dated; an updater is
  Phase 5+ work).
* **L2** — `$sitekey=` recorded but not evaluated (no ABP signing).
* **L3** — procedural cosmetic filters (`#?#`) counted, not applied.
* **L4** — partition keys and first/third-party evaluation use the client
  origin plus a registrable-domain approximation (small two-level public
  suffix table), not the full PSL; full top-level-site plumbing through
  cookie storage is future work. Receive-side CHIPS validation (the
  spec's MUST) is complete and exact.
* **L5** — fingerprint defenses run as userscripts (prototype wrapping),
  not engine-internal IDL hooks; defense-in-depth would add engine-level
  overrides in a later phase.
