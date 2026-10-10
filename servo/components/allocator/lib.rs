/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Selecting the default global allocator for Servo, and exposing common
//! allocator introspection APIs for memory profiling.

use std::os::raw::c_void;

#[cfg(not(feature = "allocation-tracking"))]
#[global_allocator]
static ALLOC: Allocator = Allocator;

#[cfg(feature = "allocation-tracking")]
#[global_allocator]
static ALLOC: crate::tracking::AccountingAlloc<Allocator> =
    crate::tracking::AccountingAlloc::with_allocator(Allocator);

#[cfg(feature = "allocation-tracking")]
mod tracking;

pub fn is_tracking_unmeasured() -> bool {
    cfg!(feature = "allocation-tracking")
}

pub fn dump_unmeasured(_writer: impl std::io::Write) {
    #[cfg(feature = "allocation-tracking")]
    ALLOC.dump_unmeasured_allocations(_writer);
}

pub struct HeapReport {
    pub path: &'static str,
    pub size: Option<usize>,
}

pub use crate::platform::*;

/// brow (phase 6.2): apply the platform's resident-memory tuning.
///
/// jemalloc (Linux build) ships with `background_thread` OFF: freed pages
/// stay mapped and are only purged as a side effect of later allocation
/// activity on the arena. A browser's steady state (page churn, image
/// decode buffers, JS heaps) leaves tens of MB mapped-but-dirty per
/// process that nothing ever touches again, so the purge never runs.
/// Enabling the background thread + shortening the dirty/muzzy decays
/// returns those pages to the OS without changing application behavior.
///
/// No-op on platforms without runtime-tunable allocators (Windows
/// HeapAlloc, plain System fallback). Must be called once per process,
/// as early as possible; safe to call again (idempotent values).
pub fn tune_residency() {
    crate::platform::tune_residency();
}

type EnclosingSizeFn = unsafe extern "C" fn(*const c_void) -> usize;

/// # Safety
/// No restrictions. The passed pointer is never dereferenced.
/// This function is only marked unsafe because the MallocSizeOfOps APIs
/// requires an unsafe function pointer.
#[cfg(feature = "allocation-tracking")]
unsafe extern "C" fn enclosing_size_impl(ptr: *const c_void) -> usize {
    let (adjusted, size) = crate::ALLOC.enclosing_size(ptr);
    if size != 0 {
        crate::ALLOC.note_allocation(adjusted, size);
    }
    size
}

#[expect(non_upper_case_globals)]
#[cfg(feature = "allocation-tracking")]
pub static enclosing_size: Option<EnclosingSizeFn> = Some(crate::enclosing_size_impl);

#[expect(non_upper_case_globals)]
#[cfg(not(feature = "allocation-tracking"))]
pub static enclosing_size: Option<EnclosingSizeFn> = None;

#[cfg(all(
    feature = "use-jemalloc",
    feature = "use-mimalloc",
    not(any(windows, target_env = "ohos"))
))]
compile_error!(
    "features `use-jemalloc` and `use-mimalloc` are mutually exclusive: \
     pick exactly one global allocator"
);

#[cfg(all(feature = "use-jemalloc", not(any(windows, target_env = "ohos"))))]
mod platform {
    use std::ffi::CStr;
    use std::mem::size_of_val;
    use std::os::raw::c_void;
    use std::ptr;

    use tikv_jemalloc_sys::mallctl;
    pub use tikv_jemallocator::Jemalloc as Allocator;

    pub fn heap_reports() -> Vec<crate::HeapReport> {
        vec![
            crate::HeapReport {
                path: "jemalloc-heap-allocated",
                size: jemalloc_stat(c"stats.allocated"),
            },
            crate::HeapReport {
                path: "jemalloc-heap-active",
                size: jemalloc_stat(c"stats.active"),
            },
            crate::HeapReport {
                path: "jemalloc-heap-mapped",
                size: jemalloc_stat(c"stats.mapped"),
            },
        ]
    }

    fn jemalloc_stat(value_name: &CStr) -> Option<usize> {
        // Before we request the measurement of interest, we first send an "epoch"
        // request. Without that jemalloc gives cached statistics(!) which can be
        // highly inaccurate.
        let epoch_c_name = c"epoch";
        let mut epoch: u64 = 0;
        let epoch_ptr = &raw mut epoch;
        let mut epoch_len = size_of_val(&epoch);

        let mut value: usize = 0;
        let value_ptr = &raw mut value;
        let mut value_len = size_of_val(&value);

        // Using the same values for the `old` and `new` parameters is enough
        // to get the statistics updated.
        let rv = unsafe {
            mallctl(
                epoch_c_name.as_ptr(),
                epoch_ptr.cast(),
                &mut epoch_len,
                epoch_ptr.cast(),
                epoch_len,
            )
        };
        if rv != 0 {
            return None;
        }

        let rv = unsafe {
            mallctl(
                value_name.as_ptr(),
                value_ptr.cast(),
                &mut value_len,
                ptr::null_mut(),
                0,
            )
        };
        if rv != 0 {
            return None;
        }

        Some(value)
    }

    /// Get the size of a heap block.
    ///
    /// # Safety
    ///
    /// Passing a non-heap allocated pointer to this function results in undefined behavior.
    pub unsafe extern "C" fn usable_size(ptr: *const c_void) -> usize {
        let size = unsafe { tikv_jemallocator::usable_size(ptr) };
        #[cfg(feature = "allocation-tracking")]
        crate::ALLOC.note_allocation(ptr, size);
        size
    }

    /// Memory allocation APIs compatible with libc
    pub mod libc_compat {
        pub use tikv_jemalloc_sys::{free, malloc, realloc};
    }

    /// brow (phase 6.2): jemalloc residency tuning (see crate-level docs).
    ///
    /// - `background_thread=true`: an internal jemalloc thread purges
    ///   decayed pages even when the allocating threads go idle.
    /// - `dirty_decay_ms=5000`, `muzzy_decay_ms=5000`: freed pages become
    ///   purgeable after 5 s instead of the 10 s defaults, halving the
    ///   window dirty memory is pinned.
    ///
    /// Values are applied best-effort: a mallctl failure (unsupported
    /// option on the linked jemalloc build) is logged and skipped — the
    /// defaults are never worse than upstream behavior.
    pub fn tune_residency() {
        fn set_bool(name: &CStr, value: bool) {
            let mut old: bool = false;
            let mut old_len = size_of_val(&old);
            let new: [u8; 1] = [value as u8];
            let rv = unsafe {
                mallctl(
                    name.as_ptr(),
                    (&raw mut old).cast(),
                    &mut old_len,
                    new.as_ptr().cast(),
                    size_of_val(&new),
                )
            };
            if rv != 0 {
                log::warn!("jemalloc mallctl({name:?}, {value}) failed: {rv}");
            }
        }
        fn set_size_t(name: &CStr, value: usize) {
            let mut old: usize = 0;
            let mut old_len = size_of_val(&old);
            let new: usize = value;
            let rv = unsafe {
                mallctl(
                    name.as_ptr(),
                    (&raw mut old).cast(),
                    &mut old_len,
                    (&raw const new).cast(),
                    size_of_val(&new),
                )
            };
            if rv != 0 {
                log::warn!("jemalloc mallctl({name:?}, {value}) failed: {rv}");
            } else {
                log::debug!("jemalloc {name:?} set to {value} (was {old})");
            }
        }

        set_bool(c"background_thread", true);
        set_size_t(c"dirty_decay_ms", 5000);
        set_size_t(c"muzzy_decay_ms", 5000);
    }
}

// brow: mimalloc platform module. mimalloc's tight size-class spacing and
// eager page release give a lower resident footprint than jemalloc for the
// many-small-object allocation profile of a browser with many live tabs
// (Phase 3 resource strategy). See docs/BUILD_PGO.md and the Phase 3 report.
#[cfg(all(
    feature = "use-mimalloc",
    not(feature = "use-jemalloc"),
    not(any(windows, target_env = "ohos"))
))]
mod platform {
    use std::os::raw::c_void;
    use std::ptr;

    /// mimalloc provides no `GlobalAlloc` implementation itself in
    /// `libmimalloc-sys`, so we implement a minimal one over the C API.
    pub struct MiMallocAllocator;

    unsafe impl std::alloc::GlobalAlloc for MiMallocAllocator {
        unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
            if layout.align() > MI_MAX_ALIGN {
                return ptr::null_mut();
            }
            unsafe { libmimalloc_sys::mi_malloc(layout.size()) as *mut u8 }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _layout: std::alloc::Layout) {
            unsafe { libmimalloc_sys::mi_free(ptr as *mut c_void) }
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
            if layout.align() > MI_MAX_ALIGN {
                return ptr::null_mut();
            }
            unsafe { libmimalloc_sys::mi_realloc(ptr as *mut c_void, new_size) as *mut u8 }
        }

        unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
            if layout.align() > MI_MAX_ALIGN {
                return ptr::null_mut();
            }
            unsafe { libmimalloc_sys::mi_zalloc(layout.size()) as *mut u8 }
        }
    }

    const MI_MAX_ALIGN: usize = 16;

    // `libmimalloc-sys` does not bind `mi_usable_size`, but the symbol is
    // exported by the mimalloc library it links, so declare it directly.
    unsafe extern "C" {
        fn mi_usable_size(p: *const c_void) -> usize;
    }

    pub use MiMallocAllocator as Allocator;

    pub fn heap_reports() -> Vec<crate::HeapReport> {
        // mimalloc exposes stats only via its verbose printer; programmatic
        // per-counter extraction is not available through libmimalloc-sys.
        Vec::new()
    }

    /// Get the size of a heap block.
    ///
    /// # Safety
    ///
    /// Passing a non-heap allocated pointer to this function results in undefined behavior.
    pub unsafe extern "C" fn usable_size(ptr: *const c_void) -> usize {
        let size = unsafe { mi_usable_size(ptr) };
        #[cfg(feature = "allocation-tracking")]
        crate::ALLOC.note_allocation(ptr, size);
        size
    }

    /// Memory allocation APIs compatible with libc
    pub mod libc_compat {
        pub use libmimalloc_sys::{mi_free as free, mi_malloc as malloc, mi_realloc as realloc};
    }

    /// brow (phase 6.2): mimalloc already releases pages eagerly (tight
    /// size classes, short purge delay — the reason this backend exists),
    /// and `libmimalloc-sys` does not bind the option setters, so there is
    /// nothing to tune here. No-op by design.
    pub fn tune_residency() {}
}

#[cfg(all(
    not(windows),
    any(
        target_env = "ohos",
        not(any(feature = "use-jemalloc", feature = "use-mimalloc"))
    )
))]
mod platform {
    pub use std::alloc::System as Allocator;
    use std::os::raw::c_void;

    /// Get the size of a heap block.
    ///
    /// # Safety
    ///
    /// Passing a non-heap allocated pointer to this function results in undefined behavior.
    pub unsafe extern "C" fn usable_size(ptr: *const c_void) -> usize {
        #[cfg(target_vendor = "apple")]
        unsafe {
            let size = libc::malloc_size(ptr);
            #[cfg(feature = "allocation-tracking")]
            crate::ALLOC.note_allocation(ptr, size);
            size
        }

        #[cfg(not(target_vendor = "apple"))]
        unsafe {
            let size = libc::malloc_usable_size(ptr as *mut _);
            #[cfg(feature = "allocation-tracking")]
            crate::ALLOC.note_allocation(ptr, size);
            size
        }
    }

    pub mod libc_compat {
        pub use libc::{free, malloc, realloc};
    }

    pub fn heap_reports() -> Vec<crate::HeapReport> {
        Vec::new()
    }

    /// brow (phase 6.2): the System allocator has no runtime tuning knobs.
    pub fn tune_residency() {}
}

#[cfg(windows)]
mod platform {
    pub use std::alloc::System as Allocator;
    use std::os::raw::c_void;

    use windows_sys::Win32::Foundation::FALSE;
    use windows_sys::Win32::System::Memory::{GetProcessHeap, HeapSize, HeapValidate};

    /// Get the size of a heap block.
    ///
    /// # Safety
    ///
    /// Passing a non-heap allocated pointer to this function results in undefined behavior.
    pub unsafe extern "C" fn usable_size(mut ptr: *const c_void) -> usize {
        unsafe {
            let heap = GetProcessHeap();

            if HeapValidate(heap, 0, ptr) == FALSE {
                ptr = *(ptr as *const *const c_void).offset(-1)
            }

            let size = HeapSize(heap, 0, ptr) as usize;
            #[cfg(feature = "allocation-tracking")]
            crate::ALLOC.note_allocation(ptr, size);
            size
        }
    }

    pub fn heap_reports() -> Vec<crate::HeapReport> {
        Vec::new()
    }

    /// brow (phase 6.2): the Windows HeapAlloc-based System allocator has
    /// no useful runtime tuning knobs (LFH sizing is process-wide and
    /// heuristic). No-op by design.
    pub fn tune_residency() {}
}
