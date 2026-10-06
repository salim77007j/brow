# brow — reference RPM spec (Phase 5)
#
# This spec is for distro packagers and Fedora/openSUSE users who prefer the
# native rpmbuild workflow over the canonical nfpm path (packaging/nfpm.yaml,
# which is what release CI uses). It consumes the portable tarball artifact:
#
#   rpmbuild -bb packaging/rpm/brow.spec \
#     --define "brow_version 0.5.0" \
#     --define "brow_payload /abs/path/to/staged/payload"
#
# Payload contract (same as build-deb.sh): <payload>/brow (ELF 0755) and
# <payload>/resources/ tree. Installed layout matches nfpm.yaml.

%bcond_without check

Name:           brow
Version:        %{brow_version}
Release:        1%{?dist}
Summary:        Ultra-lightweight, privacy-first web browser on the Servo engine

License:        MPL-2.0
URL:            https://github.com/salim77007j/brow
# Local-build spec: payload comes from a staged directory, not a remote source.
Source0:        brow-%{version}-x86_64-linux-gnu.tar.gz
Source1:        brow.desktop
Source2:        brow.metainfo.xml
Source3:        brow_64.png
Source4:        brow_128.png
Source5:        brow_256.png
BuildArch:      x86_64

# Runtime requirements mirror nfpm.yaml (keep the two lists in sync).
Requires:       glibc >= 2.35
Requires:       fontconfig, freetype
Requires:       libX11, libxkbcommon, libxkbcommon-x11, wayland
Requires:       mesa-libEGL, mesa-libGL
Requires:       gstreamer1, gstreamer1-plugins-base, gstreamer1-plugins-good
Requires:       alsa-lib, dbus-libs, systemd-libs
Recommends:     gstreamer1-plugins-bad-free

%description
brow is a Rust-native web browser built on the Servo rendering engine:
parallel Stylo CSS + WebRender compositor, network-level ad blocking
(EasyList/ABP), CNAME cloaking detection, anti-fingerprinting, CHIPS
partitioned cookies, HTTP/3 + QUIC and DNS-over-HTTPS. No telemetry, ever.

%prep
# No archive unpacking: the payload directory is passed straight through.
# Source0 is declared only to satisfy rpmbuild's expectations for %files
# provenance; local builds reference %{brow_payload} directly.

%build
# The engine is built out-of-band (cargo build --release -p brow-shell).

%install
install -D -m 0755 "%{brow_payload}/brow"        %{buildroot}/usr/lib/brow/brow
mkdir -p                                          %{buildroot}/usr/lib/brow
cp -R "%{brow_payload}/resources"                 %{buildroot}/usr/lib/brow/resources
install -D -m 0644 %{SOURCE1}                     %{buildroot}/usr/share/applications/brow.desktop
install -D -m 0644 %{SOURCE2}                     %{buildroot}/usr/share/metainfo/brow.metainfo.xml
install -D -m 0644 %{SOURCE3}                     %{buildroot}/usr/share/icons/hicolor/64x64/apps/brow.png
install -D -m 0644 %{SOURCE4}                     %{buildroot}/usr/share/icons/hicolor/128x128/apps/brow.png
install -D -m 0644 %{SOURCE5}                     %{buildroot}/usr/share/icons/hicolor/256x256/apps/brow.png
install -D -m 0644 %{_topdir}/LICENSE             %{buildroot}/usr/share/licenses/brow/LICENSE 2>/dev/null || true
mkdir -p                                          %{buildroot}/usr/bin
ln -sf /usr/lib/brow/brow                         %{buildroot}/usr/bin/brow

%files
/usr/lib/brow/brow
/usr/lib/brow/resources
/usr/bin/brow
/usr/share/applications/brow.desktop
/usr/share/metainfo/brow.metainfo.xml
/usr/share/icons/hicolor/64x64/apps/brow.png
/usr/share/icons/hicolor/128x128/apps/brow.png
/usr/share/icons/hicolor/256x256/apps/brow.png
%license LICENSE

%changelog
* Tue Oct 06 2026 brow engineering <salim77007j@users.noreply.github.com> - 0.5.0-1
- First packaged release: AppImage/.deb/.rpm distribution engineering (Phase 5).
