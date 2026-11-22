//! A single-producer, multi-consumer channel that only retains the last sent value.
//!
//! This mirrors `tokio::sync::watch`, but wakes waiting receivers in a deterministic order.
//! tokio spreads the receivers waiting for a change over eight `Notify` shards picked by its
//! thread-local random number generator whenever its `rt` or `macros` feature is enabled, so
//! with several receivers waiting, the order their tasks are woken (and so queued on the
//! simulator executor) varies between runs of the same seed. Here every waiter is kept in one
//! list ordered by when it started waiting, and a send wakes them in that order.
//!
//! Waiters for [`Sender::closed`] are kept the same way.
//!
//! With `thread-simulator`, a simulated thread that finds the value locked (another thread holds
//! a [`Ref`] or is sending) waits on `switchy_thread`'s scheduler instead of blocking its OS
//! thread, so the other simulated threads keep running and a deadlock is reported.

use std::{
    collections::BTreeMap,
    fmt,
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard},
    task::{Context, Poll, Waker},
};

/// Watch error types.
pub mod error {
    use std::{error::Error, fmt};

    /// Error produced when sending a value fails because every receiver is gone.
    #[derive(PartialEq, Eq, Clone, Copy)]
    pub struct SendError<T>(pub T);

    impl<T> fmt::Debug for SendError<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("SendError").finish_non_exhaustive()
        }
    }

    impl<T> fmt::Display for SendError<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "channel closed")
        }
    }

    impl<T> Error for SendError<T> {}

    /// Error produced when waiting for a change fails because every sender is gone.
    #[derive(Debug, Clone)]
    pub struct RecvError(pub(super) ());

    impl fmt::Display for RecvError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "channel closed")
        }
    }

    impl Error for RecvError {}
}

/// Wakers in the order their waits started.
#[derive(Default)]
struct Waiters {
    next: u64,
    wakers: BTreeMap<u64, Waker>,
}

impl Waiters {
    /// Register (or refresh) the waker of the wait `slot` holds, keeping its place in line if it
    /// is still waiting.
    fn register(&mut self, slot: &mut Option<u64>, waker: &Waker) {
        if let Some(entry) = slot.and_then(|id| self.wakers.get_mut(&id)) {
            entry.clone_from(waker);
            return;
        }
        let id = self.next;
        self.next += 1;
        self.wakers.insert(id, waker.clone());
        *slot = Some(id);
    }

    fn remove(&mut self, slot: Option<u64>) {
        if let Some(id) = slot {
            self.wakers.remove(&id);
        }
    }

    /// Every waker, oldest wait first, for waking once the state lock is released.
    fn take(&mut self) -> Vec<Waker> {
        std::mem::take(&mut self.wakers).into_values().collect()
    }
}

fn wake(wakers: Vec<Waker>) {
    for waker in wakers {
        waker.wake();
    }
}

/// Everything about the channel except its value. Never held across a switch point or a wake.
struct State {
    /// Incremented by every send that modifies the value, while the value's write lock is held.
    version: u64,
    /// Whether every sender is gone.
    closed: bool,
    senders: usize,
    receivers: usize,
    /// Receivers waiting for a change.
    changed: Waiters,
    /// Senders waiting for every receiver to be gone.
    closed_waiters: Waiters,
}

struct Shared<T> {
    value: RwLock<T>,
    state: Mutex<State>,
    /// The `switchy_thread` wait key of the value's lock.
    #[cfg(feature = "thread-simulator")]
    lock_key: u64,
}

impl<T: fmt::Debug> fmt::Debug for Shared<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = self.state();
        f.debug_struct("Shared")
            .field("value", &self.value)
            .field("version", &state.version)
            .field("is_closed", &state.closed)
            .field("ref_count_rx", &state.receivers)
            .finish_non_exhaustive()
    }
}

/// Shared read access to the value, releasing it to waiting simulated threads when dropped.
struct ReadGuard<'a, T> {
    guard: Option<RwLockReadGuard<'a, T>>,
    shared: &'a Shared<T>,
}

impl<T> Deref for ReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.guard.as_ref().expect("guard is held until drop")
    }
}

impl<T> Drop for ReadGuard<'_, T> {
    fn drop(&mut self) {
        drop(self.guard.take());
        self.shared.unlocked();
    }
}

/// Exclusive access to the value, releasing it to waiting simulated threads when dropped.
struct WriteGuard<'a, T> {
    guard: Option<RwLockWriteGuard<'a, T>>,
    shared: &'a Shared<T>,
}

impl<T> Deref for WriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        self.guard.as_ref().expect("guard is held until drop")
    }
}

impl<T> DerefMut for WriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.guard.as_mut().expect("guard is held until drop")
    }
}

impl<T> Drop for WriteGuard<'_, T> {
    fn drop(&mut self) {
        drop(self.guard.take());
        self.shared.unlocked();
    }
}

impl<T> Shared<T> {
    fn new(value: T) -> Self {
        Self {
            value: RwLock::new(value),
            state: Mutex::new(State {
                version: 0,
                closed: false,
                senders: 1,
                receivers: 1,
                changed: Waiters::default(),
                closed_waiters: Waiters::default(),
            }),
            #[cfg(feature = "thread-simulator")]
            lock_key: switchy_thread::simulator::new_key(),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Acquire the value's lock with `try_lock`, falling back to `lock` outside a simulated
    /// thread. A simulated thread that finds it taken waits on the scheduler until it is
    /// released, so the thread holding it can run.
    #[cfg_attr(not(feature = "thread-simulator"), allow(clippy::unused_self))]
    fn acquire<G>(
        &self,
        try_lock: impl Fn() -> std::sync::TryLockResult<G>,
        lock: impl FnOnce() -> std::sync::LockResult<G>,
    ) -> G {
        #[cfg(feature = "thread-simulator")]
        if let Some(scheduler) = switchy_thread::simulator::try_scheduler() {
            loop {
                match try_lock() {
                    Ok(guard) => return guard,
                    Err(std::sync::TryLockError::Poisoned(poisoned)) => {
                        return poisoned.into_inner();
                    }
                    Err(std::sync::TryLockError::WouldBlock) => {
                        scheduler.block(
                            switchy_thread::simulator::Op::Lock,
                            switchy_thread::simulator::WaitKey::Lock(self.lock_key),
                            None,
                        );
                    }
                }
            }
        }
        #[cfg(not(feature = "thread-simulator"))]
        let _ = try_lock;
        lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn read(&self) -> ReadGuard<'_, T> {
        let guard = self.acquire(|| self.value.try_read(), || self.value.read());
        ReadGuard {
            guard: Some(guard),
            shared: self,
        }
    }

    fn write(&self) -> WriteGuard<'_, T> {
        let guard = self.acquire(|| self.value.try_write(), || self.value.write());
        WriteGuard {
            guard: Some(guard),
            shared: self,
        }
    }

    /// Let simulated threads waiting for the value's lock retry.
    #[cfg_attr(
        not(feature = "thread-simulator"),
        allow(clippy::unused_self, clippy::missing_const_for_fn)
    )]
    fn unlocked(&self) {
        #[cfg(feature = "thread-simulator")]
        if let Some(scheduler) = switchy_thread::simulator::try_scheduler() {
            scheduler.wake_all(switchy_thread::simulator::WaitKey::Lock(self.lock_key));
        }
    }
}

/// Creates a new watch channel, returning the sender and receiver halves.
///
/// Every receiver starts having seen `init`.
#[must_use]
pub fn channel<T>(init: T) -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared::new(init));
    (
        Sender {
            shared: shared.clone(),
        },
        Receiver { shared, version: 0 },
    )
}

/// A reference to the value held in the channel.
///
/// It holds a read lock on the value: holding it across an `.await` or for long blocks senders.
pub struct Ref<'a, T> {
    inner: ReadGuard<'a, T>,
    has_changed: bool,
}

impl<T> Ref<'_, T> {
    /// Whether the borrowed value was not yet seen by the receiver that borrowed it.
    ///
    /// Always `false` for a reference borrowed from the [`Sender`].
    #[must_use]
    pub const fn has_changed(&self) -> bool {
        self.has_changed
    }
}

impl<T> Deref for Ref<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.inner
    }
}

impl<T: fmt::Debug> fmt::Debug for Ref<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ref")
            .field("inner", &**self)
            .field("has_changed", &self.has_changed)
            .finish()
    }
}

/// Receives values from the associated [`Sender`].
#[derive(Debug)]
pub struct Receiver<T> {
    shared: Arc<Shared<T>>,
    /// The version last seen.
    version: u64,
}

/// Sends values to the associated [`Receiver`]s.
#[derive(Debug)]
pub struct Sender<T> {
    shared: Arc<Shared<T>>,
}

/// Waits for the version to differ from `version`, then marks it seen. Its place in the wait
/// list is taken on its first pending poll and given up when it completes or is dropped.
struct Changed<'a, T> {
    shared: &'a Shared<T>,
    version: &'a mut u64,
    waiter: Option<u64>,
}

impl<T> Future for Changed<'_, T> {
    type Output = Result<(), error::RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if super::super::faults::spurious_pending_now(cx, || {
            let state = this.shared.state();
            state.version != *this.version || state.closed
        }) {
            return Poll::Pending;
        }
        let mut state = this.shared.state();
        if state.version != *this.version {
            *this.version = state.version;
            state.changed.remove(this.waiter.take());
            return Poll::Ready(Ok(()));
        }
        if state.closed {
            state.changed.remove(this.waiter.take());
            return Poll::Ready(Err(error::RecvError(())));
        }
        state.changed.register(&mut this.waiter, cx.waker());
        Poll::Pending
    }
}

impl<T> Drop for Changed<'_, T> {
    fn drop(&mut self) {
        if self.waiter.is_some() {
            self.shared.state().changed.remove(self.waiter.take());
        }
    }
}

/// Waits for every receiver to be gone, keeping its place in line like [`Changed`].
struct Closed<'a, T> {
    shared: &'a Shared<T>,
    waiter: Option<u64>,
}

impl<T> Future for Closed<'_, T> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        let this = self.get_mut();
        if super::super::faults::spurious_pending_now(cx, || this.shared.state().receivers == 0) {
            return Poll::Pending;
        }
        let mut state = this.shared.state();
        if state.receivers == 0 {
            state.closed_waiters.remove(this.waiter.take());
            return Poll::Ready(());
        }
        state.closed_waiters.register(&mut this.waiter, cx.waker());
        Poll::Pending
    }
}

impl<T> Drop for Closed<'_, T> {
    fn drop(&mut self) {
        if self.waiter.is_some() {
            self.shared
                .state()
                .closed_waiters
                .remove(self.waiter.take());
        }
    }
}

impl<T> Receiver<T> {
    /// Borrows the most recent value without marking it seen.
    ///
    /// The returned [`Ref`] holds a read lock on the value; see its documentation.
    #[must_use]
    pub fn borrow(&self) -> Ref<'_, T> {
        let inner = self.shared.read();
        // Versions change only under the value's write lock, so this one matches the value.
        let has_changed = self.version != self.shared.state().version;
        Ref { inner, has_changed }
    }

    /// Borrows the most recent value and marks it seen.
    ///
    /// The returned [`Ref`] holds a read lock on the value; see its documentation.
    pub fn borrow_and_update(&mut self) -> Ref<'_, T> {
        let inner = self.shared.read();
        let version = self.shared.state().version;
        let has_changed = self.version != version;
        self.version = version;
        Ref { inner, has_changed }
    }

    /// Whether a value not yet seen by this receiver was sent.
    ///
    /// # Errors
    ///
    /// * If every sender is gone
    pub fn has_changed(&self) -> Result<bool, error::RecvError> {
        let state = self.shared.state();
        if state.closed {
            return Err(error::RecvError(()));
        }
        Ok(self.version != state.version)
    }

    /// Whether every sender is gone.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.shared.state().closed
    }

    /// Marks the current value as not seen, so the next [`Self::changed`] completes at once.
    pub const fn mark_changed(&mut self) {
        self.version = self.version.wrapping_sub(1);
    }

    /// Marks the current value as seen.
    pub fn mark_unchanged(&mut self) {
        self.version = self.shared.state().version;
    }

    /// Waits for a value not yet seen by this receiver, then marks the newest value seen.
    ///
    /// Waiting receivers are woken in the order they started waiting. Cancelling the wait leaves
    /// the value unseen.
    ///
    /// # Errors
    ///
    /// * If every sender is gone and every value was seen
    // Generic: `Send` whenever `T: Send + Sync`, as with tokio's watch (see the contract tests).
    #[allow(clippy::future_not_send)]
    pub async fn changed(&mut self) -> Result<(), error::RecvError> {
        Changed {
            shared: &self.shared,
            version: &mut self.version,
            waiter: None,
        }
        .await
    }

    /// Waits for a value that satisfies `f`, starting with the current one, and marks it seen.
    ///
    /// # Errors
    ///
    /// * If every sender is gone before a value satisfied `f`
    // Generic: `Send` whenever `T: Send + Sync`, as with tokio's watch (see the contract tests).
    #[allow(clippy::future_not_send)]
    pub async fn wait_for(
        &mut self,
        mut f: impl FnMut(&T) -> bool,
    ) -> Result<Ref<'_, T>, error::RecvError> {
        let mut closed = false;
        loop {
            {
                let inner = self.shared.read();
                let version = self.shared.state().version;
                let has_changed = self.version != version;
                self.version = version;
                if (!closed || has_changed) && f(&inner) {
                    return Ok(Ref { inner, has_changed });
                }
            }
            if closed {
                return Err(error::RecvError(()));
            }
            closed = Changed {
                shared: &self.shared,
                version: &mut self.version,
                waiter: None,
            }
            .await
            .is_err();
        }
    }

    /// Whether both receivers belong to the same channel.
    #[must_use]
    pub fn same_channel(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        self.shared.state().receivers += 1;
        Self {
            shared: self.shared.clone(),
            version: self.version,
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut state = self.shared.state();
        state.receivers -= 1;
        let wakers = if state.receivers == 0 {
            state.closed_waiters.take()
        } else {
            Vec::new()
        };
        drop(state);
        wake(wakers);
    }
}

impl<T> Sender<T> {
    /// A sender of a channel without receivers; see [`Self::subscribe`].
    #[must_use]
    pub fn new(init: T) -> Self {
        channel(init).0
    }

    /// Sends a new value and wakes every waiting receiver, oldest wait first.
    ///
    /// # Errors
    ///
    /// * If there are no receivers; the value is returned and not stored
    pub fn send(&self, value: T) -> Result<(), error::SendError<T>> {
        if self.receiver_count() == 0 {
            return Err(error::SendError(value));
        }
        self.send_replace(value);
        Ok(())
    }

    /// Modifies the value in place and wakes every waiting receiver, even without receivers.
    pub fn send_modify<F>(&self, modify: F)
    where
        F: FnOnce(&mut T),
    {
        self.send_if_modified(|value| {
            modify(value);
            true
        });
    }

    /// Modifies the value in place, and if `modify` returns `true` marks it changed and wakes
    /// every waiting receiver, oldest wait first. Returns what `modify` returned.
    pub fn send_if_modified<F>(&self, modify: F) -> bool
    where
        F: FnOnce(&mut T) -> bool,
    {
        let mut value = self.shared.write();
        if !modify(&mut value) {
            return false;
        }
        let mut state = self.shared.state();
        state.version = state.version.wrapping_add(1);
        let wakers = state.changed.take();
        drop(state);
        drop(value);
        wake(wakers);
        true
    }

    /// Replaces the value, wakes every waiting receiver, and returns the previous value, even
    /// without receivers.
    pub fn send_replace(&self, mut value: T) -> T {
        self.send_modify(|old| std::mem::swap(old, &mut value));
        value
    }

    /// Borrows the current value; [`Ref::has_changed`] is always `false`.
    #[must_use]
    pub fn borrow(&self) -> Ref<'_, T> {
        Ref {
            inner: self.shared.read(),
            has_changed: false,
        }
    }

    /// Whether every receiver is gone.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.receiver_count() == 0
    }

    /// Waits for every receiver to be gone. Waiters are woken in the order they started waiting.
    // Generic: `Send` whenever `T: Send + Sync`, as with tokio's watch (see the contract tests).
    #[allow(clippy::future_not_send)]
    pub async fn closed(&self) {
        Closed {
            shared: &self.shared,
            waiter: None,
        }
        .await;
    }

    /// A new receiver that has seen the current value.
    #[must_use]
    pub fn subscribe(&self) -> Receiver<T> {
        let mut state = self.shared.state();
        state.receivers += 1;
        let version = state.version;
        drop(state);
        Receiver {
            shared: self.shared.clone(),
            version,
        }
    }

    /// The number of receivers.
    #[must_use]
    pub fn receiver_count(&self) -> usize {
        self.shared.state().receivers
    }

    /// The number of senders.
    #[must_use]
    pub fn sender_count(&self) -> usize {
        self.shared.state().senders
    }

    /// Whether both senders belong to the same channel.
    #[must_use]
    pub fn same_channel(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.shared.state().senders += 1;
        Self {
            shared: self.shared.clone(),
        }
    }
}

impl<T: Default> Default for Sender<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut state = self.shared.state();
        state.senders -= 1;
        let wakers = if state.senders == 0 {
            state.closed = true;
            state.changed.take()
        } else {
            Vec::new()
        };
        drop(state);
        wake(wakers);
    }
}
