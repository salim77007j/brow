#!/usr/bin/env bash
# brow — .deb package builder (Phase 5)
#
# Pure-dpkg path: no debhelper, no external tool beyond dpkg-deb, so the
# packaging step is reproducible on any Debian-family machine and in minimal
# CI containers. This is the *fallback/cross-check* implementation; the
# canonical CI packager is nfpm (see ../nfpm.yaml). Both consume the SAME
# staged payload layout and must produce equivalent packages — running both
# in CI is a deliberate redundancy that catches staging regressions.
#
# Installed layout (mirrors ../nfpm.yaml — keep in sync):
#   /usr/lib/brow/brow                       real binary (resources sit beside it)
#   /usr/lib/brow/resources/                 servo engine resources incl. easylist.txt
#   /usr/bin/brow                            -> /usr/lib/brow/brow symlink
#   /usr/share/applications/brow.desktop
#   /usr/share/icons/hicolor/{64x64,128x128,256x256}/apps/brow.png
#   /usr/share/metainfo/brow.metainfo.xml
#   /usr/share/doc/brow/copyright
#
# Usage:
#   ./build-deb.sh --payload <dir> --version <v> [--out <dir>] \
#                  [--arch amd64] [--maintainer "Name <email>"]
#
# Payload contract: <dir>/brow (ELF, mode 0755) + <dir>/resources/ tree.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PAYLOAD_DIR=""
VERSION=""
OUT_DIR=""
ARCH="amd64"
MAINTAINER="brow engineering <salim77007j@users.noreply.github.com>"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --payload)    PAYLOAD_DIR="$2"; shift 2 ;;
    --version)    VERSION="$2"; shift 2 ;;
    --out)        OUT_DIR="$2"; shift 2 ;;
    --arch)       ARCH="$2"; shift 2 ;;
    --maintainer) MAINTAINER="$2"; shift 2 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[[ -n "$PAYLOAD_DIR" && -n "$VERSION" ]] || {
  echo "error: --payload and --version are required" >&2; exit 2; }
[[ -x "$PAYLOAD_DIR/brow" ]] || {
  echo "error: payload missing executable: $PAYLOAD_DIR/brow" >&2; exit 2; }
[[ -d "$PAYLOAD_DIR/resources" ]] || {
  echo "error: payload missing resources dir: $PAYLOAD_DIR/resources" >&2; exit 2; }

command -v dpkg-deb >/dev/null || {
  echo "error: dpkg-deb not found (Debian/Ubuntu required)" >&2; exit 1; }

OUT_DIR="${OUT_DIR:-$SCRIPT_DIR/dist}"
mkdir -p "$OUT_DIR"

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

# ---- payload tree ----------------------------------------------------------
USR="$STAGE/usr"
DEBIAN="$STAGE/DEBIAN"
mkdir -p "$DEBIAN" "$USR/lib/brow" "$USR/bin" \
         "$USR/share/applications" "$USR/share/metainfo" \
         "$USR/share/doc/brow" \
         "$USR/share/icons/hicolor/64x64/apps" \
         "$USR/share/icons/hicolor/128x128/apps" \
         "$USR/share/icons/hicolor/256x256/apps"

install -m 0755 "$PAYLOAD_DIR/brow" "$USR/lib/brow/brow"
cp -R "$PAYLOAD_DIR/resources" "$USR/lib/brow/resources"
ln -s /usr/lib/brow/brow "$USR/bin/brow"

install -m 0644 "$SCRIPT_DIR/brow.desktop" "$USR/share/applications/brow.desktop"
install -m 0644 "$SCRIPT_DIR/brow.metainfo.xml" "$USR/share/metainfo/brow.metainfo.xml"
ICON_DIR="$SCRIPT_DIR/../icons"
install -m 0644 "$ICON_DIR/brow_64.png"  "$USR/share/icons/hicolor/64x64/apps/brow.png"
install -m 0644 "$ICON_DIR/brow_128.png" "$USR/share/icons/hicolor/128x128/apps/brow.png"
install -m 0644 "$ICON_DIR/brow_256.png" "$USR/share/icons/hicolor/256x256/apps/brow.png"

# MPL-2.0 text lives one level above the servo workspace (repo root).
LICENSE_SRC="$SCRIPT_DIR/../../LICENSE"
[[ -f "$LICENSE_SRC" ]] && install -m 0644 "$LICENSE_SRC" "$USR/share/doc/brow/copyright"

# ---- control ---------------------------------------------------------------
# Depends mirrors servo's Linux runtime requirements (gstreamer, X11/wayland,
# fontconfig, ALSA). libasound2 was renamed libasound2t64 on Ubuntu 24.04 —
# the alternative syntax covers both. Keep in sync with ../nfpm.yaml overrides.
INSTALLED_KB="$(du -sk "$STAGE" | cut -f1)"

cat > "$STAGE/DEBIAN/control" <<EOF
Package: brow
Version: ${VERSION}
Architecture: ${ARCH}
Maintainer: ${MAINTAINER}
Installed-Size: ${INSTALLED_KB}
Depends: libc6 (>= 2.35), libfontconfig1, libfreetype6, libx11-6, libxcb1,
 libxkbcommon0, libxkbcommon-x11-0, libwayland-client0, libegl1, libgl1,
 libgstreamer1.0-0, gstreamer1.0-plugins-base, gstreamer1.0-plugins-good,
 libasound2t64 | libasound2, libdbus-1-3, libudev1
Recommends: gstreamer1.0-plugins-bad, gstreamer1.0-libav
Section: web
Priority: optional
Homepage: https://github.com/salim77007j/brow
Description: Ultra-lightweight, privacy-first web browser on the Servo engine
 brow is a Rust-native browser built on the Servo rendering engine.
 Network-level ad blocking (EasyList/ABP), CNAME cloaking detection,
 anti-fingerprinting, CHIPS partitioned cookies, HTTP/3 + QUIC and
 DNS-over-HTTPS. No telemetry, ever.
EOF

# ---- md5sums (what debhelper's dh_md5sums would produce) -------------------
(
  cd "$STAGE"
  find usr -type f ! -name md5sums -print0 | sort -z \
    | xargs -0 md5sum > DEBIAN/md5sums
)

# ---- build -----------------------------------------------------------------
# mktemp creates the stage with 0700; normalize the data.tar root entry.
chmod 0755 "$STAGE"
DEB="$OUT_DIR/brow_${VERSION}_${ARCH}.deb"
dpkg-deb --build --root-owner-group "$STAGE" "$DEB"
echo "wrote $DEB"
