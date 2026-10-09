# YouTube status in brow (honest verdict, Phase 4)

**Verdict: degraded-but-functional at best in v0.7.** Phase 4 removes one
class of failure (browser-bug truncation) and documents the remaining one
(missing web-platform APIs). It does not make YouTube "work".

## What was broken and what we fixed

Owner-hardware symptom: YouTube never finished loading; console showed

```
spf.js:56        "expected expression, got end of script"
kevlar_base:1010 "missing } after function body"
[bugsnag]        "No valid entry type provided to observe()"
```

Root cause (R-16, fixed in Phase 4.4): mid-body network errors were
delivered as **completed** responses carrying the partial bytes. YouTube's
multi-MB scripts got truncated in flight (flaky path + brow's h3 pump:
30 s chunk timeouts, pool eviction, early FIN treated as clean EOF, no
Content-Length check anywhere), the truncated source was compiled (→
syntax errors), and the poisoned body was served from memory+disk cache
across restarts — which is why the errors persisted. The privacy engine
was exonerated during research: its fingerprint-defense payload runs as a
delayed userscript in the page realm and never rewrites response bodies.

Fixes shipped:

1. Mid-body errors fail the resource (`Data::Error`); partial bytes never
   reach the parser (http_loader + h3 pump wire-level Content-Length check).
2. Failed bodies are never stored or served (aborted flag across serve /
   revalidation / disk flush; cache namespace bump isolates pre-fix disk
   entries).
3. `network_http3_enabled` defaults OFF for v0.7 (D-016) — the custom h3
   path is opt-in until it survives owner A/B validation.

Expected after the fix: **no more syntax-error class**; script loads either
succeed fully or fail loudly (reload-able), including on YouTube.

## What remains broken on YouTube (not fixed in v0.7)

Upstream servo/servo#47963 documents that YouTube does not fully work on
pure upstream either: missing `Animation`, `SVGAnimatedString`, and related
API surface break the search results page. brow inherits that gap verbatim —
it is upstream-scale work, not something a product fork should patch
locally.

Also seen, cosmetic: `[bugsnag] No valid entry type provided to observe()`
is servo's own console warning from `PerformanceObserver.observe()` with
entry types servo does not implement yet (components/script
`performanceobserver.rs`) — an API-gap symptom, unrelated to truncation.

## Practical expectations for v0.7 owner testing

- With h3 off (default) and a fresh profile: YouTube should load further
  than before and stop throwing parse errors; playback/search may still
  fail or render partially due to the upstream API gaps above.
- This is documented as R-17 (OPEN, upstream-track). Revisit if/when
  upstream lands the missing DOM APIs (#47963).
