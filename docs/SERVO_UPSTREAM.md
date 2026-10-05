# Servo Upstream Provenance

This directory (`servo/`) contains a **vendored, in-tree copy of the official
Servo v0.6.0 (LTS) release source**, lightly filtered as described below. brow
enhances the engine directly in this tree (phases 2–4), so the engine and shell
evolve atomically in one repository.

## Upstream identity

| Field | Value |
|-------|-------|
| Repository | https://github.com/servo/servo |
| Release | **v0.6.0 (LTS)** — the latest stable release at project start |
| Published | 2026-09-29 |
| Release tag commit | `c78d2c206f80a1c8b67eefa97f773bba513205d3` |
| Verified binary | `servo-x86_64-linux-gnu.tar.gz`, SHA-256 `ad951ede1a1a73899b822c9464f6bdb3ec25b531b27cd806671d79ac8b6a60d0` (matches official `.sha256` asset) |
| Upstream license | MPL-2.0 (preserved in-tree) |

`servoshell --version` reports `Servo 0.6.0-c78d2c206`, confirming the vendored
source matches the released binary commit.

## What was vendored

- The complete upstream source tree at tag `v0.6.0`, **excluding only `.git/`**.
- Web-platform-tests is **in-tree upstream as of this release** (`tests/wpt/`,
  ~1.2 GB) and is vendored too, so conformance testing works from a plain clone.
- Upstream git submodules: **none remain** at this tag (`.gitmodules` is empty —
  upstream merged WPT into the monorepo).

## Verifying / refreshing upstream

```bash
git ls-remote --tags https://github.com/servo/servo.git refs/tags/v0.6.0
# expect: c78d2c206f80a1c8b67eefa97f773bba513205d3
```

To diff brow's engine changes against upstream in the future:

```bash
git clone --depth 1 --branch v0.6.0 https://github.com/servo/servo /tmp/servo-up
diff -ru /tmp/servo-up/components servo/components | less
```

## Policy

- Never re-vendor over local changes blindly; keep brow patches reviewable
  (phase reports list every engine file we touch).
- Engine upgrades (e.g. future servo stable) are a deliberate, documented event —
  re-run the full WPT subset + headless smoke before/after.
