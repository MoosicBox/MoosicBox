//! Multi-producer, single-consumer channel implementation for tokio runtime.
//! This wraps flume to provide additional methods needed by the codebase.

use std::task::{Context, Poll};

/// Receiving end of an MPSC channel.
///
/// This wraps a flume receiver to provide both blocking and async receive operations.
/// Only one receiver can exist per channel, consuming values in FIFO order.
pub struct Receiver<T> {
    inner: flume::Receiver<T>,
}

/// Sending end of an MPSC channel.
///
/// This wraps a flume sender to provide both blocking and async send operations.
/// Multiple senders can send to the same channel, and the channel remains open
/// as long as at least one sender exists.
pub struct Sender<T> {
    inner: flume::Sender<T>,
}

// Re-export error types
pub use flume::{RecvError, RecvTimeoutError, SendError, TryRecvError, TrySendError};

impl<T> Receiver<T> {
    /// Receive a value, blocking until one is available.
    ///
    /// # Errors
    ///
    /// * Returns `RecvError::Disconnected` if all senders have been dropped
    pub fn recv(&self) -> Result<T, RecvError> {
        self.inner.recv()
    }

    /// Try to receive a value without blocking.
    ///
    /// # Errors
    ///
    /// * Returns `TryRecvError::Empty` if no data is available
    /// * Returns `TryRecvError::Disconnected` if all senders have been dropped
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        self.inner.try_recv()
    }

    /// Receive a value with a timeout.
    ///
    /// # Errors
    ///
    /// * Returns `RecvTimeoutError::Timeout` if timeout expires
    /// * Returns `RecvTimeoutError::Disconnected` if all senders have been dropped
    pub fn recv_timeout(&self, timeout: std::time::Duration) -> Result<T, RecvTimeoutError> {
        self.inner.recv_timeout(timeout)
    }

    /// Poll to receive a value (for async contexts).
    pub fn poll_recv(&mut self, _cx: &mut Context<'_>) -> Poll<Option<T>> {
        match self.inner.try_recv() {
            Ok(value) => Poll::Ready(Some(value)),
            Err(TryRecvError::Empty) => Poll::Pending,
            Err(TryRecvError::Disconnected) => Poll::Ready(None),
        }
    }

    /// Receive a value by polling the channel in an async context.
    ///
    /// # Errors
    ///
    /// * Returns `RecvError::Disconnected` if all senders have been dropped
    pub async fn recv_async(&self) -> Result<T, RecvError> {
        self.inner.recv_async().await
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Sender<T> {
    /// Send a value, blocking if the channel is full.
    ///
    /// # Errors
    ///
    /// * Returns `SendError` if all receivers have been dropped
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        self.inner.send(value)
    }

    /// Send a value asynchronously.
    ///
    /// # Errors
    ///
    /// * Returns `SendError` if all receivers have been dropped
    pub async fn send_async(&self, value: T) -> Result<(), SendError<T>> {
        self.inner.send_async(value).await
    }

    /// Try to send a value without blocking.
    ///
    /// # Errors
    ///
    /// * Returns `TrySendError::Full` if the channel is at capacity
    /// * Returns `TrySendError::Disconnected` if all receivers have been dropped
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        self.inner.try_send(value)
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Create an unbounded channel.
#[must_use]
pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = flume::unbounded();
    (Sender { inner: tx }, Receiver { inner: rx })
}

/// Create a bounded channel.
#[must_use]
pub fn bounded<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let (tx, rx) = flume::bounded(capacity);
    (Sender { inner: tx }, Receiver { inner: rx })
}
