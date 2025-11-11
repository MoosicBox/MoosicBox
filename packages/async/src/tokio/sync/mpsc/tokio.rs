//! Multi-producer, single-consumer channel implementation for tokio runtime.
//!
//! This wraps the MPMC channel implementation to provide MPSC-specific types and error handling.

use std::task::{Context, Poll};

use crate::tokio::sync::mpmc;

/// Receiver wrapper that adds `poll_recv` method
pub struct Receiver<T> {
    inner: mpmc::Receiver<T>,
}

/// Sender wrapper
pub struct Sender<T> {
    inner: mpmc::Sender<T>,
}
impl<T> Receiver<T> {
    /// Receive a value, blocking until one is available.
    ///
    /// # Errors
    ///
    /// * Returns `RecvError::Disconnected` if all senders have been dropped
    pub fn recv(&mut self) -> Result<T, mpmc::RecvError> {
        self.inner.recv()
    }

    /// Try to receive a value without blocking.
    ///
    /// # Errors
    ///
    /// * Returns `TryRecvError::Empty` if no data is available
    /// * Returns `TryRecvError::Disconnected` if all senders have been dropped
    pub fn try_recv(&mut self) -> Result<T, mpmc::TryRecvError> {
        self.inner.try_recv()
    }

    /// Receive a value with a timeout.
    ///
    /// # Errors
    ///
    /// * Returns `RecvTimeoutError::Timeout` if timeout expires
    /// * Returns `RecvTimeoutError::Disconnected` if all senders have been dropped
    pub fn recv_timeout(
        &mut self,
        timeout: std::time::Duration,
    ) -> Result<T, mpmc::RecvTimeoutError> {
        self.inner.recv_timeout(timeout)
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
    pub async fn recv_async(&mut self) -> Result<T, mpmc::RecvError> {
        self.inner.recv_async().await
    }
}

/// Error returned when sending to a channel fails.
#[derive(Debug, thiserror::Error)]
pub enum SendError<T> {
    /// The receiver has been dropped.
    #[error("Disconnected")]
    Disconnected(T),
}

impl<T> From<mpmc::SendError<T>> for SendError<T> {
    fn from(e: mpmc::SendError<T>) -> Self {
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

impl<T> From<mpmc::TrySendError<T>> for TrySendError<T> {
    fn from(err: mpmc::TrySendError<T>) -> Self {
        match err {
            mpmc::TrySendError::Full(t) => Self::Full(t),
            mpmc::TrySendError::Disconnected(t) => Self::Disconnected(t),
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

impl<T> From<mpmc::SendError<T>> for TrySendError<T> {
    fn from(e: mpmc::SendError<T>) -> Self {
        match e {
            mpmc::SendError(t) => Self::Disconnected(t),
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
    let (tx, rx) = mpmc::unbounded();
    (Sender { inner: tx }, Receiver { inner: rx })
}
