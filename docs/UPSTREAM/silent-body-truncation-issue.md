# Upstream issue draft: silent response-body truncation poisons the HTTP cache

**Target:** servo/servo, `components/net/http_loader.rs` (+ `http_cache.rs`,
`disk_cache.rs`)
**Status:** found in brow (`v0.7-rebuild`) while triaging why YouTube died
with `spf.js:56 "expected expression, got end of script"`. Upstream parses
these multi-MB scripts fine, so upstream's failure mode differs — but the
truncation class below is upstream code, and the cache poisoning makes it
persistent across restarts.

---

## Title

net: mid-body errors deliver partial bodies as completed responses and poison the HTTP cache

## Reproduction sketch

1. Fetch a large script (multi-MB, e.g. YouTube's spf.js) over a flaky
   connection or an h3 path with aggressive pool eviction.
2. Any error while the body is streaming (not `InvalidData`) hits the
   `.map_err` arm of the body-collection future in
   `components/net/http_loader.rs` (the future spawned after
   `*response_body.lock() = ResponseBody::Receiving(vec![])`).

## Current behavior

- Only `ErrorKind::InvalidData` (decompression) produces
  `Data::Error(NetworkError::DecompressionError)`.
- **Every other mid-body error** takes the same shared-body lock, moves the
  PARTIAL bytes out, marks the body `ResponseBody::Done(partial)`, and sends
  `Data::Done`.
- The script element compiles whatever arrived →
  `"expected expression, got end of script"` at the truncation point.
- The http-cache holds the live shared body Arc
  (`CachedResource.body`), so the truncated body is marked done and served
  to every later load; on eviction it is flushed to the disk cache
  (`DiskCache::store` accepts anything `is_done()`) and restored across
  browser restarts. The corruption is therefore persistent.

## Expected behavior

A body that failed mid-transfer is not a response. The resource should fail
(`Data::Error`), the partial bytes should never reach the parser, and the
entry should never be stored or served (memory or disk).

## brow's fix (implemented, shipping)

- `map_err`: non-cancellation mid-body errors send
  `Data::Error(NetworkError::ResourceLoadError(...))` and set the shared
  response's `aborted` flag; decompression failures and cancelled fetches
  set `aborted` too.
- The `aborted` flag is already respected by cache serve
  (`create_cached_response`), revalidate-candidate selection, and now
  (patched) by `DiskCache::store`'s `is_done()` filter.
- Content-Length cross-checks live at the WIRE layer only: hyper enforces
  CL/chunked framing client-side (its error now fails the resource instead
  of being swallowed), and brow's h3 pump counts data frames
  pre-decompression and rejects a clean FIN whose byte count contradicts
  Content-Length. A serve-time equality check would be unsound because the
  stored body is post-decompression while CL is the wire length.
- Cache-format namespace bump (`brow-cache-v2` key prefix) makes
  already-poisoned disk entries unreachable without a migration; they age
  out under the size cap.

## Test sketch

- A CL-framed server that sends fewer bytes than promised → fetch must
  yield a network error (not a partial success).
- An h3 server that finishes the stream early → body must error.
- Cache keys namespaced per format generation → pre-fix entries unreachable.
