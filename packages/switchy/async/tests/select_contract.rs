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
