/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! brow (phase5.1-c): `IdleDeadline` for `requestIdleCallback`.
//!
//! v1 model, documented honestly: the callback task is queued on the DOM
//! manipulation task source (the next event-loop turn) with the standard
//! 50 ms idle budget. `timeRemaining()` reports the remaining budget from
//! the moment the task started running, which is what the spec's deadline
//! model describes for the non-timed-out path. The `options.timeout`
//! forced-run path (didTimeout = true) is accepted by the IDL and recorded
//! as a v2 refinement in docs/PHASE5_PLAN.md — sites in the wild
//! overwhelmingly probe presence and run opportunistic work through the
//! no-options path.

use dom_struct::dom_struct;
use js::context::JSContext;
use script_bindings::reflector::{Reflector, reflect_dom_object_with_cx};
use script_bindings::root::DomRoot;
use servo_base::cross_process_instant::CrossProcessInstant;

use crate::dom::bindings::codegen::Bindings::IdleDeadlineBinding::IdleDeadlineMethods;
use crate::dom::bindings::codegen::Bindings::PerformanceBinding::DOMHighResTimeStamp;
use crate::dom::performance::performance::ToDOMHighResTimeStamp;
use crate::dom::window::Window;

/// The idle budget the spec grants a callback: 50 ms per idle period.
/// <https://w3c.github.io/requestidlecallback/#idle-periods>
pub(crate) const IDLE_BUDGET_MS: f64 = 50.0;

/// <https://w3c.github.io/requestidlecallback/#the-idledeadline-interface>
#[dom_struct]
pub(crate) struct IdleDeadline {
    reflector_: Reflector,

    /// Clock instant when the idle task started running; `timeRemaining`
    /// is the budget minus the elapsed slice.
    start: CrossProcessInstant,

    /// Whether this deadline comes from the `options.timeout` forced run.
    did_timeout: bool,
}

impl IdleDeadline {
    fn new_inherited(start: CrossProcessInstant, did_timeout: bool) -> Self {
        Self {
            reflector_: Reflector::new(),
            start,
            did_timeout,
        }
    }

    pub(crate) fn new(
        window: &Window,
        cx: &mut JSContext,
        start: CrossProcessInstant,
        did_timeout: bool,
    ) -> DomRoot<IdleDeadline> {
        reflect_dom_object_with_cx(
            Box::new(IdleDeadline::new_inherited(start, did_timeout)),
            window,
            cx,
        )
    }

    /// <https://w3c.github.io/requestidlecallback/#dom-idledeadline-timeremaining>
    fn TimeRemaining(&self) -> DOMHighResTimeStamp {
        let elapsed = (CrossProcessInstant::now() - self.start).to_dom_high_res_time_stamp();
        (IDLE_BUDGET_MS - elapsed).max(0.0)
    }

    /// <https://w3c.github.io/requestidlecallback/#dom-idledeadline-didtimeout>
    fn DidTimeout(&self) -> bool {
        self.did_timeout
    }
}
