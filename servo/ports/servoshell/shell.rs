/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Product-embedder facade (brow, see brow docs/DECISIONS.md D-010/D-011).
//!
//! Lets a thin product binary rebrand and parameterize the servoshell desktop
//! shell without forking it: window identity (title, app id, icon), initial
//! URL / homepage / search page, a full engine `Preferences` replacement, the
//! engine config directory, and an optional toolbar status-item provider.
//!
//! Everything defaults to servo behavior: if the embedder never calls
//! [`set_identity`] / [`set_overrides`] / [`set_status_provider`], the shell
//! is byte-for-byte the servo shell (same title, same icon, same defaults,
//! no status item).

use servo::Preferences;
use std::path::PathBuf;
use std::sync::OnceLock;

/// Window/process identity a product embedder can override.
pub struct ShellIdentity {
    /// Initial window title (also the Wayland instance name).
    pub window_title: &'static str,
    /// Wayland app id (Linux taskbar pinning); Windows ignores it.
    pub wayland_app_id: &'static str,
    /// Window icon PNG bytes (Linux/Windows taskbar). `None` sets no icon.
    pub icon_png: Option<&'static [u8]>,
}

/// Everything a product embedder can override before the shell runs.
pub struct ShellOverrides {
    /// URL for the first tab (falls back to `homepage`, then about:blank).
    pub initial_url: Option<String>,
    /// Homepage (used when no initial URL was given).
    pub homepage: Option<String>,
    /// Search page used to interpret non-URL location-bar input.
    pub searchpage: Option<String>,
    /// Replaces the parsed engine `Preferences` wholesale. The product bin
    /// builds this from its own settings store (brow: settings.json → prefs,
    /// the same mapping the v0.6.1 shell applied at builder time).
    pub preferences: Option<Preferences>,
    /// Engine config directory (brow: `<profile>/engine`, where the privacy
    /// stats snapshot and fingerprint userscripts materialize).
    pub config_dir: Option<PathBuf>,
}

static SERVO_IDENTITY: ShellIdentity = ShellIdentity {
    window_title: "Servo",
    wayland_app_id: "org.servo.Servo",
    icon_png: Some(include_bytes!("../../resources/servo_64.png")),
};

static IDENTITY: OnceLock<ShellIdentity> = OnceLock::new();
static OVERRIDES: OnceLock<ShellOverrides> = OnceLock::new();
static STATUS_PROVIDER: OnceLock<Option<fn() -> Option<String>>> = OnceLock::new();

/// Set the product identity. Only the first call wins; later calls log and
/// are ignored (the shell runs once per process).
pub fn set_identity(identity: ShellIdentity) {
    if IDENTITY.set(identity).is_err() {
        log::warn!("shell::set_identity called more than once — ignoring");
    }
}

/// The active identity (servo defaults if the embedder set none).
pub fn identity() -> &'static ShellIdentity {
    IDENTITY.get().unwrap_or(&SERVO_IDENTITY)
}

/// Set the embedder overrides. Only the first call wins.
pub fn set_overrides(overrides: ShellOverrides) {
    if OVERRIDES.set(overrides).is_err() {
        log::warn!("shell::set_overrides called more than once — ignoring");
    }
}

/// The active overrides, if the embedder set any.
pub(crate) fn overrides() -> Option<&'static ShellOverrides> {
    OVERRIDES.get()
}

/// Register the toolbar status-item provider (`None` clears it). Only the
/// first call wins — the product bin sets this once before running.
pub fn set_status_provider(provider: Option<fn() -> Option<String>>) {
    if STATUS_PROVIDER.set(provider).is_err() {
        log::warn!("shell::set_status_provider called more than once — ignoring");
    }
}

/// The current status item, if a provider is registered and produced one.
/// Called once per egui frame from the toolbar.
pub(crate) fn status_item() -> Option<String> {
    match STATUS_PROVIDER.get() {
        Some(Some(provider)) => provider(),
        _ => None,
    }
}

/// Run the desktop shell with the given argument vector (no binary name).
/// Servo's own binary forwards `std::env::args().skip(1)`.
pub fn run_from_args(args: Vec<String>) {
    crate::desktop::cli::run_from_args(args)
}
