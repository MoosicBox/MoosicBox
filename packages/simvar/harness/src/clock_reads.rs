//! `SIMULATOR_CLOCK_READS`: count the real clock reads simulation runs make.
//!
//! A run's clock is the simulator's. With `switchy_clock_probe` loaded into the process (Linux:
//! `LD_PRELOAD`, macOS: `DYLD_INSERT_LIBRARIES`), every call to the C library's clock functions
//! goes through it, and those made by a thread in a run are counted by call site. Set to
//! `report`, the call sites are listed when the simulation ends; set to `fail`, any not allowed
//! fails it.
//!
//! A call site is judged by its caller: the first frame outside the standard library. Reads by
//! the simulators themselves (simvar's harness, switchy's simulators: a run's real duration,
//! waiting for the OS to spare a thread) are allowed; so are callers matching any comma-separated
//! pattern of `SIMULATOR_CLOCK_READS_ALLOW` (a function or source path whose reads never reach
//! a run, such as a logger's timestamps). Build with symbols and line tables to see names
//! (`CARGO_PROFILE_RELEASE_STRIP=none CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`).

use std::ffi::{CStr, c_char};
use std::sync::{Mutex, PoisonError};

/// How reads are handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Report,
    Fail,
}

type InRun = extern "C" fn() -> bool;
type Visit = extern "C" fn(u64, *const c_char, usize);
type Install = unsafe extern "C" fn(InRun);
type Report = unsafe extern "C" fn(Visit);

/// The loaded probe, driving it.
pub struct ClockReads {
    mode: Mode,
    report: Report,
}

/// Whether `caller` is the simulators' own source (simvar's harness, a switchy crate's
/// simulator), whose real reads (a run's duration, waiting for the OS to spare a thread) never
/// reach a run.
fn simulator(caller: &str) -> bool {
    caller.contains("/packages/simvar/harness/")
        || (caller.contains("/packages/switchy/") && caller.contains("/simulator"))
}

/// The call sites the probe reported, while it reports.
static SITES: Mutex<Vec<(u64, String)>> = Mutex::new(Vec::new());

extern "C" fn in_run() -> bool {
    switchy::simulation::in_run()
}

extern "C" fn visit(count: u64, stack: *const c_char, len: usize) {
    // SAFETY: the probe passes `len` bytes of UTF-8, valid during the call.
    let stack = unsafe { std::slice::from_raw_parts(stack.cast::<u8>(), len) };
    SITES
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((count, String::from_utf8_lossy(stack).into_owned()));
}

/// The probe's function `name`, of type `F`, if the probe is loaded.
///
/// # Safety
///
/// `F` must be the function's type.
#[cfg(unix)]
unsafe fn probe_function<F: Copy>(name: &CStr) -> Option<F> {
    // SAFETY: a terminated name; RTLD_DEFAULT searches every loaded image.
    let function = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
    // SAFETY: a function pointer of type `F`, as the caller promises.
    (!function.is_null()).then(|| unsafe { std::mem::transmute_copy(&function) })
}

#[cfg(not(unix))]
const unsafe fn probe_function<F: Copy>(_name: &CStr) -> Option<F> {
    None
}

impl ClockReads {
    /// Start counting, as `SIMULATOR_CLOCK_READS` asks; `None` when it is not set.
    ///
    /// # Errors
    ///
    /// * If it is set to anything but `report` or `fail`, or the probe is not loaded
    pub fn start() -> Result<Option<Self>, String> {
        let mode = match std::env::var("SIMULATOR_CLOCK_READS").as_deref() {
            Err(_) | Ok("" | "0" | "false") => return Ok(None),
            Ok("report" | "1" | "true") => Mode::Report,
            Ok("fail") => Mode::Fail,
            Ok(other) => {
                return Err(format!(
                    "SIMULATOR_CLOCK_READS must be report or fail, got '{other}'"
                ));
            }
        };
        // SAFETY: the probe's functions, of these types.
        let (install, report) = unsafe {
            (
                probe_function::<Install>(c"switchy_clock_probe_install"),
                probe_function::<Report>(c"switchy_clock_probe_report"),
            )
        };
        let (Some(install), Some(report)) = (install, report) else {
            return Err(
                "SIMULATOR_CLOCK_READS is set, but the clock probe is not loaded: \
                 load switchy_clock_probe with LD_PRELOAD (Linux) or DYLD_INSERT_LIBRARIES \
                 (macOS)"
                    .to_owned(),
            );
        };
        // SAFETY: the probe's function, given a function that never unwinds.
        unsafe { install(in_run) };
        Ok(Some(Self { mode, report }))
    }

    /// The reads counted, listed on standard error.
    ///
    /// # Errors
    ///
    /// * With `fail`, if a call site not allowed read the clock
    pub fn finish(self) -> Result<(), String> {
        SITES.lock().unwrap_or_else(PoisonError::into_inner).clear();
        // SAFETY: the probe's function, given a function that never unwinds.
        unsafe { (self.report)(visit) };
        let sites = std::mem::take(&mut *SITES.lock().unwrap_or_else(PoisonError::into_inner));
        let allowed: Vec<String> = std::env::var("SIMULATOR_CLOCK_READS_ALLOW")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty())
            .map(str::to_owned)
            .collect();
        let mut refused = 0;
        let total: u64 = sites.iter().map(|(count, _)| count).sum();
        eprintln!(
            "real clock reads in simulation runs: {total} from {} call sites",
            sites.len()
        );
        for (count, stack) in &sites {
            let frames = interesting_frames(stack);
            let caller = frames.first().copied().unwrap_or_default();
            let allow = if simulator(caller) {
                Some("a simulator's own")
            } else {
                allowed
                    .iter()
                    .find(|pattern| caller.contains(pattern.as_str()))
                    .map(String::as_str)
            };
            if let Some(pattern) = allow {
                eprintln!("  {count} (allowed: {pattern})");
            } else {
                refused += 1;
                eprintln!("  {count}");
            }
            for frame in frames {
                eprintln!("      {frame}");
            }
        }
        if self.mode == Mode::Fail && refused > 0 {
            return Err(format!(
                "{refused} call sites read the real clock in simulation runs \
                 (SIMULATOR_CLOCK_READS=fail): read switchy_time's clock there, or allow them \
                 with SIMULATOR_CLOCK_READS_ALLOW if what they read never reaches a run"
            ));
        }
        Ok(())
    }
}

/// The frames of `stack` that say who read the clock, its caller first: from the first frame
/// that is not the probe's, the stack walker's, the standard library's or the C library's
/// clock function, at most a dozen.
fn interesting_frames(stack: &str) -> Vec<&str> {
    stack
        .lines()
        .skip_while(|frame| infrastructure(frame))
        .take(12)
        .collect()
}

/// Whether `frame` (`function` or `function (file:line)`) is the probe's, the stack walker's,
/// the standard library's or a C library clock function, rather than a caller's.
fn infrastructure(frame: &str) -> bool {
    let (name, source) = frame
        .rsplit_once(" (")
        .map_or((frame, ""), |(name, source)| (name, source));
    [
        "/clock_probe/",
        "/backtrace-",
        "/rustc/",
        "/library/std/",
        "/library/core/",
    ]
    .iter()
    .any(|path| source.contains(path))
        || name.starts_with("std::")
        || name.starts_with("<std")
        || ["clock_gettime", "gettimeofday", "time"].contains(&name)
        || (source.is_empty() && name.starts_with("0x"))
}
