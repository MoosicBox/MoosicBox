//! Several workers polling a runtime's tasks at once (`thread-simulator`).
//!
//! A multi-thread runtime runs two tasks at the same time, so the synchronous code between
//! their awaits overlaps at every lock and atomic. With one executor thread it never does: a
//! task runs undisturbed until it awaits. With workers, each step of the runtime (each
//! `tick`, each turn of `block_on`) polls up to `n` ready tasks at once: the stepping thread
//! polls one, and `n - 1` simulated worker threads poll the others; the step ends when every
//! one of those polls returns.
//!
//! Helpers are simulated threads: only one thread runs at a time, they switch only at switch
//! points (locks, atomics, channels, I/O), and the run's seeded scheduler decides every switch,
//! so every interleaving is a pure function of the seed. Each runtime keeps its own (see
//! [`Pool`]): they belong to its thread group (`Runtime::shutdown_now` kills them) and are
//! daemons (the run's end kills them).
//!
//! Time still moves per step, as with one executor: a task that is ready again at once (a
//! `Sleep` waiting for the clock) is polled once per step, not in a loop.
//!
//! The number of workers comes from the runtime's `Builder` (1 for a current-thread runtime,
//! `worker_threads` for a multi-thread one), else from `SIMULATOR_ASYNC_WORKERS` for the whole
//! run: a number, or `swarm` to draw it per run (one run in four gets 1, the others 2 to 4).
//! Unset, it is 1: one executor thread, as before.

use std::{any::Any, cell::Cell, collections::VecDeque, sync::Arc};

use switchy_thread::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use super::runtime::{Runtime, Task};

thread_local! {
    /// Whether this thread is polling a task of a step with several workers (a poll there
    /// never starts another such step).
    static IN_STEP: Cell<bool> = const { Cell::new(false) };
}

/// Whether the calling thread is polling a task of a step with several workers.
pub(super) fn in_step() -> bool {
    IN_STEP.get()
}

/// The run's number of workers when no runtime fixes its own: from `SIMULATOR_ASYNC_WORKERS`
/// (a number, or `swarm` to draw it per run), else 1.
///
/// # Panics
///
/// * If `SIMULATOR_ASYNC_WORKERS` is neither a number nor `swarm`
pub(super) fn run_workers() -> usize {
    struct RunWorkers(usize);
    switchy_simulation::state_with(|| {
        let count = match std::env::var("SIMULATOR_ASYNC_WORKERS").as_deref() {
            Ok("swarm") => {
                let stream = switchy_simulation::stream("switchy_async::workers");
                let mut rng = stream.lock().unwrap_or_else(PoisonError::into_inner);
                if rng.below(4) == 0 {
                    1
                } else {
                    2 + usize::try_from(rng.below(3)).unwrap_or(0)
                }
            }
            Ok(value) => value.parse().unwrap_or_else(|_| {
                panic!("SIMULATOR_ASYNC_WORKERS must be a number or swarm, got '{value}'")
            }),
            Err(_) => 1,
        };
        RunWorkers(count.max(1))
    })
    .0
}

/// A runtime's workers: simulated daemon threads, started as its steps first need them, that
/// poll the tasks a step hands them. They wait between steps, and live until the runtime is
/// shut down (its thread group is killed) or the run ends (daemons are killed then), so a run
/// starts at most `n - 1` of them per runtime, however many steps it takes.
#[derive(Debug, Default)]
pub(super) struct Pool {
    state: Mutex<PoolState>,
    /// Signalled when jobs are queued.
    work: Condvar,
    /// Signalled when a job is done.
    done: Condvar,
}

#[derive(Default)]
struct PoolState {
    workers: usize,
    jobs: VecDeque<Arc<Task>>,
    outstanding: usize,
    panic: Option<Box<dyn Any + Send>>,
}

impl std::fmt::Debug for PoolState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PoolState")
            .field("workers", &self.workers)
            .field("jobs", &self.jobs.len())
            .field("outstanding", &self.outstanding)
            .finish_non_exhaustive()
    }
}

/// Counts a worker's job done when dropped, even when the worker is killed while polling it.
struct JobDone<'a>(&'a Pool);

impl Drop for JobDone<'_> {
    fn drop(&mut self) {
        let mut state = self.0.lock();
        state.outstanding -= 1;
        drop(state);
        self.0.done.notify_all();
    }
}

/// Wakes the stepping thread when a worker is killed (its runtime shut down), so it stops
/// waiting for jobs no worker will take.
struct Leaving<'a>(&'a Pool);

impl Drop for Leaving<'_> {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.0.done.notify_all();
        }
    }
}

impl Pool {
    fn lock(&self) -> MutexGuard<'_, PoolState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Poll `batch` at once: the first task on this thread, each other on a worker, and return
    /// when every poll has. A poll that panicked panics here (this thread's first).
    ///
    /// # Panics
    ///
    /// * If a worker cannot start, or a poll panicked
    pub(super) fn run_batch(self: &Arc<Self>, runtime: &Runtime, mut batch: Vec<Arc<Task>>) {
        let first = batch.remove(0);
        let mut state = self.lock();
        while state.workers < batch.len() {
            state.workers += 1;
            let (pool, runtime) = (Arc::clone(self), runtime.clone());
            switchy_thread::thread::Builder::new()
                .name(format!("async-worker-{}", state.workers))
                .group(runtime.thread_group())
                .daemon(true)
                .fault_free(true)
                .spawn(move || pool.serve(&runtime))
                .expect("a simulated thread starts");
        }
        state.outstanding += batch.len();
        state.jobs.extend(batch);
        drop(state);
        self.work.notify_all();
        let outer = IN_STEP.replace(true);
        let own = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runtime.process_on_this_thread(first);
        }));
        IN_STEP.set(outer);
        let mut state = self.lock();
        while state.outstanding > 0 && !runtime.is_shut_down() {
            state = self
                .done
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
        // Shut down: its workers are killed, and the jobs they never took are dropped.
        let untaken = std::mem::take(&mut state.jobs);
        let theirs = state.panic.take();
        drop(state);
        drop(untaken);
        if let Some(payload) = own.err().or(theirs) {
            std::panic::resume_unwind(payload);
        }
    }

    /// A worker's life: poll the jobs it takes, until it is killed.
    fn serve(&self, runtime: &Runtime) {
        IN_STEP.set(true);
        let _leaving = Leaving(self);
        loop {
            let mut state = self.lock();
            let task = loop {
                if let Some(task) = state.jobs.pop_front() {
                    break task;
                }
                state = self
                    .work
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            };
            drop(state);
            let done = JobDone(self);
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.process_on_this_thread(task);
            }));
            if let Err(payload) = outcome {
                if payload.is::<switchy_thread::simulator::Killed>() {
                    std::panic::resume_unwind(payload);
                }
                self.lock().panic.get_or_insert(payload);
            }
            drop(done);
        }
    }
}
