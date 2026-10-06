# brow — Packaging & Distribution Engineering (Phase 5)

This document is the operator manual for brow's release artifacts: what ships,
how it is built, how to verify it, and how to produce the packages locally.

## 1. Artifact matrix

A release tag `vX.Y.Z` (see `.github/workflows/release.yml`) produces:

| Artifact | Platform | Producer | Contents |
|---|---|---|---|
| `brow-X.Y.Z-x64.msi` | Windows x86_64 | WiX 6 (`packaging/windows/brow.wxs`) | `brow.exe` + `resources\` into `%ProgramFiles%\brow`, Start-Menu shortcut, ARP icon |
| `brow-X.Y.Z-x86_64-windows-portable.zip` | Windows x86_64 | `packaging/windows/make-portable.ps1` | self-contained `brow\` folder (`brow.exe`, `resources\`, `brow.ico`, docs, `README.txt`) |
| `brow-X.Y.Z-x86_64.AppImage` | Linux x86_64 | `packaging/linux/build-appimage.sh` (linuxdeploy) | AppDir with `usr/lib/brow/{brow,resources}`, custom AppRun, desktop entry, icon, ldd-bundled libs |
| `brow_X.Y.Z_amd64.deb` | Debian/Ubuntu | **nfpm** (`packaging/nfpm.yaml`) | canonical Linux layout (below), curated runtime `Depends` |
| `brow-X.Y.Z-1.x86_64.rpm` | Fedora/openSUSE | **nfpm** (`packaging/nfpm.yaml`) | same layout, rpm `Requires` |
| `brow-X.Y.Z-x86_64-linux-gnu.tar.gz` | Linux x86_64 | release workflow | raw portable build (`brow/` folder) |
| `SHA256SUMS` | all | `packaging/generate-checksums.sh` / `.ps1` | `sha256sum -c`-compatible manifest over every artifact |

The binaries are built with the workspace's **`production-stripped`**
profile: `lto = true`, `codegen-units = 1`, `opt-level = "s"`,
`strip = true` — the "ultra-lightweight" profile, `--locked` against the
committed `Cargo.lock`.

## 2. Installed layout (Linux)

One canonical layout is shared by the .deb, the .rpm and the AppImage
(`packaging/nfpm.yaml` is the reference; `build-deb.sh` mirrors it):

```
/usr/lib/brow/brow                     real ELF binary
/usr/lib/brow/resources/               engine resources (incl. easylist.txt)
/usr/bin/brow                          -> /usr/lib/brow/brow  (symlink)
/usr/share/applications/brow.desktop
/usr/share/metainfo/brow.metainfo.xml  AppStream metadata
/usr/share/icons/hicolor/{64x64,128x128,256x256}/apps/brow.png
/usr/share/licenses/brow/LICENSE       (rpm/nfpm; deb uses doc/brow/copyright)
```

The binary is installed under `/usr/lib/brow` **with `resources/` beside it**
because the Servo engine resolves its resource directory relative to the
executable. `/usr/bin/brow` is a symlink so PATH lookup works.

Windows follows the same adjacency rule: `resources\` must stay next to
`brow.exe` (true for both the MSI install dir and the portable zip).

## 3. Release pipeline (CI)

```
git tag v0.5.0 && git push origin v0.5.0
        │
        ├─ linux-x86_64 (ubuntu-24.04)
        │    mach build --locked --profile production-stripped
        │    stage payload → tar.gz → dpkg .deb (cross-check)
        │    nfpm .deb + .rpm → layout cross-check → AppImage (+extract test)
        │
        ├─ windows-x86_64 (windows-2022)
        │    LLVM 20.1 (LIBCLANG_PATH) → WiX 6 (winget) → mach bootstrap-gstreamer
        │    mach build --locked --profile production-stripped
        │    stage payload → portable zip → MSI
        │
        └─ release (needs both)
             download artifacts → SHA256SUMS → sha256sum -c → GitHub Release
```

Design notes:

* **Two .deb implementations on purpose.** nfpm is the canonical packager;
  `build-deb.sh` (pure `dpkg-deb`, zero dependencies beyond dpkg) rebuilds the
  same layout and the workflow asserts both contain every functionally
  required path. The dpkg build is a CI-side check and is not shipped.
* **nfpm is pinned** (`NFPM_VERSION` env, default `v2.47.0`); when set to
  `latest` the tag is resolved from the `releases/latest` *redirect* — never
  the REST API, whose unauthenticated quota shared-runner IPs routinely
  exhaust.
* **Dry runs:** trigger the workflow manually (workflow_dispatch) with
  "Publish the GitHub Release" unchecked — all artifacts are built and
  checksummed, nothing is published.

## 4. Verifying a download

```sh
sha256sum -c SHA256SUMS          # from inside the artifact directory
```

Windows (PowerShell): `Get-FileHash -Algorithm SHA256 <file>` and compare
against `SHA256SUMS`. The .ps1 and .sh manifest generators emit the
byte-identical `"<hex>  <name>"` text-mode format, so manifests are uniform
across platforms.

## 5. Building packages locally (Linux)

```sh
# 0) payload contract: a dir containing brow (ELF 0755) + resources/
cargo build --release -p brow-shell            # inside servo/
mkdir -p packaging/payload
cp servo/target/release/brow-shell packaging/payload/brow
cp -R servo/resources packaging/payload/resources

export BROW_VERSION=0.5.0

# .deb + .rpm (canonical, nfpm ≥ 2.4x; run from repo root)
nfpm package -f packaging/nfpm.yaml -p deb -t packaging/dist/brow_${BROW_VERSION}_amd64.deb
nfpm package -f packaging/nfpm.yaml -p rpm -t packaging/dist/brow-${BROW_VERSION}-1.x86_64.rpm

# .deb (dpkg cross-check path)
./packaging/linux/build-deb.sh --payload packaging/payload --version "$BROW_VERSION"

# AppImage (downloads linuxdeploy; caches under ~/.cache/brow)
./packaging/linux/build-appimage.sh --payload packaging/payload --version "$BROW_VERSION"

# checksums
./packaging/generate-checksums.sh packaging/dist
```

Windows (PowerShell, after `winget install WiXToolset.WiXCLI`):

```powershell
powershell -File packaging\windows\make-portable.ps1 -PayloadDir <staging> -Version 0.5.0
powershell -File packaging\windows\build-msi.ps1     -PayloadDir <staging> -Version 0.5.0
```

Icons are reproducible from source: `python3 packaging/icons/generate_icons.py`.

## 6. Runtime dependencies

The nfpm `overrides` block and `build-deb.sh` carry curated runtime deps
mirroring servo's Linux requirements: X11/xcb, xkbcommon, wayland,
fontconfig/freetype, EGL/GL, GStreamer 1.0 (base + good as hard deps;
bad/libav as recommends), ALSA, dbus, udev. RPM names follow Fedora's.
`packaging/rpm/brow.spec` is provided for distro packagers who prefer
`rpmbuild` over nfpm and consumes the same payload contract.

## 7. Known limitations

* The MSI is compiled on Windows runners only (WiX 6 has no Linux-native
  story for this authoring model in our pipeline); the .wxs is XML-validated
  locally, the toolchain mirrors upstream servo's `windows.yml`.
* AppImages bundle what `ldd` discovers for `brow` itself; GStreamer plugin
  packs are expected from the base system (they are hard/recommended deps of
  the .deb/.rpm). A fully self-contained media AppImage would additionally
  need `linuxdeploy-plugin-gstreamer` — tracked for a later cycle.
* No macOS artifacts: out of the Phase 5 scope, which specifies
  Windows/Linux distribution targets.
