/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// brow (phase5.1-c): requestIdleCallback — the coordinator API React-class
// schedulers and analytics probes check for. v1 model documented in
// components/script/dom/idle_deadline.rs.
// https://w3c.github.io/requestidlecallback/

[Exposed=Window]
interface IdleDeadline {
  readonly attribute boolean didTimeout;
  DOMHighResTimeStamp timeRemaining();
};

callback IdleRequestCallback = undefined (IdleDeadline deadline);

dictionary IdleRequestOptions {
  unsigned long timeout;
};

partial interface Window {
  unsigned long requestIdleCallback(IdleRequestCallback callback,
                                    optional IdleRequestOptions options = {});
  undefined cancelIdleCallback(unsigned long handle);
};
