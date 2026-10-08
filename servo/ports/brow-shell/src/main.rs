/* brow-shell: the brow product binary — a thin bin over the servoshell
 * desktop shell (brow docs/DECISIONS.md D-010).
 *
 * Responsibilities (and nothing else):
 *   1. resolve the profile dir (BROW_DATA_DIR / XDG / default) and load
 *      settings.json (brow-shell-core, unchanged schema = unchanged CI
 *      contract);
 *   2. bridge settings -> engine `Preferences` (the same mapping the v0.6.1
 *      shell applied at builder time — privacy toggles stay live);
 *   3. brand the shell (window title, app id, icon) via the servoshell
 *      facade;
 *   4. register the privacy status item (D-011: brow-privacy process-global
 *      atomics -> toolbar counter);
 *   5. hand over to `servoshell::shell::run_from_args`, which owns logging
 *      setup, the crypto provider, the event loop and every window.
 */

use std::path::PathBuf;

use servo::Preferences;

fn main() {
    // 1. Profile dir + settings (same resolution as the v0.6.1 shell).
    let data_dir = brow_data_dir();
    let _ = std::fs::create_dir_all(&data_dir);
    let settings = brow_shell_core::settings::Settings::load(&data_dir.join("settings.json"))
        .unwrap_or_default();
    log::info!(
        "brow: profile dir {}, home_page={}, block_ads={}",
        data_dir.display(),
        settings.home_page,
        settings.block_ads
    );

    // 2. Settings -> engine prefs (v0.6.1 `engine_preferences` mapping,
    //    builder-time so the privacy engine initializes with them).
    let mut preferences = Preferences::default();
    preferences.network_privacy_filter_enabled = settings.block_ads;
    preferences.network_privacy_block_third_party_cookies = settings.block_third_party_cookies;
    preferences.network_privacy_fingerprint_level = settings.fingerprint_defense.clone();
    preferences.network_privacy_cname_detection_enabled = settings.block_cname_tracking;
    preferences.network_dns_over_https_templates = settings.doh_template.clone();
    if matches!(settings.min_tls_version.as_str(), "1.2" | "1.3") {
        preferences.network_tls_min_version = format!("TLSv{}", settings.min_tls_version);
    }
    preferences.hidden_webview_max_fps = i64::from(settings.hidden_webview_fps);

    // 3+4. Identity + overrides + status provider, then run.
    servoshell::shell::set_identity(servoshell::shell::ShellIdentity {
        window_title: "brow",
        wayland_app_id: "org.brow.brow",
        icon_png: Some(include_bytes!("../assets/brow_64.png")),
    });
    servoshell::shell::set_overrides(servoshell::shell::ShellOverrides {
        initial_url: Some(settings.home_page.clone()),
        homepage: Some(settings.home_page.clone()),
        searchpage: None,
        preferences: Some(preferences),
        config_dir: Some(data_dir.join("engine")),
    });
    servoshell::shell::set_status_provider(Some(privacy_status_item));

    servoshell::shell::run_from_args(std::env::args().skip(1).collect());
}

/// brow (D-011): the toolbar status item — cumulative blocked-request count,
/// read lock-free from the privacy engine's process-global mirrors. Plain
/// ASCII text: egui's default fonts do not carry emoji glyphs.
fn privacy_status_item() -> Option<String> {
    let totals = brow_privacy::stats::global_totals();
    Some(format!("{} blocked", totals.network_blocked))
}

/// Profile dir: BROW_DATA_DIR -> $XDG_CONFIG_HOME/brow -> ~/.config/brow
/// (identical to the v0.6.1 shell's resolution in src/state.rs).
fn brow_data_dir() -> PathBuf {
    std::env::var("BROW_DATA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    std::env::var("HOME")
                        .map(|h| PathBuf::from(h).join(".config"))
                        .unwrap_or_else(|_| PathBuf::from("."))
                })
                .join("brow")
        })
}
