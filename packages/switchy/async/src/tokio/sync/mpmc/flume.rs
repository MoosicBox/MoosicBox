//! Multi-producer, multi-consumer channel implementation for tokio runtime.
//!
//! This wraps flume to provide additional methods needed by the codebase.

use std::sync::{Arc, Mutex, Weak};
use std::task::{Context, Poll};

use futures::task::AtomicWaker;

/// Flume may retain disconnected send hooks; those hooks must not own task state.
struct SendWake(Weak<AtomicWaker>);

impl std::task::Wake for SendWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if let Some(waker) = self.0.upgrade() {
            waker.wake();
        }
    }
}

#[derive(Default)]
struct Receivers(Mutex<Vec<Weak<AtomicWaker>>>);

impl Receivers {
    fn register(&self) -> Arc<AtomicWaker> {
        let waker = Arc::new(AtomicWaker::new());
        let mut receivers = self.0.lock().unwrap();
        receivers.retain(|receiver| receiver.strong_count() != 0);
        receivers.push(Arc::downgrade(&waker));
        waker
    }

    fn wake(&self) {
        let wakers: Vec<_> = {
            let mut receivers = self.0.lock().unwrap();
            receivers.retain(|receiver| receiver.strong_count() != 0);
            receivers.iter().filter_map(Weak::upgrade).collect()
        };
        // Wakers may synchronously re-enter channel operations.
        for waker in wakers {
            waker.wake();
        }
    }
}

/// Receiving end of an MPMC channel.
///
/// Multiple receivers compete for messages. Each receiver's `poll_recv` retains
/// only its most recently registered waker; clone the receiver for separate tasks.
pub struct Receiver<T> {
    inner: flume::Receiver<T>,
    receivers: Arc<Receivers>,
    waker: Arc<AtomicWaker>,
}

impl<T> std::fmt::Debug for Receiver<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Receiver").finish_non_exhaustive()
    }
}

/// Sending end of an MPMC channel.
///
/// The channel closes when its last sender is dropped; queued messages remain
/// available to receivers until drained.
pub struct Sender<T> {
    inner: Option<flume::Sender<T>>,
    receivers: Arc<Receivers>,
}

impl<T> std::fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sender").finish_non_exhaustive()
    }
}

pub use flume::{RecvError, RecvTimeoutError, SendError, TryRecvError, TrySendError};

impl<T> Receiver<T> {
    /// Receive a value, blocking until one is available.
    ///
    /// # Errors
    /// * Returns `RecvError::Disconnected` if all senders have been dropped.
    pub fn recv(&self) -> Result<T, RecvError> {
        self.inner.recv()
    }

    /// Try to receive a value without blocking.
    ///
    /// # Errors
    /// * Returns `TryRecvError::Empty` if no data is available.
    /// * Returns `TryRecvError::Disconnected` if all senders have been dropped.
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.inner.try_recv()
    }

    /// Receive a value with a timeout.
    ///
    /// # Errors
    /// * Returns `RecvTimeoutError::Timeout` if the timeout expires.
    /// * Returns `RecvTimeoutError::Disconnected` if all senders have been dropped.
    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Result<T, RecvTimeoutError> {
        self.inner.recv_timeout(timeout)
    }

    /// Poll for a value, registering the current task when no value is available.
    pub fn poll_recv(&self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        // Register before inspecting the queue to avoid a send/close race.
        self.waker.register(cx.waker());
        let result = match self.inner.try_recv() {
            Ok(value) => Poll::Ready(Some(value)),
            Err(TryRecvError::Empty) => Poll::Pending,
            Err(TryRecvError::Disconnected) => Poll::Ready(None),
        };
        if result.is_ready() {
            // Completed receives no longer need notification. Drop outside any
            // registry lock: task-owned state can re-enter channel operations.
            drop(self.waker.take());
        }
        result
    }

    /// Receive a value asynchronously.
    ///
    /// # Errors
    /// * Returns `RecvError::Disconnected` if all senders have been dropped.
    pub async fn recv_async(&self) -> Result<T, RecvError> {
        self.inner.recv_async().await
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            receivers: self.receivers.clone(),
            waker: self.receivers.register(),
        }
    }
}

impl<T> Sender<T> {
    /// Send a value, blocking if the channel is full.
    ///
    /// # Errors
    /// * Returns `SendError` if all receivers have been dropped.
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        // Polling first lets a rendezvous sender notify a pending poll_recv
        // after Flume has actually made the value available to that receiver.
        use std::future::Future as _;
        struct ThreadWake(std::thread::Thread);
        impl std::task::Wake for ThreadWake {
            fn wake(self: Arc<Self>) {
                self.0.unpark();
            }
        }
        let waker = std::task::Waker::from(Arc::new(ThreadWake(std::thread::current())));
        let mut context = Context::from_waker(&waker);
        let mut send = std::pin::pin!(self.send_async(value));
        loop {
            if let Poll::Ready(result) = send.as_mut().poll(&mut context) {
                return result;
            }
            std::thread::park();
        }
    }

    /// Send a value asynchronously.
    ///
    /// # Errors
    /// * Returns `SendError` if all receivers have been dropped.
    pub async fn send_async(&self, value: T) -> Result<(), SendError<T>> {
        use std::future::Future as _;
        let Some(inner) = self.inner.as_ref() else {
            return Err(SendError(value));
        };
        let task_waker = Arc::new(AtomicWaker::new());
        let forwarding_waker =
            std::task::Waker::from(Arc::new(SendWake(Arc::downgrade(&task_waker))));
        let mut send = std::pin::pin!(inner.send_async(value));
        std::future::poll_fn(|cx| {
            // Register before polling so a concurrent capacity notification is
            // never lost. The future owns task state; Flume owns only a Weak.
            task_waker.register(cx.waker());
            let mut context = Context::from_waker(&forwarding_waker);
            let result = send.as_mut().poll(&mut context);
            self.receivers.wake();
            result
        })
        .await
    }

    /// Try to send a value without blocking.
    ///
    /// # Errors
    /// * Returns `TrySendError::Full` if the channel is at capacity.
    /// * Returns `TrySendError::Disconnected` if all receivers have been dropped.
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        let Some(inner) = self.inner.as_ref() else {
            return Err(TrySendError::Disconnected(value));
        };
        let result = inner.try_send(value);
        if result.is_ok() {
            self.receivers.wake();
        }
        result
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            receivers: self.receivers.clone(),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        // Publish Flume's closed state before waking receivers.
        drop(self.inner.take());
        self.receivers.wake();
    }
}

fn wrap<T>((tx, rx): (flume::Sender<T>, flume::Receiver<T>)) -> (Sender<T>, Receiver<T>) {
    let receivers = Arc::new(Receivers::default());
    let waker = receivers.register();
    (
        Sender {
            inner: Some(tx),
            receivers: receivers.clone(),
        },
        Receiver {
            inner: rx,
            receivers,
            waker,
        },
    )
}

/// Create an unbounded channel.
#[must_use]
pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
    wrap(flume::unbounded())
}

/// Create a bounded channel (zero capacity supports rendezvous).
#[must_use]
pub fn bounded<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    wrap(flume::bounded(capacity))
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use std::task::{Wake, Waker};

    use futures::task::AtomicWaker;

    struct CountWake(AtomicUsize);

    impl Wake for CountWake {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct PausedWake {
        entered: std::sync::mpsc::Sender<()>,
        release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl Wake for PausedWake {
        fn wake(self: Arc<Self>) {
            self.entered.send(()).unwrap();
            self.release
                .lock()
                .unwrap()
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
        }
    }

    struct PanicWake;

    impl Wake for PanicWake {
        fn wake(self: Arc<Self>) {
            panic!("task wake failure");
        }
    }

    #[test]
    fn send_forwarder_releases_temporary_ownership_when_callback_panics() {
        let task = Arc::new(PanicWake);
        let task_probe = Arc::downgrade(&task);
        let state = Arc::new(AtomicWaker::new());
        let state_probe = Arc::downgrade(&state);
        state.register(&Waker::from(task));
        let forwarder = Waker::from(Arc::new(super::SendWake(Arc::downgrade(&state))));
        let worker = std::thread::spawn(move || forwarder.wake());
        assert!(worker.join().is_err());
        // The owner is still alive, but the panicking callback must not be.
        assert!(task_probe.upgrade().is_none());
        assert_eq!(Arc::strong_count(&state), 1);
        drop(state);
        assert!(state_probe.upgrade().is_none());
    }

    #[test]
    fn send_forwarder_in_flight_callback_outlives_owner_only_until_return() {
        let (entered, observe) = std::sync::mpsc::channel();
        let (release, resume) = std::sync::mpsc::channel();
        let task = Arc::new(PausedWake {
            entered,
            release: std::sync::Mutex::new(resume),
        });
        let task_probe = Arc::downgrade(&task);
        let state = Arc::new(AtomicWaker::new());
        let state_probe = Arc::downgrade(&state);
        state.register(&Waker::from(task));
        let forwarder = Waker::from(Arc::new(super::SendWake(Arc::downgrade(&state))));
        let worker = std::thread::spawn(move || {
            forwarder.wake_by_ref();
            forwarder
        });
        observe
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        drop(state);
        let state_in_flight = state_probe.upgrade().is_some();
        let task_in_flight = task_probe.upgrade().is_some();
        release.send(()).unwrap();
        let orphaned_waker = worker.join().unwrap();
        assert!(state_in_flight);
        assert!(task_in_flight);
        assert!(state_probe.upgrade().is_none());
        assert!(task_probe.upgrade().is_none());
        orphaned_waker.wake();
    }

    #[test]
    fn send_forwarder_racing_owner_drop_releases_task_state() {
        for _ in 0..32 {
            let task = Arc::new(CountWake(AtomicUsize::new(0)));
            let task_probe = Arc::downgrade(&task);
            let state = Arc::new(AtomicWaker::new());
            let state_probe = Arc::downgrade(&state);
            state.register(&Waker::from(task));
            let forwarder = Waker::from(Arc::new(super::SendWake(Arc::downgrade(&state))));
            let start = Arc::new(std::sync::Barrier::new(2));
            let worker_start = start.clone();
            let worker = std::thread::spawn(move || {
                worker_start.wait();
                forwarder.wake_by_ref();
                forwarder
            });
            start.wait();
            drop(state);
            // An in-flight Weak upgrade may temporarily own state. Reclamation
            // is required after the callback completes, not during that race.
            let orphaned_waker = worker.join().unwrap();
            assert!(state_probe.upgrade().is_none());
            assert!(task_probe.upgrade().is_none());
            orphaned_waker.wake_by_ref();
            orphaned_waker.wake();
            assert!(task_probe.upgrade().is_none());
        }
    }

    #[test]
    fn send_forwarder_notifies_live_state_and_ignores_stale_wakes() {
        let task = Arc::new(CountWake(AtomicUsize::new(0)));
        let task_probe = Arc::downgrade(&task);
        let task_waker = Waker::from(task.clone());
        let state = Arc::new(AtomicWaker::new());
        let state_probe = Arc::downgrade(&state);
        let forwarder = Waker::from(Arc::new(super::SendWake(Arc::downgrade(&state))));
        state.register(&task_waker);
        forwarder.wake_by_ref();
        assert_eq!(task.0.load(Ordering::SeqCst), 1);
        state.register(&task_waker);
        Waker::from(Arc::new(super::SendWake(Arc::downgrade(&state)))).wake();
        assert_eq!(task.0.load(Ordering::SeqCst), 2);
        // Leave a task registered when its owner disappears.
        state.register(&task_waker);
        drop(task_waker);
        drop(task);
        drop(state);
        assert!(state_probe.upgrade().is_none());
        assert!(task_probe.upgrade().is_none());
        forwarder.wake_by_ref();
        forwarder.wake();
        assert!(task_probe.upgrade().is_none());
    }
}
