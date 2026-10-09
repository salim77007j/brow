/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Platform crash capture.
//!
//! Windows (brow, Phase 4.1): unhandled SEH exceptions are captured with
//! `SetUnhandledExceptionFilter` and written as dbghelp minidumps plus a
//! rolling `brow.log` to `%LOCALAPPDATA%\brow\crashes\`. Before this change
//! `install()` was a no-op on Windows, so every crash on owner hardware was
//! invisible (stderr is detached when the shell is launched by double-click).
//! See upstream servo/servo #48110.
//!
//! macOS/Linux: signal handlers print a backtrace to stderr (unchanged).

/// UTC timestamp from the Unix epoch, formatted `YYYY-MM-DDTHH:MM:SSZ`.
/// Pure function so it can be unit-tested on every platform.
pub(crate) fn format_utc_timestamp(epoch_secs: u64) -> String {
    let days = epoch_secs / 86_400;
    let secs_of_day = epoch_secs % 86_400;
    let (year, month, day) = civil_from_days(days as i64);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year,
        month,
        day,
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60
    )
}

/// Howard Hinnant's `civil_from_days` (public domain): converts days since
/// 1970-01-01 to a proleptic Gregorian (year, month, day) without a date library.
fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = (z - era * 146_097) as u64; // [0, 146096]
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365; // [0, 399]
    let year = year_of_era as i64 + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100); // [0, 365]
    let mp = (5 * day_of_year + 2) / 153; // [0, 11]
    let day = (day_of_year - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if month <= 2 { year + 1 } else { year }, month, day)
}

/// Log a panic to the persistent crash log. Windows only; a no-op elsewhere
/// (macOS/Linux report panics through stderr + signal handlers already).
pub(crate) fn log_panic(
    msg: &str,
    thread_name: &str,
    file: Option<&str>,
    line: Option<u32>,
) {
    #[cfg(target_os = "windows")]
    imp::log_panic(msg, thread_name, file, line);
    #[cfg(not(target_os = "windows"))]
    let _ = (msg, thread_name, file, line);
}

/// The directory where crash artifacts (minidumps and `brow.log`) are written,
/// or `None` when crash capture is unavailable or the directory could not be
/// created. Surfaced in the shell's startup log so owner-reported crash files
/// can be located without guessing.
pub(crate) fn crash_log_dir() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        imp::crash_log_dir()
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// Append a timestamped note (build identity, GL driver strings, …) to the
/// persistent crash log. No-op when crash capture is unavailable.
pub(crate) fn log_startup_note(note: &str) {
    #[cfg(target_os = "windows")]
    {
        imp::log_startup_note(note);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = note;
    }
}

#[cfg(target_os = "windows")]
pub fn install() {
    imp::install();
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn install() {}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn install() {
    use std::io::Write;
    use std::sync::atomic;
    use std::thread;

    use libc::siginfo_t;

    use crate::backtrace;

    fn handler(siginfo: &siginfo_t) {
        // Only print crash message and backtrace the first time, to avoid
        // infinite recursion if the printing causes another signal.
        static BEEN_HERE_BEFORE: atomic::AtomicBool = atomic::AtomicBool::new(false);
        if !BEEN_HERE_BEFORE.swap(true, atomic::Ordering::SeqCst) {
            // stderr is unbuffered, so we won’t lose output if we crash later
            // in this handler, and the std::io::stderr() call never allocates.
            // std::io::stdout() allocates the first time it’s called, which in
            // practice will often segfault (see below).
            let stderr = std::io::stderr();
            let mut stderr = stderr.lock();
            let _ = write!(&mut stderr, "Caught signal {}", siginfo.si_signo);
            if let Some(name) = thread::current().name() {
                let _ = write!(&mut stderr, " in thread \"{}\"", name);
            }
            let _ = writeln!(&mut stderr);
            let _ = backtrace::print(&mut stderr);
        }

        // Outside the BEEN_HERE_BEFORE check, we must only call functions we
        // know to be “async-signal-safe”, which includes sigaction(), raise(),
        // and _exit(), but generally doesn’t include anything that allocates.
        // https://pubs.opengroup.org/onlinepubs/9699919799/functions/V2_chap02.html#tag_15_04_03_03
        raise_signal_or_exit_with_error(siginfo.si_signo);
    }

    unsafe {
        signal_hook_registry::register_unchecked(libc::SIGSEGV, handler)
            .expect("Could not register SIGSEGV handler");
        signal_hook_registry::register_unchecked(libc::SIGILL, handler)
            .expect("Could not register SIGILL handler");
        signal_hook_registry::register_unchecked(libc::SIGIOT, handler)
            .expect("Could not register SIGIOT handler");
        signal_hook_registry::register_unchecked(libc::SIGBUS, handler)
            .expect("Could not register SIGBUS handler");
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "android")))]
pub(crate) fn raise_signal_or_exit_with_error(_signal: i32) {
    std::process::exit(1);
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
pub(crate) fn raise_signal_or_exit_with_error(signal: i32) {
    unsafe {
        // Reset the signal to the default action, and reraise the signal.
        // Unlike libc::_exit(sig), which terminates the process normally,
        // this terminates abnormally just like an uncaught signal, allowing
        // mach (or your shell) to distinguish it from an ordinary exit, and
        // allows your kernel to make a core dump if configured to do so.
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = libc::SIG_DFL;
        libc::sigaction(signal, &action, std::ptr::null_mut());
        libc::raise(signal);
    }
}

#[cfg(target_os = "windows")]
mod imp {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::os::windows::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::sync::OnceLock;

    use windows_sys::Win32::Foundation::{
        CloseHandle, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE, FILETIME,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL,
    };
    use windows_sys::Win32::System::Diagnostics::Debug::{
        MiniDumpWriteDump, SetUnhandledExceptionFilter, EXCEPTION_EXECUTE_HANDLER,
        EXCEPTION_POINTERS, MINIDUMP_EXCEPTION_INFORMATION, MINIDUMP_TYPE,
    };
    use windows_sys::Win32::System::SystemInformation::GetSystemTimeAsFileTime;
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId,
    };

    /// dbghelp.h `MINIDUMP_TYPE` flags:
    /// `MiniDumpWithIndirectlyReferencedMemory` (0x40) |
    /// `MiniDumpWithUnloadedModules` (0x20).
    /// Enough to diagnose ANGLE/WGPU/driver crashes without multi-GB
    /// full-memory dumps on owner hardware.
    const MINIDUMP_TYPE_FLAGS: MINIDUMP_TYPE = 0x60;

    /// Roll `brow.log` over once it exceeds this size, keeping a single
    /// `.1` generation so the crash folder stays small and bounded.
    const MAX_LOG_BYTES: u64 = 1 << 20;

    /// 100 ns ticks between 1601-01-01 and the Unix epoch (FILETIME basis).
    const FILETIME_EPOCH_OFFSET_100NS: u64 = 11_644_473_600_000_000;

    fn crash_dir() -> Option<PathBuf> {
        static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
        DIR.get_or_init(|| {
            let base = std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("TEMP").map(PathBuf::from))
                .unwrap_or_else(std::env::temp_dir);
            let dir = base.join("brow").join("crashes");
            fs::create_dir_all(&dir).ok()?;
            Some(dir)
        })
        .clone()
    }

    pub(super) fn crash_log_dir() -> Option<PathBuf> {
        crash_dir()
    }

    fn append_to_log(text: &str) -> Option<()> {
        let dir = crash_dir()?;
        let log = dir.join("brow.log");
        if let Ok(metadata) = fs::metadata(&log) {
            if metadata.len() > MAX_LOG_BYTES {
                let _ = fs::remove_file(dir.join("brow.log.1"));
                let _ = fs::rename(&log, dir.join("brow.log.1"));
            }
        }
        let mut file = OpenOptions::new().create(true).append(true).open(&log).ok()?;
        writeln!(file, "{}", text).ok()?;
        Some(())
    }

    fn now_epoch_secs() -> u64 {
        let mut filetime = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        // SAFETY: plain getter into a local struct.
        unsafe { GetSystemTimeAsFileTime(&mut filetime) };
        let ticks_100ns =
            ((filetime.dwHighDateTime as u64) << 32) | filetime.dwLowDateTime as u64;
        (ticks_100ns - FILETIME_EPOCH_OFFSET_100NS) / 10_000_000
    }

    pub(super) fn log_startup_note(note: &str) {
        let stamp = super::format_utc_timestamp(now_epoch_secs());
        let _ = append_to_log(&format!("{} STARTUP {}", stamp, note));
    }

    pub(super) fn log_panic(msg: &str, thread_name: &str, file: Option<&str>, line: Option<u32>) {
        let stamp = super::format_utc_timestamp(now_epoch_secs());
        let location = match (file, line) {
            (Some(file), Some(line)) => format!("{}:{}", file, line),
            (Some(file), None) => file.to_string(),
            _ => "<unknown>".to_string(),
        };
        let _ = append_to_log(&format!(
            "{} PANIC thread={} at={} msg={}",
            stamp, thread_name, location, msg
        ));
    }

    pub(super) fn install() {
        // Create the crash directory eagerly and record what is running, so a
        // minidump without symbols on the analyst’s machine still identifies
        // the build (version, git sha, profile) it came from.
        let build_profile = option_env!("BROW_BUILD_PROFILE").unwrap_or(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        });
        let dir_note = crash_dir()
            .map(|dir| dir.display().to_string())
            .unwrap_or_else(|| "<unavailable>".to_string());
        log_startup_note(&format!(
            "{} | profile={} | crash-capture=active | dump-dir={}",
            crate::VERSION,
            build_profile,
            dir_note
        ));

        // SAFETY: registering the process-wide unhandled-exception filter.
        // The previous filter (if any) is intentionally not called: returning
        // EXCEPTION_EXECUTE_HANDLER terminates the process after we have
        // written the dump, so a chained filter would never run anyway.
        unsafe {
            SetUnhandledExceptionFilter(Some(unhandled_exception_filter));
        }
    }

    unsafe extern "system" fn unhandled_exception_filter(
        exception_pointers: *const EXCEPTION_POINTERS,
    ) -> i32 {
        write_minidump(exception_pointers);
        EXCEPTION_EXECUTE_HANDLER
    }

    /// Write a dbghelp minidump for the exception the OS just handed us.
    ///
    /// This runs inside the SEH filter on the crashing thread. It allocates
    /// (path formatting, and `MiniDumpWriteDump` itself), which is the standard
    /// tradeoff made by in-process crash handlers that use dbghelp; a strictly
    /// async-safe handler is impossible while calling `MiniDumpWriteDump`.
    fn write_minidump(exception_pointers: *const EXCEPTION_POINTERS) -> Option<()> {
        let dir = crash_dir()?;
        let path = dir.join(format!("brow-minidump-{}.dmp", now_epoch_secs()));
        let mut wide_path: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        wide_path.shrink_to_fit();

        // SAFETY: CreateFileW with a NUL-terminated wide path we just built.
        let handle: HANDLE = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                GENERIC_WRITE,
                0, // exclusive: WER/debuggers must not read a half-written dump
                std::ptr::null(),
                CREATE_ALWAYS,
                FILE_ATTRIBUTE_NORMAL,
                std::ptr::null_mut(),
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            return None;
        }

        let exception_information = MINIDUMP_EXCEPTION_INFORMATION {
            ThreadId: unsafe { GetCurrentThreadId() },
            // The dump writer runs in-process, so the pointers are already in
            // this process's address space (ClientPointers = FALSE).
            ExceptionPointers: exception_pointers as *mut EXCEPTION_POINTERS,
            ClientPointers: 0,
        };
        // SAFETY: handles and pointers are valid; this is the documented
        // dbghelp call for in-process minidump capture.
        let written = unsafe {
            MiniDumpWriteDump(
                GetCurrentProcess(),
                GetCurrentProcessId(),
                handle,
                MINIDUMP_TYPE_FLAGS,
                &exception_information,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        // SAFETY: we own this handle.
        unsafe {
            CloseHandle(handle);
        }
        (written != 0).then_some(())
    }
}

#[cfg(test)]
mod tests {
    use super::{civil_from_days, format_utc_timestamp};

    #[test]
    fn epoch_is_1970_01_01() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(format_utc_timestamp(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn day_before_epoch() {
        assert_eq!(civil_from_days(-1), (1969, 12, 31));
    }

    #[test]
    fn leap_day_2024() {
        // 2024-01-01T00:00:00Z = 1704067200; +59 days lands on 2024-02-29.
        assert_eq!(format_utc_timestamp(1_704_067_200 + 59 * 86_400), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn october_2026_matches_wall_clock_reference() {
        // 2026-01-01T00:00:00Z = 1767225600; Jan–Sep = 273 days, so Oct 9 is
        // day 281 of the year.
        assert_eq!(format_utc_timestamp(1_767_225_600 + 281 * 86_400), "2026-10-09T00:00:00Z");
    }

    #[test]
    fn time_of_day_components() {
        assert_eq!(format_utc_timestamp(86_399), "1970-01-01T23:59:59Z");
        assert_eq!(format_utc_timestamp(86_400 + 3_600), "1970-01-02T01:00:00Z");
    }
}
