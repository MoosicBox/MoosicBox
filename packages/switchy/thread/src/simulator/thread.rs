//! Simulated threads: real OS threads that run only when the run's scheduler picks them.

use std::{
    io,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use super::{
    IDENTITY, Killed, Op, Scheduler, ThreadIndex, WaitKey, coverage, deadline_after, scheduler,
};

pub use std::thread::{current, panicking};

/// A handle to join a simulated thread.
#[derive(Debug)]
pub struct JoinHandle<T> {
    scheduler: Arc<Scheduler>,
    index: ThreadIndex,
    result: Arc<Mutex<Option<std::thread::Result<T>>>>,
    thread: std::thread::Thread,
}

impl<T> JoinHandle<T> {
    /// Wait for the thread to finish, returning its result or its panic.
    ///
    /// # Errors
    ///
    /// * The thread's panic payload, if it panicked
    ///
    /// # Panics
    ///
    /// * If the run deadlocks while waiting
    #[track_caller]
    pub fn join(self) -> std::thread::Result<T> {
        let _site = coverage::enter();
        loop {
            self.scheduler.switch_point(Op::Join);
            if self.scheduler.finished(self.index) {
                break;
            }
            self.scheduler
                .wait(Op::Join, WaitKey::Join(self.index), None);
        }
        self.result
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .expect("a finished simulated thread stores its result")
    }

    /// Whether the thread has finished. A switch point.
    #[must_use]
    #[track_caller]
    pub fn is_finished(&self) -> bool {
        let _site = coverage::enter();
        self.scheduler.switch_point(Op::Join);
        self.scheduler.finished(self.index)
    }

    /// The underlying OS thread.
    #[must_use]
    pub const fn thread(&self) -> &std::thread::Thread {
        &self.thread
    }
}

/// Configures a simulated thread before spawning it.
#[derive(Debug, Default)]
pub struct Builder {
    name: Option<String>,
    stack_size: Option<usize>,
    group: Option<u64>,
    daemon: bool,
}

impl Builder {
    /// A builder with no name and the default stack size.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            name: None,
            stack_size: None,
            group: None,
            daemon: false,
        }
    }

    /// Make the thread a daemon: one that serves the run until it ends (a runtime's worker, a
    /// service loop). When the run ends, it is killed if it has not finished, and it is not
    /// reported as leaked.
    #[must_use]
    pub const fn daemon(mut self, daemon: bool) -> Self {
        self.daemon = daemon;
        self
    }

    /// Put the thread in `group` (see [`Scheduler::kill_group`]), rather than the group set by
    /// [`super::with_group`] or else its parent's. Group 0 is none: no group kill reaches it.
    #[must_use]
    pub const fn group(mut self, group: u64) -> Self {
        self.group = Some(group);
        self
    }

    /// Name the thread. Names appear in deadlock reports and traces.
    #[must_use]
    pub fn name(mut self, name: String) -> Self {
        self.name = Some(name);
        self
    }

    /// The OS thread's stack size.
    #[must_use]
    pub const fn stack_size(mut self, size: usize) -> Self {
        self.stack_size = Some(size);
        self
    }

    /// Spawn the thread. It runs when the scheduler picks it; spawning is a switch point.
    ///
    /// # Errors
    ///
    /// * [`io::ErrorKind::WouldBlock`] when the run's spawn failure fault strikes (see
    ///   [`super::set_spawn_failures`])
    ///
    /// # Panics
    ///
    /// * If the operating system cannot start the OS thread, even after waiting for other runs
    ///   to free some (a real failure would make the run depend on the machine)
    #[track_caller]
    pub fn spawn<F, T>(self, f: F) -> io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let _site = coverage::enter();
        let scheduler = scheduler();
        let context = switchy_simulation::current();
        let context_id = context.id();
        let name = self.name.unwrap_or_else(|| {
            let count = scheduler.snapshot().threads.len();
            format!("sim-thread-{count}")
        });
        let group = self.group.or_else(super::current_group);
        // Spawning numbers the new thread, so it depends on every other spawn (and on
        // anything reading the scheduler's threads).
        scheduler.touch_any();
        if super::spawn_fails() {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "Resource temporarily unavailable",
            ));
        }
        let index = scheduler.add_thread(scheduler.me(), name.clone(), group);
        if self.daemon {
            scheduler.mark_daemon(index);
        }
        let result = Arc::new(Mutex::new(None));
        let child_scheduler = Arc::clone(&scheduler);
        let child_result = Arc::clone(&result);
        let body = move || {
            switchy_simulation::set_current(Some(context));
            IDENTITY.set(Some((context_id, index)));
            child_scheduler.wait_turn(index);
            let outcome = if child_scheduler.is_killed(index) {
                // Killed before it started: its closure never runs.
                drop(f);
                Err(Box::new(Killed) as Box<dyn std::any::Any + Send>)
            } else {
                std::panic::catch_unwind(AssertUnwindSafe(f))
            };
            let panicked = outcome
                .as_ref()
                .is_err_and(|payload| !payload.is::<Killed>());
            *child_result.lock().unwrap_or_else(PoisonError::into_inner) = Some(outcome);
            child_scheduler.exit(index, panicked);
            // Stay alive (parked) until the run ends, so this thread's thread-local
            // destructors cannot run concurrently with the run's other threads.
            child_scheduler.wait_turn(index);
        };
        let handle = match start_os_thread(&name, self.stack_size, body) {
            Ok(handle) => handle,
            Err(error) => {
                // No OS thread: the scheduler must never hand this one a turn.
                scheduler.forget(index);
                panic!(
                    "the operating system cannot start simulated thread '{name}' ({error}): \
                     the run needs more threads than the system allows (simulated threads keep \
                     their OS thread until their run ends; run fewer runs at once)"
                );
            }
        };
        let thread = handle.thread().clone();
        scheduler.attach(index, handle);
        scheduler.switch_point(Op::Spawn);
        Ok(JoinHandle {
            scheduler,
            index,
            result,
            thread,
        })
    }
}

/// How long a simulated spawn waits for the operating system to have a thread to spare.
const OS_THREAD_PATIENCE: Duration = Duration::from_secs(120);

/// Start the OS thread of a simulated thread. When the operating system refuses (too many
/// threads: the runs at once, each keeping its finished threads until it ends), wait in real
/// time for other runs to free some and try again, up to [`OS_THREAD_PATIENCE`]: whether the OS
/// has a thread to spare is not the run's to decide, so it must never change what the run does.
fn start_os_thread(
    name: &str,
    stack_size: Option<usize>,
    body: impl FnOnce() + Send + 'static,
) -> io::Result<std::thread::JoinHandle<()>> {
    let body = Arc::new(Mutex::new(Some(body)));
    let started = std::time::Instant::now();
    let mut pause = Duration::from_millis(1);
    loop {
        let mut builder = std::thread::Builder::new().name(name.to_string());
        if let Some(size) = stack_size {
            builder = builder.stack_size(size);
        }
        let slot = Arc::clone(&body);
        let attempt = builder.spawn(move || {
            let body = slot.lock().unwrap_or_else(PoisonError::into_inner).take();
            if let Some(body) = body {
                body();
            }
        });
        match attempt {
            Ok(handle) => return Ok(handle),
            Err(error) if started.elapsed() >= OS_THREAD_PATIENCE => return Err(error),
            Err(_) => {
                std::thread::sleep(pause);
                pause = (pause * 2).min(Duration::from_millis(500));
            }
        }
    }
}

/// Spawn a simulated thread; see [`Builder::spawn`].
///
/// # Panics
///
/// * If the OS thread cannot be created, or the run's spawn failure fault strikes (see
///   [`super::set_spawn_failures`])
#[track_caller]
pub fn spawn<F, T>(f: F) -> JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    Builder::new()
        .spawn(f)
        .expect("failed to spawn simulated thread")
}

/// Let other simulated threads run. A switch point.
#[track_caller]
pub fn yield_now() {
    let _site = coverage::enter();
    scheduler().switch_point(Op::Yield);
}

/// Block the calling simulated thread for `duration` of simulated time.
#[track_caller]
pub fn sleep(duration: Duration) {
    let _site = coverage::enter();
    let scheduler = scheduler();
    // A killed thread unwinding (a dead process) waits for nothing: its destructors' sleeps
    // return at once, so they release what they hold instead of parking the thread for good.
    if scheduler.is_killed(scheduler.me()) {
        return;
    }
    if duration.is_zero() {
        scheduler.switch_point(Op::Sleep);
        return;
    }
    let deadline = deadline_after(duration);
    while !scheduler.wait(Op::Sleep, WaitKey::Sleep, deadline) {}
}
