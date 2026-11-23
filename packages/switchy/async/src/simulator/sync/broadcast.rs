//! A multi-producer, multi-consumer broadcast queue with tokio's API: each receiver sees every
//! value sent while it exists.
//!
//! Mirrors `tokio::sync::broadcast`, down to its ring of slots: the capacity is rounded up to a
//! power of two, a receiver that falls more than the capacity behind is told how many values it
//! missed ([`error::RecvError::Lagged`]) and continues from the oldest retained one, and values
//! still retained are received after every sender is gone. Receivers waiting for a value are
//! kept in one list ordered by when they started waiting, and a send wakes them in that order.

use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    task::{Context, Poll},
};

use super::{
    Notify,
    blocking::{Wait, block_on},
};
use crate::simulator::{
    faults,
    waiters::{Waiters, wake},
};

/// Broadcast error types.
pub mod error {
    use std::fmt;

    /// Error returned by [`super::Sender::send`] when there are no receivers; it holds the
    /// value.
    #[derive(Debug)]
    pub struct SendError<T>(pub T);

    impl<T> fmt::Display for SendError<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "channel closed")
        }
    }

    impl<T: fmt::Debug> std::error::Error for SendError<T> {}

    /// Error returned by [`super::Receiver::recv`].
    #[derive(Debug, PartialEq, Eq, Clone)]
    pub enum RecvError {
        /// Every sender is gone and every retained value was received.
        Closed,
        /// The receiver fell behind and missed this many values; the next receive returns the
        /// oldest value retained.
        Lagged(u64),
    }

    impl fmt::Display for RecvError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Closed => write!(f, "channel closed"),
                Self::Lagged(amt) => write!(f, "channel lagged by {amt}"),
            }
        }
    }

    impl std::error::Error for RecvError {}

    /// Error returned by [`super::Receiver::try_recv`].
    #[derive(Debug, PartialEq, Eq, Clone)]
    pub enum TryRecvError {
        /// No value to receive now.
        Empty,
        /// Every sender is gone and every retained value was received.
        Closed,
        /// The receiver fell behind and missed this many values.
        Lagged(u64),
    }

    impl fmt::Display for TryRecvError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Empty => write!(f, "channel empty"),
                Self::Closed => write!(f, "channel closed"),
                Self::Lagged(amt) => write!(f, "channel lagged by {amt}"),
            }
        }
    }

    impl std::error::Error for TryRecvError {}
}

use error::{RecvError, SendError, TryRecvError};

/// The most receivers a channel can have.
const MAX_RECEIVERS: usize = usize::MAX >> 2;

struct Slot<T> {
    /// Receivers that have not seen it yet.
    rem: usize,
    /// The position of the value it holds (or held).
    pos: u64,
    val: Option<T>,
}

struct State<T> {
    buffer: Box<[Slot<T>]>,
    /// The position of the next value sent.
    pos: u64,
    receivers: usize,
    /// Every sender, or every receiver, is gone.
    closed: bool,
    senders: usize,
    weak_senders: usize,
    /// Receivers waiting for a value.
    waiters: Waiters,
}

struct Shared<T> {
    state: Mutex<State<T>>,
    mask: usize,
    /// Notified when the last receiver is dropped.
    last_receiver_dropped: Notify,
}

impl<T> Shared<T> {
    fn state(&self) -> MutexGuard<'_, State<T>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    const fn capacity(&self) -> u64 {
        (self.mask + 1) as u64
    }

    /// The slot of position `pos`.
    const fn index(&self, pos: u64) -> usize {
        #[allow(clippy::cast_possible_truncation)] // masked to an index of the buffer
        let index = (pos as usize) & self.mask;
        index
    }
}

impl<T> State<T> {
    /// Close the channel and take every waiting receiver's waker.
    fn close(&mut self) -> Vec<std::task::Waker> {
        self.closed = true;
        self.waiters.take()
    }
}

/// Creates a broadcast channel holding up to `capacity` values (rounded up to a power of two),
/// returning a sender and a receiver.
///
/// # Panics
///
/// * If `capacity` is 0 or above `usize::MAX / 2`
#[must_use]
pub fn channel<T: Clone>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let sender = Sender::with_receivers(1, capacity);
    let receiver = Receiver {
        shared: Arc::clone(&sender.shared),
        next: 0,
    };
    (sender, receiver)
}

/// Sends values to every [`Receiver`].
pub struct Sender<T> {
    shared: Arc<Shared<T>>,
}

/// A sender that does not keep the channel open; see [`Sender::downgrade`].
pub struct WeakSender<T> {
    shared: Arc<Shared<T>>,
}

/// Receives the values sent while it exists.
pub struct Receiver<T> {
    shared: Arc<Shared<T>>,
    /// The position of the next value to receive.
    next: u64,
}

impl<T> Sender<T> {
    /// A sender of a channel without receivers; see [`Self::subscribe`].
    ///
    /// # Panics
    ///
    /// * If `capacity` is 0 or above `usize::MAX / 2`
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self::with_receivers(0, capacity)
    }

    fn with_receivers(receivers: usize, capacity: usize) -> Self {
        assert!(capacity > 0, "broadcast channel capacity cannot be zero");
        assert!(
            capacity <= usize::MAX >> 1,
            "broadcast channel capacity exceeded `usize::MAX / 2`"
        );
        let capacity = capacity.next_power_of_two();
        let buffer = (0..capacity as u64)
            .map(|i| Slot {
                rem: 0,
                pos: i.wrapping_sub(capacity as u64),
                val: None,
            })
            .collect();
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    buffer,
                    pos: 0,
                    receivers,
                    closed: receivers == 0,
                    senders: 1,
                    weak_senders: 0,
                    waiters: Waiters::new(),
                }),
                mask: capacity - 1,
                last_receiver_dropped: Notify::new(),
            }),
        }
    }

    /// Sends `value` to every receiver, waking those waiting (oldest wait first). Returns how
    /// many receivers it was sent to.
    ///
    /// # Errors
    ///
    /// * The value, if there are no receivers
    pub fn send(&self, value: T) -> Result<usize, SendError<T>> {
        let mut state = self.shared.state();
        if state.receivers == 0 {
            return Err(SendError(value));
        }
        let pos = state.pos;
        let rem = state.receivers;
        state.pos = pos.wrapping_add(1);
        let slot = &mut state.buffer[self.shared.index(pos)];
        slot.pos = pos;
        slot.rem = rem;
        slot.val = Some(value);
        let wakers = state.waiters.take();
        drop(state);
        wake(wakers);
        Ok(rem)
    }

    /// A new receiver of the values sent from now on.
    ///
    /// # Panics
    ///
    /// * If the channel has too many receivers
    #[must_use]
    pub fn subscribe(&self) -> Receiver<T> {
        new_receiver(Arc::clone(&self.shared))
    }

    /// A [`WeakSender`] of this channel.
    #[must_use = "Downgrade creates a WeakSender without destroying the original non-weak sender."]
    pub fn downgrade(&self) -> WeakSender<T> {
        self.shared.state().weak_senders += 1;
        WeakSender {
            shared: Arc::clone(&self.shared),
        }
    }

    /// The number of values retained: sent and not yet seen by every receiver that existed
    /// then (or evicted).
    #[must_use]
    pub fn len(&self) -> usize {
        let state = self.shared.state();
        let base = self.shared.index(state.pos);
        let (mut low, mut high) = (0, state.buffer.len());
        while low < high {
            let mid = low + (high - low) / 2;
            let index = base.wrapping_add(mid) & self.shared.mask;
            if state.buffer[index].rem == 0 {
                low = mid + 1;
            } else {
                high = mid;
            }
        }
        state.buffer.len() - low
    }

    /// Whether no value is retained; see [`Self::len`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let state = self.shared.state();
        state.buffer[self.shared.index(state.pos.wrapping_sub(1))].rem == 0
    }

    /// The number of receivers.
    #[must_use]
    pub fn receiver_count(&self) -> usize {
        self.shared.state().receivers
    }

    /// Whether both senders belong to the same channel.
    #[must_use]
    pub fn same_channel(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }

    /// Waits for every receiver to be gone.
    pub async fn closed(&self) {
        loop {
            let notified = self.shared.last_receiver_dropped.notified();
            if self.shared.state().closed {
                return;
            }
            notified.await;
        }
    }

    /// The number of senders.
    #[must_use]
    pub fn strong_count(&self) -> usize {
        self.shared.state().senders
    }

    /// The number of weak senders.
    #[must_use]
    pub fn weak_count(&self) -> usize {
        self.shared.state().weak_senders
    }
}

fn new_receiver<T>(shared: Arc<Shared<T>>) -> Receiver<T> {
    let mut state = shared.state();
    assert!(state.receivers != MAX_RECEIVERS, "max receivers");
    if state.receivers == 0 {
        state.closed = false;
    }
    state.receivers += 1;
    let next = state.pos;
    drop(state);
    Receiver { shared, next }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        self.shared.state().senders += 1;
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let mut state = self.shared.state();
        state.senders -= 1;
        let wakers = if state.senders == 0 {
            state.close()
        } else {
            Vec::new()
        };
        drop(state);
        wake(wakers);
    }
}

impl<T> WeakSender<T> {
    /// A sender, if any is left.
    #[must_use]
    pub fn upgrade(&self) -> Option<Sender<T>> {
        let mut state = self.shared.state();
        if state.senders == 0 {
            return None;
        }
        state.senders += 1;
        drop(state);
        Some(Sender {
            shared: Arc::clone(&self.shared),
        })
    }

    /// The number of senders.
    #[must_use]
    pub fn strong_count(&self) -> usize {
        self.shared.state().senders
    }

    /// The number of weak senders.
    #[must_use]
    pub fn weak_count(&self) -> usize {
        self.shared.state().weak_senders
    }
}

impl<T> Clone for WeakSender<T> {
    fn clone(&self) -> Self {
        self.shared.state().weak_senders += 1;
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> Drop for WeakSender<T> {
    fn drop(&mut self) {
        self.shared.state().weak_senders -= 1;
    }
}

/// What [`Receiver::recv_with`] found.
enum Received<R> {
    /// The value at the receiver's position, given to the callback.
    Value(R),
    Empty,
    Closed,
    Lagged(u64),
}

impl<T> Receiver<T> {
    /// The number of values sent that this receiver has not received.
    #[must_use]
    pub fn len(&self) -> usize {
        let pos = self.shared.state().pos;
        usize::try_from(pos - self.next).unwrap_or(usize::MAX)
    }

    /// Whether this receiver has received every value sent.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether both receivers belong to the same channel.
    #[must_use]
    pub fn same_channel(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.shared, &other.shared)
    }

    /// The number of senders.
    #[must_use]
    pub fn sender_strong_count(&self) -> usize {
        self.shared.state().senders
    }

    /// The number of weak senders.
    #[must_use]
    pub fn sender_weak_count(&self) -> usize {
        self.shared.state().weak_senders
    }

    /// Whether every sender is gone.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.shared.state().senders == 0
    }

    /// Whether a receive would find something (a value, a lag, or the close) now.
    fn ready(&self) -> bool {
        let state = self.shared.state();
        let slot = &state.buffer[self.shared.index(self.next)];
        slot.pos == self.next
            || slot.pos.wrapping_add(self.shared.capacity()) != self.next
            || state.closed
    }

    /// Receive the value at this receiver's position with `take` (which sees it before this
    /// receiver's claim on it is released), as tokio's `recv_ref` does. If there is none,
    /// register `waiter` (with its waker) to be woken by the next send.
    fn recv_with<R>(
        &mut self,
        waiter: Option<(&mut Option<u64>, &std::task::Waker)>,
        take: impl FnOnce(&Option<T>) -> R,
    ) -> Received<R> {
        let shared = &*self.shared;
        shared.recv_locked(&mut shared.state(), &mut self.next, waiter, take)
    }
}

impl<T> Shared<T> {
    /// [`Receiver::recv_with`] for a receiver at position `next`, with the state locked.
    fn recv_locked<R>(
        &self,
        state: &mut State<T>,
        next: &mut u64,
        waiter: Option<(&mut Option<u64>, &std::task::Waker)>,
        take: impl FnOnce(&Option<T>) -> R,
    ) -> Received<R> {
        let capacity = self.capacity();
        let index = self.index(*next);
        if state.buffer[index].pos != *next {
            let next_pos = state.buffer[index].pos.wrapping_add(capacity);
            if next_pos == *next {
                if state.closed {
                    return Received::Closed;
                }
                if let Some((slot, waker)) = waiter {
                    state.waiters.register(slot, waker);
                }
                return Received::Empty;
            }
            // More than the capacity behind: skip to the oldest value retained.
            let oldest = state.pos.wrapping_sub(capacity);
            let missed = oldest.wrapping_sub(*next);
            if missed != 0 {
                *next = oldest;
                return Received::Lagged(missed);
            }
        }
        *next = next.wrapping_add(1);
        let slot = &mut state.buffer[index];
        let value = take(&slot.val);
        slot.rem -= 1;
        if slot.rem == 0 {
            slot.val = None;
        }
        Received::Value(value)
    }
}

impl<T: Clone> Receiver<T> {
    /// A new receiver of the values sent from now on.
    ///
    /// # Panics
    ///
    /// * If the channel has too many receivers
    #[must_use]
    pub fn resubscribe(&self) -> Self {
        new_receiver(Arc::clone(&self.shared))
    }

    /// Waits for the next value.
    ///
    /// Waiting receivers are woken in the order they started waiting. Cancelling the wait
    /// consumes nothing.
    ///
    /// # Errors
    ///
    /// * [`RecvError::Lagged`] if the receiver fell behind (it then continues from the oldest
    ///   value retained)
    /// * [`RecvError::Closed`] if every sender is gone and every value was received
    pub async fn recv(&mut self) -> Result<T, RecvError> {
        Recv {
            receiver: self,
            waiter: None,
        }
        .await
    }

    /// Receives the next value if there is one now.
    ///
    /// # Errors
    ///
    /// * [`TryRecvError::Empty`] if there is no value now
    /// * [`TryRecvError::Lagged`] if the receiver fell behind
    /// * [`TryRecvError::Closed`] if every sender is gone and every value was received
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        match self.recv_with(None, Clone::clone) {
            Received::Value(value) => value.ok_or(TryRecvError::Closed),
            Received::Empty => Err(TryRecvError::Empty),
            Received::Closed => Err(TryRecvError::Closed),
            Received::Lagged(missed) => Err(TryRecvError::Lagged(missed)),
        }
    }

    /// [`Self::recv`], blocking the calling thread (a simulated thread waits on the run's
    /// scheduler).
    ///
    /// # Errors
    ///
    /// * As [`Self::recv`]
    pub fn blocking_recv(&mut self) -> Result<T, RecvError> {
        block_on(Wait::Recv, self.recv())
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut state = self.shared.state();
        state.receivers -= 1;
        let until = state.pos;
        let last = state.receivers == 0;
        if last {
            state.closed = true;
        }
        drop(state);
        // Release this receiver's claim on the values it has not received.
        while self.next < until {
            match self.recv_with(None, |_| ()) {
                Received::Value(()) | Received::Lagged(_) => {}
                Received::Closed => break,
                Received::Empty => panic!("unexpected empty broadcast channel"),
            }
        }
        if last {
            self.shared.last_receiver_dropped.notify_waiters();
        }
    }
}

/// The future of [`Receiver::recv`]. Its place in the wait list is taken on its first pending
/// poll and given up when it completes or is dropped.
struct Recv<'a, T> {
    receiver: &'a mut Receiver<T>,
    waiter: Option<u64>,
}

impl<T: Clone> Future for Recv<'_, T> {
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if faults::spurious_pending_now(cx, || this.receiver.ready()) {
            return Poll::Pending;
        }
        let result = match this
            .receiver
            .recv_with(Some((&mut this.waiter, cx.waker())), Clone::clone)
        {
            Received::Value(value) => value.ok_or(RecvError::Closed),
            Received::Empty => return Poll::Pending,
            Received::Closed => Err(RecvError::Closed),
            Received::Lagged(missed) => Err(RecvError::Lagged(missed)),
        };
        this.leave();
        Poll::Ready(result)
    }
}

impl<T> Recv<'_, T> {
    fn leave(&mut self) {
        if self.waiter.is_some() {
            self.receiver.shared.state().waiters.remove(self.waiter.take());
        }
    }
}

impl<T> Drop for Recv<'_, T> {
    fn drop(&mut self) {
        self.leave();
    }
}

impl<T> fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "broadcast::Sender")
    }
}

impl<T> fmt::Debug for WeakSender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "broadcast::WeakSender")
    }
}

impl<T> fmt::Debug for Receiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "broadcast::Receiver")
    }
}
