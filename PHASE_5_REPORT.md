# brow — Phase 5 Report: Build, Package & Cross-Platform Distribution

**Status: COMPLETE** · Branch `main` · Date: 2026-10-06

Phase gate respected: Phases 1–4 are complete and pushed (`4e5c917b2` is
`origin/main` at the start of this cycle); this phase was executed only after
the explicit "Continue to Phase 5" instruction.

---

## 1. Scope delivered

* **Tag-driven release pipeline** (`.github/workflows/release.yml`):
  `git tag v0.5.0 && git push origin v0.5.0` → two build jobs + a release job,
  publishing every artifact with a SHA256SUMS manifest on the GitHub Release.
  `workflow_dispatch` provides a no-publish dry-run mode.
* **Windows MSVC artifacts**:
  * `brow-X.Y.Z-x64.msi` — WiX 7 authoring (`packaging/windows/brow.wxs`,
    v4 schema, same toolchain version upstream servo pins: `WiX 7.0.0.0` via
    winget), installs `brow.exe` + `resources\` to `%ProgramFiles%\brow`,
    Start-Menu shortcut, Add/Remove-Programs icon, MajorUpgrade semantics.
  * `brow-X.Y.Z-x86_64-windows-portable.zip` — self-contained folder with
    run-notes (`packaging/windows/make-portable.ps1`).
  * Windows CI mirrors upstream `windows.yml` hygiene: LLVM 20.1 +
    `LIBCLANG_PATH` for aws-lc-rs bindgen, WinGet repair, `mach
    bootstrap-gstreamer`.
* **Linux artifacts**:
  * `.deb` (canonical: nfpm; cross-check: pure-dpkg `build-deb.sh`),
    `.rpm` (nfpm; reference `brow.spec` for distro packagers),
    `AppImage` (linuxdeploy + custom AppRun + headless extract test),
    portable `.tar.gz`.
  * Canonical layout `/usr/lib/brow/{brow,resources}` + `/usr/bin/brow`
    symlink — resources resolve relative to the ELF, which is how the engine
    (and Phase 4's EasyList engine) locates them at runtime.
  * Desktop integration: `brow.desktop` (HTTP/HTTPS + xhtml MIME,
    `StartupWMClass`, new-window action), AppStream 1.0 metainfo with
    OARS 1.1 rating and a 0.5.0 release entry, hicolor icons 64/128/256.
* **Checksum engineering**: `generate-checksums.sh` and `generate-checksums.ps1`
  emit a byte-identical text-mode manifest on both platforms; the release job
  runs `sha256sum -c` before publishing (fail-fast `if-no-files-found: error`,
  `fail_on_unmatched_files: true`).
* **Reproducible brand assets**: `packaging/icons/generate_icons.py` renders
  the brow tile/shield mark (PIL) into every needed raster + `.ico` — no
  unaudited binary blobs.
* **Ship profile**: `production-stripped` (`lto`, `codegen-units = 1`,
  `opt-level = "s"`, `strip`) — the size-optimized end of the vendored
  workspace's profile set, matching the "ultra-lightweight" product claim.
* **Docs**: `docs/PACKAGING.md` (artifact matrix, layouts, local builds,
  verification, limitations) and this report.

## 2. Validation evidence (measured, not estimated)

Local end-to-end validation on this Linux sandbox (2 cores, real dpkg/nfpm/
linuxdeploy), using a real workspace binary (`brow-resbench`, release build,
16.6 s) as the disposable payload — the CI pipeline packages the real
`brow-shell` engine build on runners:

| Check | Result |
|---|---|
| `cargo build --release -p brow-resbench` | green (16.6 s, 1 benign warning pre-existing in crate) |
| dpkg `.deb` build → `dpkg-deb --info` / `--contents` | control fields, `md5sums`, symlink, desktop/metainfo/icons/copyright all present |
| nfpm `.deb` + `.rpm` (v2.47.0) | both formats produced; `file` reports `RPM v3.0 bin i386/x86_64`; lead magic `\xed\xab\xee\xdb` verified |
| Layout cross-check nfpm-deb vs dpkg-deb | **PASSED** — all 6 functional paths present in both (binary, resources/, easylist.txt, /usr/bin/brow symlink, desktop, metainfo) |
| AppImage build → `--appimage-extract` assertions | **PASSED** — binary + `resources/easylist.txt` + desktop entry extracted |
| `generate-checksums.sh` → `sha256sum -c` | 4/4 artifacts OK |
| YAML lint (`ci.yml`, `release.yml`) | parsed clean; `push.branches == ['main']` |
| XML lint (`brow.wxs`, `brow.metainfo.xml`), desktop-file scan | clean |

CI performs the full-artifact validation that a 10 GB sandbox cannot:
engine builds on both platforms, MSI compilation (WiX 7 runs on Windows),
and the published SHA256SUMS.

## 3. Bugs found and fixed during validation (kept for the record)

1. **`ci.yml` corrupted trigger** — `branches: ain]` (a Phase 2–4 era typo
   that survived review; the Phase 4 log believed it verified). Now
   `branches: [main]`. Without this, push-triggered CI silently never ran.
2. **`build-deb.sh` missing `DEBIAN/` control dir** — `dpkg-deb --build`
   failed; fixed by explicit `mkdir` (first execution caught it).
3. **nfpm `overrides.rpm.group` is invalid** — `group` belongs in a
   top-level `rpm:` block (nfpm 2.47 schema rejected the config; caught on
   first run).
4. **nfpm does not expand env vars in `contents[].src`** — payload path
   moved to the canonical relative staging dir `packaging/payload/`;
   `version: ${BROW_VERSION}` *is* expanded and stays env-driven.
5. **`--custom-apprun` must point outside the AppDir** — passing the
   in-AppDir path made linuxdeploy copy a file onto itself; AppRun is now
   staged outside and copied into place by linuxdeploy.
6. **`sha256sum -c`-compatible manifest required bare filenames + absolute
   `OUT_DIR` resolution in the AppImage builder** (its extract check `cd`s
   into a temp dir).
7. **Symlink-aware cross-check patterns** — `dpkg-deb -c` symlink lines end
   with their target, so `$`-anchored patterns misfire; checks now anchor on
   the tar entry type character (`^-…`, `^l…`).

## 4. Deviations & honest accounting

* **Local payloads are stand-ins by design.** The engine cannot be built in
  this sandbox (10 GB disk, 2 cores — see Phase 3/4 reports on the same
  constraint). Local validation therefore used the small real workspace
  binary `brow-resbench`; every artifact-producing script is identical to
  what CI runs against the real `brow-shell` build. Nothing fake ships.
* **Windows/MSI verification is CI-side.** No Windows host or dotnet/WiX in
  the sandbox; the `.wxs` is XML-validated locally, and the authoring model
  plus tool versions mirror upstream servo's maintained `windows.yml`.
* **nfpm pinned, not floating** — `v2.47.0` (built 2026-06-20, Go 1.26) is
  the current release; the workflow still supports `NFPM_VERSION=latest`
  via the redirect-resolution trick (REST API avoided: shared-runner IPs
  exhaust its unauthenticated quota — observed live in this session).
* **Phase 5 does not add macOS** — out of the specified scope (Windows +
  Linux targets).

## 5. Commits

* `phase(5): packaging toolchain — nfpm deb/rpm, dpkg cross-check, AppImage, WiX 7 MSI, portable zip, SHA256SUMS, icons`
* `phase(5): release pipeline — tag-driven CI for Windows/Linux artifacts; fix ci.yml push trigger`
* `phase(5): docs — PACKAGING.md, README status, PHASE_5_REPORT.md`

(Exact hashes recorded in the push log; all work is on `origin/main`.)

## 6. Limitations & next steps

* AppImage bundles `ldd`-discoverable libs only; GStreamer plugin packs come
  from the base distro (hard deps of the .deb/.rpm). A fully self-contained
  media AppImage would use `linuxdeploy-plugin-gstreamer` — deferred.
* No code-signing yet: Authenticode (Windows) and distro signing keys
  (deb/rpm) are natural Phase 6+ hardening alongside the validation cycle.
* Screenshots for the AppStream metainfo need a GUI session (headless CI
  cannot produce them honestly); the component is valid without them.
* Suggested release flow for the maintainer: tag `v0.5.0` on `origin/main`
  after reviewing the `workflow_dispatch` dry run.

Phase 5 (build, package & cross-platform compilation) is **complete**.
Per the phase-gate protocol, work stops here until the explicit
"Continue to Phase 6" instruction.
