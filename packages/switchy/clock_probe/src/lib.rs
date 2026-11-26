//! Counts the real clock reads a simulation run makes.
//!
//! Loaded into a simulation's process (Linux: `LD_PRELOAD`; macOS: `DYLD_INSERT_LIBRARIES`), it
//! stands in for the C library's clock functions: `clock_gettime`, `gettimeofday` and `time`
//! (Rust's `Instant` and `SystemTime` use `clock_gettime`; not `mach_absolute_time`, which the
//! system itself calls while making a thread's storage, before the probe could tell). Every call is passed on to the real function, and those
//! made by a thread the program says is in a simulation run are counted by call site. A run's
//! clock is the simulator's: a real read inside a run is one the determinism checks did not
//! see (a dependency calling the C library itself, say), harmless only if nothing it returns
//! reaches the run (a log line's timestamp).
//!
//! It is never linked: the program finds its functions at run time (`dlsym`) and drives it
//! with them. [`switchy_clock_probe_install`] gives it the test for "in a run",
//! [`switchy_clock_probe_report`] hands over each call site counted, with its call stack, and
//! [`switchy_clock_probe_reset`] forgets them. simvar does all three when
//! `SIMULATOR_CLOCK_READS` is set.
#![cfg(any(target_os = "linux", target_os = "macos"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::cell::Cell;
use std::collections::BTreeMap;
use std::ffi::{c_char, c_void};
use std::sync::{Mutex, OnceLock, PoisonError};

/// Frames kept of each call site's stack.
const FRAMES: usize = 32;

/// Whether the calling thread is in a simulation run, as the program answers it.
pub type InRun = extern "C" fn() -> bool;

/// Called with each call site counted: how many reads it made, and its call stack, a frame a
/// line, innermost first, each `function` or `function (file:line)` (UTF-8, `len` bytes, valid
/// during the call).
pub type Visit = extern "C" fn(count: u64, stack: *const c_char, len: usize);

/// The program's test for "in a run", once it installed one.
static IN_RUN: OnceLock<InRun> = OnceLock::new();

/// Reads counted, by call stack (return addresses, innermost first).
static SITES: Mutex<BTreeMap<Vec<usize>, u64>> = Mutex::new(BTreeMap::new());

thread_local! {
    /// The probe is working on this thread: what it calls meanwhile is not counted.
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

/// Count reads by threads `in_run` says are in a simulation run, from now on. Only the first
/// test installed is used.
#[unsafe(no_mangle)]
pub extern "C" fn switchy_clock_probe_install(in_run: InRun) {
    let _ = IN_RUN.set(in_run);
}

/// Forget every read counted so far.
#[unsafe(no_mangle)]
pub extern "C" fn switchy_clock_probe_reset() {
    SITES.lock().unwrap_or_else(PoisonError::into_inner).clear();
}

/// Call `visit` with each call site counted, most reads first.
#[unsafe(no_mangle)]
pub extern "C" fn switchy_clock_probe_report(visit: Visit) {
    let mut sites: Vec<(Vec<usize>, u64)> = SITES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .map(|(frames, count)| (frames.clone(), *count))
        .collect();
    sites.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    busy(|| {
        for (frames, count) in sites {
            let stack = describe(&frames);
            visit(count, stack.as_ptr().cast(), stack.len());
        }
    });
}

/// Run `f` with this thread marked busy; nothing if it already is (the probe called itself).
fn busy(f: impl FnOnce()) {
    let _ = BUSY.try_with(|busy| {
        if !busy.replace(true) {
            f();
            busy.set(false);
        }
    });
}

/// A clock was read: count it if the calling thread is in a run.
fn observe() {
    let Some(in_run) = IN_RUN.get() else {
        return;
    };
    busy(|| {
        if in_run() {
            let mut frames = Vec::with_capacity(FRAMES);
            backtrace::trace(|frame| {
                frames.push(frame.ip() as usize);
                frames.len() < FRAMES
            });
            *SITES
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .entry(frames)
                .or_insert(0) += 1;
        }
    });
}

/// The functions `frames` name, with their source when known, a line each (an address when
/// nothing names it).
fn describe(frames: &[usize]) -> String {
    frames
        .iter()
        .map(|&ip| {
            let mut frame = None;
            backtrace::resolve(ip as *mut c_void, |symbol| {
                if frame.is_none() {
                    let name = symbol
                        .name()
                        .map_or_else(|| format!("{ip:#x}"), |name| name.to_string());
                    frame = Some(match (symbol.filename(), symbol.lineno()) {
                        (Some(file), Some(line)) => format!("{name} ({}:{line})", file.display()),
                        _ => name,
                    });
                }
            });
            frame.unwrap_or_else(|| format!("{ip:#x}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Linux: the functions themselves, found before the C library's (`LD_PRELOAD`); the real ones
/// are the next definitions (`RTLD_NEXT`).
#[cfg(target_os = "linux")]
mod interpose {
    use std::ffi::{CStr, c_int, c_void};
    use std::sync::OnceLock;

    type ClockGettime = unsafe extern "C" fn(libc::clockid_t, *mut libc::timespec) -> c_int;
    type Gettimeofday = unsafe extern "C" fn(*mut libc::timeval, *mut c_void) -> c_int;
    type Time = unsafe extern "C" fn(*mut libc::time_t) -> libc::time_t;

    /// The next definition of `name`, of type `F` (a function pointer).
    ///
    /// # Safety
    ///
    /// `F` must be the function's type.
    unsafe fn next<F: Copy>(name: &CStr) -> F {
        // SAFETY: a terminated name; RTLD_NEXT looks past this library.
        let function = unsafe { libc::dlsym(libc::RTLD_NEXT, name.as_ptr()) };
        assert!(!function.is_null(), "no {name:?} after the clock probe");
        // SAFETY: a function pointer of type `F`, as the caller promises.
        unsafe { std::mem::transmute_copy(&function) }
    }

    /// # Safety
    ///
    /// As the C library's.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn clock_gettime(
        clock: libc::clockid_t,
        time: *mut libc::timespec,
    ) -> c_int {
        static REAL: OnceLock<ClockGettime> = OnceLock::new();
        super::observe();
        // SAFETY: the C library's function, of this type, called as the caller called this.
        unsafe { REAL.get_or_init(|| next(c"clock_gettime"))(clock, time) }
    }

    /// # Safety
    ///
    /// As the C library's.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn gettimeofday(time: *mut libc::timeval, zone: *mut c_void) -> c_int {
        static REAL: OnceLock<Gettimeofday> = OnceLock::new();
        super::observe();
        // SAFETY: as in `clock_gettime`.
        unsafe { REAL.get_or_init(|| next(c"gettimeofday"))(time, zone) }
    }

    /// # Safety
    ///
    /// As the C library's.
    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn time(time: *mut libc::time_t) -> libc::time_t {
        static REAL: OnceLock<Time> = OnceLock::new();
        super::observe();
        // SAFETY: as in `clock_gettime`.
        unsafe { REAL.get_or_init(|| next(c"time"))(time) }
    }
}

/// macOS: replacements dyld puts in place of the system's (`__interpose`), which call the real
/// functions (calls from this library are not interposed).
#[cfg(target_os = "macos")]
mod interpose {
    use std::ffi::{c_int, c_void};

    /// One `__interpose` entry: calls to `original` go to `replacement`.
    #[repr(C)]
    struct Interpose {
        replacement: *const (),
        original: *const (),
    }

    // SAFETY: plain addresses, read by dyld before anything runs.
    unsafe impl Sync for Interpose {}

    unsafe extern "C" fn clock_gettime(clock: libc::clockid_t, time: *mut libc::timespec) -> c_int {
        super::observe();
        // SAFETY: as the caller called this.
        unsafe { libc::clock_gettime(clock, time) }
    }

    unsafe extern "C" fn gettimeofday(time: *mut libc::timeval, zone: *mut c_void) -> c_int {
        super::observe();
        // SAFETY: as the caller called this.
        unsafe { libc::gettimeofday(time, zone) }
    }

    unsafe extern "C" fn time(time: *mut libc::time_t) -> libc::time_t {
        super::observe();
        // SAFETY: as the caller called this.
        unsafe { libc::time(time) }
    }

    #[used]
    #[unsafe(link_section = "__DATA,__interpose")]
    static INTERPOSED: [Interpose; 3] = [
        Interpose {
            replacement: clock_gettime as *const (),
            original: libc::clock_gettime as *const (),
        },
        Interpose {
            replacement: gettimeofday as *const (),
            original: libc::gettimeofday as *const (),
        },
        Interpose {
            replacement: time as *const (),
            original: libc::time as *const (),
        },
    ];
}
