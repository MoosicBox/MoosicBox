//! Multi-producer, single-consumer channel implementation for simulator runtime.
//!
//! This module provides MPSC channels with deterministic execution for testing.

use std::task::{Context, Poll};

use tokio::sync::mpsc;

/// Receiving end of an MPSC channel.
///
/// This wraps the underlying runtime's unbounded receiver and provides a consistent
/// API for receiving values from multiple senders. Values are received in FIFO order.
pub struct Receiver<T> {
    inner: mpsc::UnboundedReceiver<T>,
}

/// Sending end of an MPSC channel.
///
/// This wraps the underlying runtime's unbounded sender and can be cloned to create
/// multiple producers for a single consumer. The channel remains open as long as at
/// least one sender exists.
pub struct Sender<T> {
    inner: mpsc::UnboundedSender<T>,
}

/// Error returned when receiving from a channel fails.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RecvError {
    /// All senders have been dropped.
    #[error("Disconnected")]
    Disconnected,
}

/// Error returned when trying to receive from a channel without blocking.
#[derive(Debug, Clone, thiserror::Error)]
pub enum TryRecvError {
    /// The channel is currently empty.
    #[error("Empty")]
    Empty,
    /// All senders have been dropped.
    #[error("Disconnected")]
    Disconnected,
}

/// Error returned when receiving from a channel with a timeout.
#[derive(Debug, Clone, thiserror::Error)]
pub enum RecvTimeoutError {
    /// The timeout expired before a value was received.
    #[error("Timeout")]
    Timeout,
    /// All senders have been dropped.
    #[error("Disconnected")]
    Disconnected,
}

impl From<mpsc::error::TryRecvError> for TryRecvError {
    fn from(err: mpsc::error::TryRecvError) -> Self {
        match err {
            mpsc::error::TryRecvError::Empty => Self::Empty,
            mpsc::error::TryRecvError::Disconnected => Self::Disconnected,
        }
    }
}

impl From<RecvError> for TryRecvError {
    fn from(_: RecvError) -> Self {
        Self::Disconnected
    }
}

impl From<RecvTimeoutError> for TryRecvError {
    fn from(_: RecvTimeoutError) -> Self {
        Self::Disconnected
    }
}

impl From<RecvTimeoutError> for RecvError {
    fn from(_: RecvTimeoutError) -> Self {
        Self::Disconnected
    }
}

impl From<RecvError> for RecvTimeoutError {
    fn from(_: RecvError) -> Self {
        Self::Disconnected
    }
}

impl<T> Receiver<T> {
    /// Receive a value, blocking until one is available.
    ///
    /// # Errors
    ///
    /// * Returns `RecvError::Disconnected` if all senders have been dropped
    pub fn recv(&mut self) -> Result<T, RecvError> {
        self.inner.blocking_recv().ok_or(RecvError::Disconnected)
    }

    /// Try to receive a value without blocking.
    ///
    /// # Errors
    ///
    /// * Returns `TryRecvError::Empty` if no data is available
    /// * Returns `TryRecvError::Disconnected` if all senders have been dropped
    pub fn try_recv(&mut self) -> Result<T, TryRecvError> {
        Ok(self.inner.try_recv()?)
    }

    /// Receive a value with a timeout.
    ///
    /// # Errors
    ///
    /// * Returns `RecvTimeoutError::Timeout` if timeout expires
    /// * Returns `RecvTimeoutError::Disconnected` if all senders have been dropped
    pub fn recv_timeout(&mut self, timeout: std::time::Duration) -> Result<T, RecvTimeoutError> {
        crate::runtime::Handle::current().block_on(self.recv_timeout_async(timeout))
    }

    /// Receive a value with a timeout.
    ///
    /// # Errors
    ///
    /// * Returns `RecvTimeoutError::Timeout` if timeout expires
    /// * Returns `RecvTimeoutError::Disconnected` if all senders have been dropped
    pub async fn recv_timeout_async(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<T, RecvTimeoutError> {
        crate::select! {
            result = self.recv_async() => {
                Ok(result?)
            }
            () = crate::time::sleep(timeout) => {
                Err(RecvTimeoutError::Timeout)
            }
        }
    }

    /// Poll to receive a value (for async contexts).
    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.inner.poll_recv(cx)
    }

    /// Receive a value by polling the channel in an async context.
    ///
    /// # Errors
    ///
    /// * Returns `RecvError::Disconnected` if all senders have been dropped
    pub async fn recv_async(&mut self) -> Result<T, RecvError> {
        self.inner.recv().await.ok_or(RecvError::Disconnected)
    }
}

// impl<T> Clone for Receiver<T> {
//     fn clone(&self) -> Self {
//         Self {
//             inner: self.inner.clone(),
//         }
//     }
// }

/// Error returned when sending to a channel fails.
#[derive(Debug, thiserror::Error)]
pub enum SendError<T> {
    /// The receiver has been dropped.
    #[error("Disconnected")]
    Disconnected(T),
}

impl<T> From<mpsc::error::SendError<T>> for SendError<T> {
    fn from(e: mpsc::error::SendError<T>) -> Self {
        Self::Disconnected(e.0)
    }
}

/// Error returned when trying to send to a channel without blocking.
#[derive(Debug, thiserror::Error)]
pub enum TrySendError<T> {
    /// The channel is full.
    #[error("Full")]
    Full(T),
    /// The receiver has been dropped.
    #[error("Disconnected")]
    Disconnected(T),
}

impl<T> From<mpsc::error::TrySendError<T>> for TrySendError<T> {
    fn from(err: mpsc::error::TrySendError<T>) -> Self {
        match err {
            mpsc::error::TrySendError::Full(t) => Self::Full(t),
            mpsc::error::TrySendError::Closed(t) => Self::Disconnected(t),
        }
    }
}

impl<T> From<SendError<T>> for TrySendError<T> {
    fn from(e: SendError<T>) -> Self {
        match e {
            SendError::Disconnected(t) => Self::Disconnected(t),
        }
    }
}

impl<T> From<mpsc::error::SendError<T>> for TrySendError<T> {
    fn from(e: mpsc::error::SendError<T>) -> Self {
        match e {
            mpsc::error::SendError(t) => Self::Disconnected(t),
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
        Ok(self.inner.send(value)?)
    }

    /// Send a value asynchronously.
    ///
    /// # Errors
    ///
    /// * Returns `SendError` if all receivers have been dropped
    #[allow(clippy::unused_async)]
    pub async fn send_async(&self, value: T) -> Result<(), SendError<T>> {
        Ok(self.inner.send(value)?)
    }

    /// Try to send a value without blocking.
    ///
    /// # Errors
    ///
    /// * Returns `TrySendError::Full` if the channel is at capacity
    /// * Returns `TrySendError::Disconnected` if all receivers have been dropped
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        Ok(self.inner.send(value)?)
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
    let (tx, rx) = mpsc::unbounded_channel();
    (Sender { inner: tx }, Receiver { inner: rx })
}

// /// Create a bounded channel.
// #[must_use]
// pub fn bounded<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
//     let (tx, rx) = mpsc::channel(capacity);
//     (Sender { inner: tx }, Receiver { inner: rx })
// }
