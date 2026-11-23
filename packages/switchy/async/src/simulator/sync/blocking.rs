//! Waiting for one of the simulator's futures from synchronous code (the `blocking_*` methods).
//!
//! With `thread-simulator`, a simulated thread waits on `switchy_thread`'s scheduler, so the
//! other simulated threads (and the executor) keep running and a deadlock is reported instead of
//! hanging. Any other thread parks until the future's waker is woken.

use std::{
    future::Future,
    pin::pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

/// What a blocked simulated thread waits for, as `switchy_thread` reports it.
#[derive(Debug, Clone, Copy)]
pub enum Wait {
    /// A lock or permit.
    Lock,
    /// A value or notification.
    Recv,
}

/// Wakes a thread blocked in [`block_on`].
struct Signal {
    woken: AtomicBool,
    /// The OS thread to unpark, when the waiting thread is not a simulated one.
    thread: Option<std::thread::Thread>,
    /// The `switchy_thread` wait key of a simulated thread.
    #[cfg(feature = "thread-simulator")]
    key: Option<switchy_thread::simulator::WaitKey>,
}

impl Wake for Signal {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.woken.store(true, Ordering::SeqCst);
        #[cfg(feature = "thread-simulator")]
        if let Some(key) = self.key
            && let Some(scheduler) = switchy_thread::simulator::try_scheduler()
        {
            scheduler.wake_all(key);
        }
        if let Some(thread) = &self.thread {
            thread.unpark();
        }
    }
}

/// Poll `future` to completion on the calling thread, waiting between polls until its waker is
/// woken.
#[cfg_attr(not(feature = "thread-simulator"), allow(unused_variables))]
pub fn block_on<F: Future>(wait: Wait, future: F) -> F::Output {
    let mut future = pin!(future);

    #[cfg(feature = "thread-simulator")]
    if let Some(scheduler) = switchy_thread::simulator::try_scheduler() {
        use switchy_thread::simulator::{Op, WaitKey};

        let id = switchy_thread::simulator::new_key();
        let (op, key) = match wait {
            Wait::Lock => (Op::Lock, WaitKey::Lock(id)),
            Wait::Recv => (Op::Recv, WaitKey::Recv(id)),
        };
        let signal = Arc::new(Signal {
            woken: AtomicBool::new(false),
            thread: None,
            key: Some(key),
        });
        let waker = Waker::from(Arc::clone(&signal));
        let mut cx = Context::from_waker(&waker);
        loop {
            if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
                return output;
            }
            // Only one simulated thread runs at a time and this is the switch point, so a wake
            // cannot slip in between the check and the wait.
            while !signal.woken.swap(false, Ordering::SeqCst) {
                let _ = scheduler.block(op, key, None);
            }
        }
    }

    let signal = Arc::new(Signal {
        woken: AtomicBool::new(false),
        thread: Some(std::thread::current()),
        #[cfg(feature = "thread-simulator")]
        key: None,
    });
    let waker = Waker::from(Arc::clone(&signal));
    let mut cx = Context::from_waker(&waker);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return output;
        }
        while !signal.woken.swap(false, Ordering::SeqCst) {
            // simulation-safe: outside a simulated thread, a blocking call blocks its OS thread,
            // as tokio's do; its waker unparks it.
            std::thread::park();
        }
    }
}
