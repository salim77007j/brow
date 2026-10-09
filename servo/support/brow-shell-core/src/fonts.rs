/* Bundled-font installer (Phase 3, D-013).
 *
 * brow ships Noto Sans (Latin), Noto Sans Arabic, Noto Sans Hebrew and
 * Noto Sans SC in the release payload `fonts/` directory next to the
 * binary. The engine resolves fallback families through platform font
 * enumerators (fontconfig/font-kit on freetype platforms, DirectWrite on
 * Windows), which scan standard font directories — a payload directory is
 * NOT one of them. This module copies the payload fonts into the
 * per-user font directory once per machine (idempotent, size-checked), so
 * the script-aware engine fallback tables (components/fonts/platform,
 * per-platform font_list.rs) can resolve "Noto Sans SC",
 * "Noto Sans Arabic", "Noto Sans Hebrew" everywhere, including minimal
 * containers and CI sandboxes.
 *
 * Pure std — no engine, GUI or winit dependency — so it is fully unit
 * tested in the brow-shell-core fast CI job and runs as step 0 of the
 * thin brow-shell main, before any font stack initializes.
 *
 * NOTE (Windows): per-user fonts copied without registry registration are
 * not part of the DirectWrite system collection; the egui chrome loads
 * them directly by path (gui.rs), and the engine falls back to the
 * system's own CJK/Arabic/Hebrew families (YaHei/Malgun/Uighur/Segoe),
 * which the fallback tables already reference.
 */

use std::path::{Path, PathBuf};

/// Candidate payload directories relative to the running executable:
/// `<exe_dir>/fonts` (portable zip / installed layout) and
/// `<exe_dir>/../fonts` (dev builds under `<target>/<profile>/`, where
/// packaging places fonts one level up).
pub fn payload_font_dirs() -> Vec<PathBuf> {
    let Ok(exe) = std::env::current_exe() else {
        return Vec::new();
    };
    let Some(exe_dir) = exe.parent() else {
        return Vec::new();
    };
    let mut dirs = vec![exe_dir.join("fonts")];
    if let Some(parent) = exe_dir.parent() {
        dirs.push(parent.join("fonts"));
    }
    dirs
}

/// The per-user font directory for this platform, or `None` when the
/// required environment variables are unset.
pub fn user_font_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var_os("LOCALAPPDATA").map(|l| {
            PathBuf::from(l)
                .join("Microsoft")
                .join("Windows")
                .join("Fonts")
        })
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library").join("Fonts"))
    } else {
        std::env::var_os("HOME").map(|h| {
            PathBuf::from(h)
                .join(".local")
                .join("share")
                .join("fonts")
                .join("brow")
        })
    }
}

fn is_font_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("ttf") | Some("otf")
    )
}

/// Copy every font file found in `font_dirs` into `user_dir`.
///
/// Idempotent: a file is skipped when the same-size copy already exists.
/// Returns the number of files freshly copied this call.
pub fn install_fonts_from_dirs(font_dirs: &[PathBuf], user_dir: &Path) -> usize {
    if std::fs::create_dir_all(user_dir).is_err() {
        return 0;
    }
    let mut installed = 0;
    for dir in font_dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || !is_font_file(&path) {
                continue;
            }
            let Some(name) = path.file_name() else {
                continue;
            };
            let target = user_dir.join(name);
            if let (Ok(a), Ok(b)) = (std::fs::metadata(&path), std::fs::metadata(&target)) {
                if a.len() == b.len() {
                    continue;
                }
            }
            if std::fs::copy(&path, &target).is_ok() {
                installed += 1;
                log::info!("brow: installed font {}", target.display());
            }
        }
    }
    installed
}

/// Install the bundled payload fonts (see module docs). Returns the number
/// of fonts freshly copied; 0 when there is nothing to do (already
/// installed, no payload dir, or no usable user font dir).
pub fn install_bundled_fonts() -> usize {
    let font_dirs = payload_font_dirs();
    if font_dirs.is_empty() {
        return 0;
    }
    let Some(user_dir) = user_font_dir() else {
        return 0;
    };
    install_fonts_from_dirs(&font_dirs, &user_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "brow-fonts-test-{}-{}",
                tag,
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn fake_font(dir: &Path, name: &str, size: u64) -> PathBuf {
        std::fs::write(dir.join(name), vec![b'x'; size as usize]).unwrap();
        dir.join(name)
    }

    #[test]
    fn installs_fonts_and_is_idempotent() {
        let tmp = TempDir::new("install");
        let payload = tmp.path().join("fonts");
        let user = tmp.path().join("userfonts");
        std::fs::create_dir_all(&payload).unwrap();
        fake_font(&payload, "NotoSansSC-Regular.otf", 8_331_336);
        fake_font(&payload, "NotoSansHebrew-Regular.ttf", 26_860);
        fake_font(&payload, "README.md", 100); // non-font: must be skipped

        let first = install_fonts_from_dirs(std::slice::from_ref(&payload), &user);
        assert_eq!(first, 2, "two font files copied, non-font skipped");
        assert!(user.join("NotoSansSC-Regular.otf").exists());
        assert!(!user.join("README.md").exists());

        let second = install_fonts_from_dirs(&[payload], &user);
        assert_eq!(second, 0, "same-size copies are skipped on re-run");
    }

    #[test]
    fn refreshes_when_payload_changes() {
        let tmp = TempDir::new("refresh");
        let payload = tmp.path().join("fonts");
        let user = tmp.path().join("userfonts");
        std::fs::create_dir_all(&payload).unwrap();
        let path = fake_font(&payload, "NotoSansHebrew-Regular.ttf", 100);
        assert_eq!(
            install_fonts_from_dirs(std::slice::from_ref(&payload), user.as_path()),
            1
        );

        std::fs::write(&path, vec![b'y'; 200]).unwrap();
        assert_eq!(
            install_fonts_from_dirs(&[payload], user.as_path()),
            1,
            "changed-size payload is re-copied"
        );
    }

    #[test]
    fn missing_payload_dir_is_zero() {
        let tmp = TempDir::new("missing");
        let ghost = tmp.path().join("does-not-exist");
        assert_eq!(
            install_fonts_from_dirs(&[ghost], tmp.path().join("userfonts").as_path()),
            0
        );
    }
}
