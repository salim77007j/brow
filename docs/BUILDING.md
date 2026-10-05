# Building brow's engine (vendored Servo)

This guide covers building the vendored engine in `servo/` on Linux x86_64 today,
with notes for the Windows/Android targets that Servo upstream supports (brow's own
packaging targets arrive in Phase 5).

## 1. Toolchain

- Rust is pinned to **1.97.1** by `servo/rust-toolchain.toml`. With rustup installed,
  simply running `cargo`/`mach` inside `servo/` auto-installs the pinned toolchain
  (including clippy, rustfmt, rust-src, llvm-tools).
- Python 3.10+ (mach is Python-based).

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
cd servo
./mach bootstrap      # Debian/Ubuntu/Fedora: installs system deps (uses sudo)
```

## 2. System dependencies (Debian 13 / Ubuntu 24.04)

`./mach bootstrap` handles this automatically. The essential set, for reference:

```
build-essential cmake clang llvm-dev libclang-dev python3 python3-pip python3-venv
pkg-config ccache zip m4 perl
libgl1 libegl1 libglx0 libgbm1 libgl1-mesa-dri libegl-mesa0 libdrm2
libx11-dev libx11-xcb-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev
libxcb-xfixes0-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev
libfontconfig1-dev libfreetype6-dev
libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev libgstreamer-plugins-bad1.0-dev
gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad
gstreamer1.0-gl gstreamer1.0-libav
libudev-dev libdbus-1-dev libunwind-dev
```

> **Media-note:** the Servo binary dynamically links
> `libgstplay-1.0.so.0`, `libgstwebrtc-1.0.so.0` and `libgstgl-1.0.so.0`.
> On Debian 13 (trixie) these sonames ship inside `libgstreamer-plugins-bad1.0-0`
> and `libgstreamer-gl1.0-0` (package names were renamed upstream; the sonames are
> unchanged). On Ubuntu 24.04 they are `libgstreamer-plugins-bad1.0-0` too.

## 3. Build

```bash
cd servo
./mach build --release        # optimized; what CI builds
./mach build --dev            # faster build, slower binary
./mach run --release https://example.com
```

First release build: ~2 h on a 4-core CI runner, ~10–20 GB of `target/` disk,
peak RAM during linking 8+ GB (raise `RUST_MIN_STACK` if linking OOMs).

## 4. Headless smoke test (no display needed)

The binary supports self-contained headless rendering with screenshot output:

```bash
./target/release/servo -z -o /tmp/shot.png https://example.com
```

`-z/--headless` renders offscreen; `-o` writes the screenshot (format from the
extension) and the process stays in its event loop until killed — CI wraps it in
`timeout` and asserts the PNG exists.

### Headless in minimal containers (what we learned)

If you run in a stripped-down container (no sudo), the full runtime GL/EGL/media
stack can be satisfied **user-space** with `dpkg -x`:

1. Extract `libegl1`, `libegl-mesa0`, `libgstreamer-plugins-bad1.0-0`,
   `libgstreamer-gl1.0-0`, `libgudev-1.0-0` (+ `xauth`) into a prefix.
2. Point the loader and glvnd at it:

```bash
export LD_LIBRARY_PATH="$PREFIX/usr/lib/x86_64-linux-gnu"
export __EGL_VENDOR_LIBRARY_FILENAMES="$PREFIX/usr/share/glvnd/egl_vendor.d/50_mesa.json"
export XDG_RUNTIME_DIR=/tmp/xdg LIBGL_ALWAYS_SOFTWARE=1
Xvfb :99 -screen 0 1280x800x24 &
DISPLAY=:99 ./servoshell -z -o /tmp/shot.png https://example.com
```

Without `__EGL_VENDOR_LIBRARY_FILENAMES`, glvnd finds no EGL vendor and surfman
aborts with `Failed to create WR surfman` / a zero-config assertion inside
`surfman::x11::connection`. `LIBGL_ALWAYS_SOFTWARE=1` forces the mesa swrast/llvmpipe
path on machines without a GPU.

## 5. Testing

```bash
cd servo
./mach test-unit                      # crate unit tests
./mach test-tidy                      # style/license checks
./mach test-wpt --release --pref=...  # web-platform-tests (in-tree at tests/wpt/)
./mach run --dev --webdriver=7000     # WebDriver endpoint for automation
```

## 6. Binary location

```
servo/target/release/servo        # engine + shell binary (servoshell)
servo/target/release/resources/   # runtime resources (user-agent stylesheet, prefs)
```
