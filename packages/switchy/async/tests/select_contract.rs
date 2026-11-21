//! Behavioral select contracts shared by real and simulated backends.

#![cfg(all(feature = "macros", feature = "time"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};

struct PendingWork(Arc<AtomicBool>);

impl Future for PendingWork {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<()> {
        Poll::Pending
    }
}

impl Drop for PendingWork {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[switchy_async::test(real_time)]
async fn biased_ready_shutdown_precedes_ready_work() {
    let selected = switchy_async::select! {
        biased;
        () = std::future::ready(()) => "shutdown",
        () = std::future::ready(()) => "work",
    };
    assert_eq!(selected, "shutdown");
}

#[switchy_async::test(real_time)]
async fn selecting_shutdown_drops_owned_pending_work() {
    let dropped = Arc::new(AtomicBool::new(false));
    let work = PendingWork(dropped.clone());
    switchy_async::select! {
        biased;
        () = std::future::ready(()) => {},
        () = work => panic!("pending work must not complete"),
    }
    assert!(dropped.load(Ordering::SeqCst));
}

/// A branch whose precondition is false is not polled, and the others still run.
#[cfg(feature = "simulator")]
#[test]
fn preconditions_disable_branches() {
    let runtime = switchy_async::simulator::runtime::Runtime::new();
    let picked = runtime.block_on(async {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<u32>();
        tx.send(7).unwrap();
        let mut picked = Vec::new();
        let mut ready = std::future::ready(1);
        for enabled in [false, true] {
            switchy_async::select! {
                value = &mut ready, if enabled => picked.push(("ready", value)),
                value = rx.recv(), if !enabled => picked.push(("recv", value.unwrap())),
            }
        }
        picked
    });
    assert_eq!(picked, [("recv", 7), ("ready", 1)]);
}

/// Which of two always-ready branches a non-biased select picks follows the run's seed: the same
/// for a seed, different across seeds, and fair to both.
#[cfg(feature = "thread-simulator")]
#[test]
fn non_biased_selection_follows_the_seed() {
    fn choices(seed: u64) -> Vec<u8> {
        // A run of its own, seeded with `seed`.
        let outcome = switchy_thread::simulator::run(seed, || {
            switchy_random::simulator::reset_rng();
            let runtime = switchy_async::simulator::runtime::Runtime::new();
            runtime.block_on(async {
                let mut picked = Vec::new();
                for _ in 0..32 {
                    switchy_async::select! {
                        () = std::future::ready(()) => picked.push(0),
                        () = std::future::ready(()) => picked.push(1),
                    }
                }
                picked
            })
        });
        outcome.result.unwrap()
    }
    let first = choices(7);
    assert_eq!(first, choices(7));
    assert!(first.contains(&0) && first.contains(&1), "{first:?}");
    assert_ne!(first, choices(8));
}
