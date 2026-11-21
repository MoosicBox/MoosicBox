//! Simulated threads: real OS threads that run only when the run's scheduler picks them.

use std::{
    io,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

use super::{IDENTITY, Op, Scheduler, ThreadIndex, WaitKey, deadline_after, scheduler};

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
    pub fn join(self) -> std::thread::Result<T> {
        loop {
            self.scheduler.yield_point(Op::Join);
            if self.scheduler.finished(self.index) {
                break;
            }
            self.scheduler
                .block(Op::Join, WaitKey::Join(self.index), None);
        }
        self.result
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .expect("a finished simulated thread stores its result")
    }

    /// Whether the thread has finished. A switch point.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.scheduler.yield_point(Op::Join);
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
}

impl Builder {
    /// A builder with no name and the default stack size.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            name: None,
            stack_size: None,
        }
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
    /// * If the OS thread cannot be created
    pub fn spawn<F, T>(self, f: F) -> io::Result<JoinHandle<T>>
    where
        F: FnOnce() -> T + Send + 'static,
        T: Send + 'static,
    {
        let scheduler = scheduler();
        let context = switchy_simulation::current();
        let context_id = context.id();
        let name = self.name.unwrap_or_else(|| {
            let count = scheduler.snapshot().threads.len();
            format!("sim-thread-{count}")
        });
        let index = scheduler.add_thread(scheduler.me(), name.clone());
        let result = Arc::new(Mutex::new(None));
        let mut builder = std::thread::Builder::new().name(name);
        if let Some(size) = self.stack_size {
            builder = builder.stack_size(size);
        }
        let child_scheduler = Arc::clone(&scheduler);
        let child_result = Arc::clone(&result);
        let handle = builder.spawn(move || {
            switchy_simulation::set_current(Some(context));
            IDENTITY.set(Some((context_id, index)));
            child_scheduler.wait_turn(index);
            let outcome = std::panic::catch_unwind(AssertUnwindSafe(f));
            let panicked = outcome.is_err();
            *child_result.lock().unwrap_or_else(PoisonError::into_inner) = Some(outcome);
            child_scheduler.exit(index, panicked);
            // Stay alive (parked) until the run ends, so this thread's thread-local
            // destructors cannot run concurrently with the run's other threads.
            child_scheduler.wait_turn(index);
        })?;
        let thread = handle.thread().clone();
        scheduler.attach(index, handle);
        scheduler.yield_point(Op::Spawn);
        Ok(JoinHandle {
            scheduler,
            index,
            result,
            thread,
        })
    }
}

/// Spawn a simulated thread; see [`Builder::spawn`].
///
/// # Panics
///
/// * If the OS thread cannot be created
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
pub fn yield_now() {
    scheduler().yield_point(Op::Yield);
}

/// Block the calling simulated thread for `duration` of simulated time.
pub fn sleep(duration: Duration) {
    let scheduler = scheduler();
    if duration.is_zero() {
        scheduler.yield_point(Op::Sleep);
        return;
    }
    let deadline = deadline_after(duration);
    while !scheduler.block(Op::Sleep, WaitKey::Sleep, deadline) {}
}
