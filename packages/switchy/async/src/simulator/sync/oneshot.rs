//! A channel for sending a single value between tasks, with tokio's API.
//!
//! Mirrors `tokio::sync::oneshot`. Each side has at most one waiter (the receiving task, or the
//! task waiting in [`Sender::closed`]), so the order of wakeups follows the run's schedule.

use std::{
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    task::{Context, Poll, Waker},
};

use super::blocking::{Wait, block_on};
use crate::simulator::faults;

/// Oneshot error types.
pub mod error {
    use std::fmt;

    /// Error returned by awaiting a [`super::Receiver`] when the sender was dropped without
    /// sending (or the receiver was closed first).
    #[derive(Debug, Eq, PartialEq, Clone)]
    pub struct RecvError(pub(super) ());

    impl fmt::Display for RecvError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "channel closed")
        }
    }

    impl std::error::Error for RecvError {}

    /// Error returned by [`super::Receiver::try_recv`].
    #[derive(Debug, Eq, PartialEq, Clone)]
    pub enum TryRecvError {
        /// No value was sent yet.
        Empty,
        /// The sender was dropped without sending, or the value was already received.
        Closed,
    }

    impl fmt::Display for TryRecvError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::Empty => write!(f, "channel empty"),
                Self::Closed => write!(f, "channel closed"),
            }
        }
    }

    impl std::error::Error for TryRecvError {}
}

use error::{RecvError, TryRecvError};

struct State<T> {
    value: Option<T>,
    /// The sender sent or was dropped.
    complete: bool,
    /// The receiver was closed or dropped.
    closed: bool,
    rx_waker: Option<Waker>,
    tx_waker: Option<Waker>,
}

struct Inner<T> {
    state: Mutex<State<T>>,
}

impl<T> Inner<T> {
    fn state(&self) -> MutexGuard<'_, State<T>> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Mark the sender done (sent or dropped), waking the receiver.
    fn complete(&self) {
        let mut state = self.state();
        state.complete = true;
        let waker = if state.closed {
            None
        } else {
            state.rx_waker.take()
        };
        drop(state);
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Mark the receiver closed, waking a sender waiting in `closed`. Returns the value sent,
    /// if any, for the caller to drop.
    fn close(&self) -> Option<T> {
        let mut state = self.state();
        state.closed = true;
        let waker = if state.complete {
            None
        } else {
            state.tx_waker.take()
        };
        let value = state.value.take();
        drop(state);
        if let Some(waker) = waker {
            waker.wake();
        }
        value
    }
}

/// Creates a new oneshot channel, returning the sender and receiver halves.
#[must_use]
pub fn channel<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Inner {
        state: Mutex::new(State {
            value: None,
            complete: false,
            closed: false,
            rx_waker: None,
            tx_waker: None,
        }),
    });
    (
        Sender {
            inner: Some(Arc::clone(&inner)),
        },
        Receiver { inner: Some(inner) },
    )
}

/// Sends a value to the associated [`Receiver`].
pub struct Sender<T> {
    inner: Option<Arc<Inner<T>>>,
}

/// Receives a value from the associated [`Sender`], by awaiting it.
pub struct Receiver<T> {
    inner: Option<Arc<Inner<T>>>,
}

impl<T> fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sender").finish_non_exhaustive()
    }
}

impl<T> fmt::Debug for Receiver<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Receiver").finish_non_exhaustive()
    }
}

impl<T> Sender<T> {
    fn inner(&self) -> &Inner<T> {
        self.inner.as_deref().expect("the sender is used until sent")
    }

    /// Sends `value`, consuming the sender.
    ///
    /// # Errors
    ///
    /// * The value, if the receiver was closed or dropped
    ///
    /// # Panics
    ///
    /// * Never: a sender sends at most once, as it is consumed
    pub fn send(mut self, value: T) -> Result<(), T> {
        let inner = self.inner.take().expect("the sender is used until sent");
        let mut state = inner.state();
        state.complete = true;
        if state.closed {
            return Err(value);
        }
        state.value = Some(value);
        let waker = state.rx_waker.take();
        drop(state);
        if let Some(waker) = waker {
            waker.wake();
        }
        Ok(())
    }

    /// Waits for the receiver to be closed or dropped.
    pub async fn closed(&mut self) {
        std::future::poll_fn(|cx| self.poll_closed(cx)).await;
    }

    /// Whether the receiver was closed or dropped.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.inner().state().closed
    }

    /// Polls whether the receiver was closed or dropped, registering the task to be woken when
    /// it is.
    pub fn poll_closed(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        let inner = self.inner();
        if faults::spurious_pending_now(cx, || inner.state().closed) {
            return Poll::Pending;
        }
        let mut state = inner.state();
        if state.closed {
            return Poll::Ready(());
        }
        match &mut state.tx_waker {
            Some(waker) if waker.will_wake(cx.waker()) => {}
            waker => *waker = Some(cx.waker().clone()),
        }
        Poll::Pending
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        if let Some(inner) = &self.inner {
            inner.complete();
        }
    }
}

impl<T> Receiver<T> {
    /// Closes the channel: the sender can no longer send, and learns so. A value already sent
    /// can still be received.
    pub fn close(&mut self) {
        if let Some(inner) = &self.inner {
            let mut state = inner.state();
            state.closed = true;
            let waker = if state.complete {
                None
            } else {
                state.tx_waker.take()
            };
            drop(state);
            if let Some(waker) = waker {
                waker.wake();
            }
        }
    }

    /// Whether the value was received or an error was returned, so the receiver must not be
    /// polled again.
    #[must_use]
    pub const fn is_terminated(&self) -> bool {
        self.inner.is_none()
    }

    /// Whether there is no value to receive now.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.as_ref().is_none_or(|inner| {
            let state = inner.state();
            !state.complete || state.value.is_none()
        })
    }

    /// Receives the value if it was sent.
    ///
    /// # Errors
    ///
    /// * [`TryRecvError::Empty`] if no value was sent yet
    /// * [`TryRecvError::Closed`] if the sender was dropped without sending, the receiver was
    ///   closed first, or the value was already received
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        let result = if let Some(inner) = &self.inner {
            let mut state = inner.state();
            if state.complete {
                state.value.take().ok_or(TryRecvError::Closed)
            } else if state.closed {
                Err(TryRecvError::Closed)
            } else {
                return Err(TryRecvError::Empty);
            }
        } else {
            Err(TryRecvError::Closed)
        };
        self.inner = None;
        result
    }

    /// Waits for the value, blocking the calling thread (a simulated thread waits on the run's
    /// scheduler).
    ///
    /// # Errors
    ///
    /// * If the sender was dropped without sending
    pub fn blocking_recv(self) -> Result<T, RecvError> {
        block_on(Wait::Recv, self)
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        if let Some(inner) = &self.inner {
            drop(inner.close());
        }
    }
}

impl<T> Future for Receiver<T> {
    type Output = Result<T, RecvError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let inner = this.inner.as_deref().expect("called after complete");
        if faults::spurious_pending_now(cx, || {
            let state = inner.state();
            state.complete || state.closed
        }) {
            return Poll::Pending;
        }
        let mut state = inner.state();
        let result = if state.complete {
            state.value.take().ok_or(RecvError(()))
        } else if state.closed {
            Err(RecvError(()))
        } else {
            match &mut state.rx_waker {
                Some(waker) if waker.will_wake(cx.waker()) => {}
                waker => *waker = Some(cx.waker().clone()),
            }
            return Poll::Pending;
        };
        drop(state);
        this.inner = None;
        Poll::Ready(result)
    }
}
