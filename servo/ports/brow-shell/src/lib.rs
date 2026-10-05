/* brow-shell library: chrome state + Slint platform + (engine) app logic.
 * The bin crate is a thin wrapper over this library, which also makes the
 * headless UI smoke test possible. */

pub mod chrome;
pub mod generated {
    slint::include_modules!();
}
pub mod keymap;
pub mod platform;
pub mod state;

#[cfg(feature = "engine")]
pub mod app;
#[cfg(feature = "engine")]
pub mod delegate;
#[cfg(feature = "engine")]
pub mod waker;
