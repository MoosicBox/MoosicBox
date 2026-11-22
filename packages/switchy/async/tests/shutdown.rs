//! `Runtime::shutdown_now`: a runtime stopped at once, as a killed process's.
#![cfg(feature = "simulator")]

use std::{
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use switchy_async::{simulator::runtime::Runtime, task::JoinSet};

/// Counts its drops.
struct DropFlag(Arc<AtomicUsize>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

/// A task that counts its polls, holds a [`DropFlag`] and never finishes.
async fn forever(polls: Arc<AtomicUsize>, flag: DropFlag) {
    let _flag = flag;
    std::future::poll_fn(|cx| {
        polls.fetch_add(1, Ordering::SeqCst);
        // Queued again at once: always in the runtime's queue.
        cx.waker().wake_by_ref();
        std::task::Poll::<()>::Pending
    })
    .await;
}

#[test]
fn shutdown_drops_queued_and_parked_tasks_without_polling_them_again() {
    let runtime = Runtime::new();
    let polls = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicUsize::new(0));

    let queued = runtime.spawn(forever(Arc::clone(&polls), DropFlag(Arc::clone(&dropped))));
    // Parked: waits on a channel whose sender stays alive.
    let (tx, rx) = switchy_async::sync::oneshot::channel::<()>();
    let parked_flag = DropFlag(Arc::clone(&dropped));
    let parked = runtime.spawn(async move {
        let _flag = parked_flag;
        let _ = rx.await;
    });
    // A non-Send local task.
    let local_flag = Rc::new(DropFlag(Arc::clone(&dropped)));
    let local = runtime.spawn_local(async move {
        let _flag = local_flag;
        std::future::pending::<()>().await;
    });
    for _ in 0..10 {
        runtime.tick();
    }
    assert!(polls.load(Ordering::SeqCst) > 0);
    assert_eq!(dropped.load(Ordering::SeqCst), 0);

    runtime.shutdown_now();
    assert!(runtime.is_shut_down());
    assert_eq!(dropped.load(Ordering::SeqCst), 3, "every task was dropped");
    let frozen = polls.load(Ordering::SeqCst);
    for _ in 0..10 {
        runtime.tick();
    }
    drop(tx);
    assert_eq!(
        polls.load(Ordering::SeqCst),
        frozen,
        "a task ran after shutdown"
    );
    assert!(queued.is_finished() && parked.is_finished() && local.is_finished());

    // Join handles report the tasks cancelled.
    let other = Runtime::new();
    assert!(other.block_on(queued).is_err());
    assert!(other.block_on(parked).is_err());

    // A task spawned onto the shut-down runtime is dropped at once, unpolled.
    let late_polls = Arc::new(AtomicUsize::new(0));
    let late = runtime.spawn(forever(
        Arc::clone(&late_polls),
        DropFlag(Arc::clone(&dropped)),
    ));
    assert_eq!(dropped.load(Ordering::SeqCst), 4);
    runtime.tick();
    assert_eq!(late_polls.load(Ordering::SeqCst), 0);
    assert!(other.block_on(late).is_err());
    other.wait().unwrap();
}

#[test]
fn destructors_spawning_during_shutdown_are_dropped_too() {
    struct SpawnOnDrop(Arc<AtomicUsize>);
    impl Drop for SpawnOnDrop {
        fn drop(&mut self) {
            let dropped = Arc::clone(&self.0);
            let polls = Arc::new(AtomicUsize::new(0));
            // The shut-down runtime is current while destructors run.
            drop(switchy_async::task::spawn(forever(
                polls,
                DropFlag(dropped),
            )));
        }
    }
    let runtime = Runtime::new();
    let dropped = Arc::new(AtomicUsize::new(0));
    let flag = SpawnOnDrop(Arc::clone(&dropped));
    let _task = runtime.spawn(async move {
        let _flag = flag;
        std::future::pending::<()>().await;
    });
    runtime.tick();
    runtime.shutdown_now();
    assert_eq!(dropped.load(Ordering::SeqCst), 1);
}

#[test]
fn shutdown_drops_join_set_tasks() {
    let runtime = Runtime::new();
    let polls = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicUsize::new(0));
    let mut set = runtime.block_on({
        let polls = Arc::clone(&polls);
        let dropped = Arc::clone(&dropped);
        async move {
            let mut set = JoinSet::new();
            for _ in 0..3 {
                set.spawn(forever(Arc::clone(&polls), DropFlag(Arc::clone(&dropped))));
            }
            set
        }
    });
    for _ in 0..10 {
        runtime.tick();
    }
    runtime.shutdown_now();
    assert_eq!(dropped.load(Ordering::SeqCst), 3);
    let frozen = polls.load(Ordering::SeqCst);
    let other = Runtime::new();
    let mut cancelled = 0;
    while let Some(outcome) = other.block_on(set.join_next()) {
        assert!(outcome.unwrap_err().is_cancelled());
        cancelled += 1;
    }
    assert_eq!(cancelled, 3);
    assert_eq!(polls.load(Ordering::SeqCst), frozen);
}

#[cfg(feature = "thread-simulator")]
mod threads {
    use std::time::Duration;

    use switchy_thread::{simulator, thread};

    use super::*;

    /// Start a blocking closure that counts its iterations forever, then shut the runtime down.
    /// Returns the iterations before and after, the drops, and whether joining failed.
    fn kill_blocking() -> (usize, usize, usize, bool) {
        let runtime = Runtime::new();
        let progress = Arc::new(AtomicUsize::new(0));
        let dropped = Arc::new(AtomicUsize::new(0));
        let handle = {
            let progress = Arc::clone(&progress);
            let flag = DropFlag(Arc::clone(&dropped));
            runtime.spawn_blocking(move || {
                let _flag = flag;
                loop {
                    progress.fetch_add(1, Ordering::SeqCst);
                    thread::sleep(Duration::from_millis(1));
                }
            })
        };
        // A simulated thread a task starts directly belongs to the runtime too.
        let direct = {
            let progress = Arc::clone(&progress);
            let flag = DropFlag(Arc::clone(&dropped));
            runtime.spawn(async move {
                let _ = thread::spawn(move || {
                    let _flag = flag;
                    loop {
                        progress.fetch_add(1, Ordering::SeqCst);
                        thread::sleep(Duration::from_millis(1));
                    }
                });
            })
        };
        // Start both threads, then let them run a while.
        runtime.tick();
        runtime.tick();
        thread::sleep(Duration::from_millis(10));
        let before = progress.load(Ordering::SeqCst);

        runtime.shutdown_now();
        let frozen = progress.load(Ordering::SeqCst);
        thread::sleep(Duration::from_millis(10));
        assert_eq!(
            progress.load(Ordering::SeqCst),
            frozen,
            "a killed thread ran"
        );
        let other = Runtime::new();
        let failed = other.block_on(handle).is_err();
        let _ = other.block_on(direct);
        other.wait().unwrap();
        (before, frozen, dropped.load(Ordering::SeqCst), failed)
    }

    #[test]
    fn shutdown_kills_the_runtimes_simulated_threads_deterministically() {
        for seed in 0..10 {
            let outcome = simulator::run(seed, kill_blocking);
            assert!(outcome.report.leaked.is_empty(), "{:?}", outcome.report);
            assert!(outcome.report.panicked.is_empty(), "{:?}", outcome.report);
            let (before, frozen, dropped, failed) = outcome
                .result
                .unwrap_or_else(|panic| std::panic::resume_unwind(panic));
            assert!(before > 0, "seed {seed}: the threads never ran");
            assert!(frozen >= before);
            assert_eq!(dropped, 2, "seed {seed}");
            assert!(failed, "seed {seed}");
            let again = simulator::run(seed, kill_blocking);
            assert_eq!(
                outcome.report.trace_hash, again.report.trace_hash,
                "seed {seed}"
            );
        }
    }
}
