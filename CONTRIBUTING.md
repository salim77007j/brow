# Contributing to brow

Thanks for helping build a lighter, faster, more private web. This document sets the
engineering standards for the project. They are intentionally strict — brow aims for
commercial-grade quality.

## Ground rules

1. **No fake implementations.** No stubs, no mockups, no dead code, no
   `todo!()`-shipped-to-prod. Every feature must be real, functional, and tested.
2. **Memory safety first.** New `unsafe` blocks require a justification comment and
   must be justified in the PR description. Prefer safe abstractions; when wrapping
   FFI (SpiderMonkey, system libs), contain the unsafety behind a small audited API.
3. **Document everything.** Every non-obvious decision gets a comment or a `docs/` note.
   Every phase gets a `PHASE_N_REPORT.md`.
4. **Test continuously.** Do not land untested code. Unit tests live next to the code
   (`cargo test -p <crate>`); web-exposed behavior needs a
   [web-platform-tests](https://web-platform-tests.org/) case in `servo/tests/wpt/`.

## Toolchain

- Rust is pinned by `servo/rust-toolchain.toml` (currently **1.97.1**) — always build
  inside `servo/` so rustup selects the right toolchain. Never bump it casually:
  it must match the upstream Servo pin until we deliberately diverge.
- Rust edition 2024, workspace `rust-version` 1.88.0.
- `cargo fmt` is mandatory (`servo/rustfmt.toml` is the style source of truth).
- `cargo clippy --workspace` must be clean for brow-owned crates.

## Coding standards

- **Style**: follow servo's `rustfmt.toml`. Do not hand-format.
- **Naming**: `snake_case` for functions/fields, `CamelCase` for types,
  `SCREAMING_SNAKE_CASE` for consts. Browser-facing acronyms (DOM, HTML, URL) follow
  their DOM-spec casing in public API names (`to_string()` vs `innerHTML`).
- **Error handling**: `Result` with specific error types at module boundaries.
  `unwrap()`/`expect()` are allowed only in (a) tests, (b) genuinely infallible paths
  with a comment proving it, or (c) startup code where panic = honest failure.
- **Licensing**: every new file carries the MPL-2.0 header:

  ```rust
  /* This Source Code Form is subject to the terms of the Mozilla Public
   * License, v. 2.0. If a copy of the MPL was not distributed with this
   * file, You can obtain one at https://mozilla.org/MPL/2.0/. */
  ```

- **Logging**: use `log` macros (`debug!`, `info!`, `warn!`, `error!`). Never `println!`
  in library code. Privacy rule: never log URLs, cookies, or user identifiers at `info`
  or above.

## Commits and pull requests

- Conventional, phase-prefixed subjects: `phase(2): add HTTP/3 transport via quinn`.
- One logical change per commit; the body explains *why*, not *what*.
- PRs must state: motivation, approach, testing performed, and any perf impact
  (with numbers — we measure everything).
- CI must be green before merge. Red CI on `main` is treated as a build outage:
  fix-forward or revert within the hour.

## Testing standards

| Layer | Where | Command |
|-------|-------|---------|
| Unit | beside the code in `servo/components/*` | `cd servo && ./mach test-unit` |
| WPT (web-exposed) | `servo/tests/wpt/` | `cd servo && ./mach test-wpt --release` |
| Manual smoke | headless render + screenshot | see `docs/BUILDING.md` |

Performance changes require a before/after measurement in the PR description.

## Dependency policy

- 2026-current, actively maintained crates only. No abandoned dependencies.
- Every new dependency is justified in the PR: what it does, why an existing one can't,
  its maintenance status, and its transitive-deployment cost (binary size).
- Prefer crates already in Servo's tree to avoid duplicate stacks.

## Code of conduct

Be excellent to each other. Discussion stays technical; harassment gets you banned.

## Licensing

By contributing you agree your contributions are licensed under the MPL-2.0.
