# Upstream PR draft: Windows crash handler (answers servo/servo#48110)

**Target:** servo/servo, `ports/servoshell/crash_handler.rs`
**Status:** implemented and shipping in brow `v0.7-rebuild` (commit
`0324d9f6f`); offering upstream.

---

## Title

servoshell: implement Windows crash capture (SEH minidumps + rolling crash log)

## Summary

`crash_handler::install()` is a no-op on Windows (`#[cfg(not(any(macos,
linux, android)))] pub fn install() {}`), so every segfault in a release
servoshell on Windows is invisible: stderr is detached when the shell is
launched by double-click (`FreeConsole` in main.rs), and there is no
minidump. This PR implements the Windows arm:

- `SetUnhandledExceptionFilter` writes a dbghelp minidump to
  `%LOCALAPPDATA%\brow\crashes\brow-minidump-<unix-epoch>.dmp`
  (`MiniDumpWithIndirectlyReferencedMemory | MiniDumpWithUnloadedModules`
  — enough to diagnose ANGLE/WGPU/driver faults without multi-GB
  full-memory dumps).
- A rolling `brow.log` next to the dumps records: startup (version, git
  sha, build profile, dump dir), the GL driver strings
  (`GL_VERSION`/`GL_RENDERER` captured right after the egui context is
  created), and every Rust panic (via the existing panic hook).
- The build profile is baked in via `build.rs`
  (`cargo:rustc-env=BROW_BUILD_PROFILE={profile}`) so dumps without local
  symbols still identify the configuration.
- macOS/Linux signal handling is unchanged.

## Notes for reviewers

- `MiniDumpWriteDump` is used through windows-sys' static `windows_link!`
  binding (dbghelp.dll); feature-gated on
  `Win32_Security` + `Win32_System_Memory` in addition to the Debug module.
- The SEH filter allocates (path formatting, and dbghelp itself) — the
  standard in-process tradeoff; a strictly async-safe handler is impossible
  while calling `MiniDumpWriteDump`.
- Returning `EXCEPTION_EXECUTE_HANDLER` terminates without WER; the
  previous filter is not chained (it could not run after termination
  anyway).
- Motivated by servo/servo#48109 triage: without minidumps, release-build
  segfaults on Windows (0xC0000005, Script thread) are undebuggable on
  machines without a dev environment.
