/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Userscript package generation.
//!
//! Servo's script engine supports per-document userscripts (`--userscripts
//! <dir>`): every `*.js` file in the directory runs at document load. brow
//! ships its anti-fingerprinting defenses through this exact mechanism —
//! this module materialises the generated payload (see [`crate::fingerprint`])
//! into the directory the shell wires into the engine, so the defense is
//! active on every page without per-site configuration.

use std::path::{Path, PathBuf};

use crate::fingerprint::{build_payload, DefenseLevel, FingerprintConfig, SessionKey};

/// Write the fingerprint-defense userscript into `dir`.
///
/// Servo's script engine runs every `*.js` file from the configured
/// userscripts directory at document load, so the shell only needs to pass
/// `--userscripts <profile>/userscripts`. Per-origin seeds are derived at
/// runtime inside the script; the file is session-scoped.
pub fn write_defense_package(
    dir: &Path,
    level: DefenseLevel,
    session: &SessionKey,
    config: &FingerprintConfig,
) -> std::io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let payload = build_payload(level, session, config);
    let mut written = Vec::new();
    if !payload.script.is_empty() {
        let path = dir.join("brow-fingerprint-defense.js");
        std::fs::write(&path, &payload.script)?;
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_nonempty_script() {
        let dir = std::env::temp_dir().join(format!(
            "brow-userscripts-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let session = SessionKey([7u8; 16]);
        let written = write_defense_package(
            &dir,
            DefenseLevel::Standard,
            &session,
            &FingerprintConfig::default(),
        )
        .unwrap();
        assert_eq!(written.len(), 1);
        let content = std::fs::read_to_string(&written[0]).unwrap();
        assert!(content.contains("getImageData"));
        assert!(content.ends_with("})();\n"));
        // Off level → empty payload → no file written
        let written_off = write_defense_package(
            &dir,
            DefenseLevel::Off,
            &session,
            &FingerprintConfig::default(),
        )
        .unwrap();
        assert!(written_off.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
