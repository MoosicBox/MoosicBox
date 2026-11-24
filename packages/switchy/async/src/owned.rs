//! Blocking driver for one externally-woken root future, independent of either runtime.

use std::{
    future::Future,
    pin::pin,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Poll, Wake, Waker},
};

mod scheduler;
pub use scheduler::{Executor, JoinError, JoinHandle};

#[derive(Default)]
struct Signal {
    notified: Mutex<bool>,
    changed: Condvar,
}

impl Wake for Signal {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        *self.notified.lock().unwrap() = true;
        self.changed.notify_one();
    }
}

/// Poll a single future on the calling thread until it completes. The future can
/// borrow local, non-`Send` data. Repeated wakes may coalesce into one poll.
///
/// This does not enter a runtime, run spawned tasks, advance a clock or poll OS
/// readiness. Only use it where progress does not require work on the blocked
/// thread.
///
/// # Panics
///
/// Propagates a panic from the future. Panics if the driver's private notification
/// mutex is poisoned.
pub fn block_on<F: Future>(future: F) -> F::Output {
    // Per invocation: a waker retained past completion cannot signal another root.
    let signal = Arc::new(Signal::default());
    let waker = Waker::from(Arc::clone(&signal));
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
            return value;
        }
        // A wake during poll or just before waiting must still pay for the next poll.
        let notified = signal.notified.lock().unwrap();
        *signal.changed.wait_while(notified, |set| !*set).unwrap() = false;
    }
}
