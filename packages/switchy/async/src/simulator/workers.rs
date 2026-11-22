//! Several workers polling a runtime's tasks at once (`thread-simulator`).
//!
//! A multi-thread runtime runs two tasks at the same time, so the synchronous code between
//! their awaits overlaps at every lock and atomic. With one executor thread it never does: a
//! task runs undisturbed until it awaits. With workers, each step of the runtime (each
//! `tick`, each turn of `block_on`) polls up to `n` ready tasks at once: the stepping thread
//! polls one, and `n - 1` simulated helper threads poll the others; the step ends when every
//! one of those polls returns.
//!
//! Helpers are simulated threads: only one thread runs at a time, they switch only at switch
//! points (locks, atomics, channels, I/O), and the run's seeded scheduler decides every switch,
//! so every interleaving is a pure function of the seed. They belong to the runtime's thread
//! group (`Runtime::shutdown_now` kills them) and live for one step, so nothing outlives it.
//!
//! Time still moves per step, as with one executor: a task that is ready again at once (a
//! `Sleep` waiting for the clock) is polled once per step, not in a loop.
//!
//! The number of workers comes from the runtime's `Builder` (1 for a current-thread runtime,
//! `worker_threads` for a multi-thread one), else from `SIMULATOR_ASYNC_WORKERS` for the whole
//! run: a number, or `swarm` to draw it per run (one run in four gets 1, the others 2 to 4).
//! Unset, it is 1: one executor thread, as before.

use std::{cell::Cell, sync::Arc, sync::PoisonError};

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

/// Poll `batch` at once: the first task on this thread, each other on a helper thread, and
/// return when every poll has. A poll that panicked panics here (the first, in batch order).
///
/// # Panics
///
/// * If a helper thread cannot start, or a poll panicked
pub(super) fn run_batch(runtime: &Runtime, mut batch: Vec<Arc<Task>>) {
    let first = batch.remove(0);
    let helpers: Vec<_> = batch
        .into_iter()
        .enumerate()
        .map(|(index, task)| {
            let runtime = runtime.clone();
            switchy_thread::thread::Builder::new()
                .name(format!("async-worker-{}", index + 1))
                .group(runtime.thread_group())
                .spawn(move || {
                    IN_STEP.set(true);
                    runtime.process_on_this_thread(task);
                })
                .expect("a simulated thread starts")
        })
        .collect();
    let outer = IN_STEP.replace(true);
    let own = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.process_on_this_thread(first);
    }));
    IN_STEP.set(outer);
    let mut panic = own.err();
    for helper in helpers {
        if let Err(payload) = helper.join() {
            panic.get_or_insert(payload);
        }
    }
    if let Some(payload) = panic {
        std::panic::resume_unwind(payload);
    }
}
