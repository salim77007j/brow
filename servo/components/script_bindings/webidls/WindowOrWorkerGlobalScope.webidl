/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// https://html.spec.whatwg.org/multipage/#windoworworkerglobalscope

typedef (TrustedScript or DOMString or Function) TimerHandler;

[Exposed=(Window,Worker)]
interface mixin WindowOrWorkerGlobalScope {
  [Replaceable] readonly attribute USVString origin;

  undefined reportError(any e);

  // base64 utility methods
  [Throws] DOMString btoa(DOMString data);
  [Throws] DOMString atob(DOMString data);

  // timers
  // brow (phase5.2): [Clamp] on timeout — a Date.now()-scale delay
  // (epoch ms ~1.79e12) previously wrapped through `long` (i32) into a
  // negative value, and the engine's `max(0)` turned that into an
  // IMMEDIATE fire. Chrome/Firefox clamp to 2^31-1 ms (~24.8 days). With
  // [Clamp] the binding layer clamps before the i32 conversion; negative
  // values still reach the engine and its max(0) handles them (spec:
  // timeout < 0 → fire immediately).
  [Throws] long setTimeout(TimerHandler handler, optional [Clamp] long timeout = 0, any... arguments);
  undefined clearTimeout(optional long handle = 0);
  [Throws] long setInterval(TimerHandler handler, optional [Clamp] long timeout = 0, any... arguments);
  undefined clearInterval(optional long handle = 0);

  // microtask queuing
  undefined queueMicrotask(VoidFunction callback);

  // ImageBitmap
  Promise<ImageBitmap> createImageBitmap(ImageBitmapSource image, optional ImageBitmapOptions options = {});
  Promise<ImageBitmap> createImageBitmap(ImageBitmapSource image, long sx, long sy, long sw, long sh,
                                         optional ImageBitmapOptions options = {});

  // structured cloning
  [Throws]
  any structuredClone(any value, optional StructuredSerializeOptions options = {});
};

// https://w3c.github.io/hr-time/#the-performance-attribute
partial interface mixin WindowOrWorkerGlobalScope {
    [Replaceable]
    readonly attribute Performance performance;
};

// https://w3c.github.io/webappsec-secure-contexts/#monkey-patching-global-object
partial interface mixin WindowOrWorkerGlobalScope {
  readonly attribute boolean isSecureContext;
};

// https://www.w3.org/TR/trusted-types/#extensions-to-the-windoworworkerglobalscope-interface
partial interface mixin WindowOrWorkerGlobalScope {
  readonly attribute TrustedTypePolicyFactory trustedTypes;
};

// https://fetch.spec.whatwg.org/#fetch-method
partial interface mixin WindowOrWorkerGlobalScope {
  [NewObject] Promise<Response> fetch(RequestInfo input, optional RequestInit init = {});
};

// https://w3c.github.io/ServiceWorker/#global-caches-attribute
partial interface mixin WindowOrWorkerGlobalScope {
  [Pref="dom_serviceworker_enabled", SecureContext, SameObject] readonly attribute CacheStorage caches;
};

Window includes WindowOrWorkerGlobalScope;
WorkerGlobalScope includes WindowOrWorkerGlobalScope;
