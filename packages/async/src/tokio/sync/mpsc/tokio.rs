//! Multi-producer, single-consumer channel implementation for tokio runtime.
//!
//! This wraps the MPMC channel implementation to provide MPSC-specific types and error handling.

use std::task::{Context, Poll};

use crate::tokio::sync::mpmc;

/// Receiving end of an MPSC channel.
///
/// This wraps an MPMC receiver to provide MPSC semantics with both blocking and async
/// receive operations. Only one receiver exists per channel, consuming values in FIFO order.
pub struct Receiver<T> {
    inner: mpmc::Receiver<T>,
}

/// Sending end of an MPSC channel.
///
/// This wraps an MPMC sender to provide MPSC semantics with both blocking and async
/// send operations. Multiple senders can send to the same channel, and the channel
/// remains open as long as at least one sender exists.
pub struct Sender<T> {
    inner: mpmc::Sender<T>,
}

impl<T> std::fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sender").finish_non_exhaustive()
    }
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
#[derive(thiserror::Error)]
pub enum SendError<T> {
    /// The receiver has been dropped.
    #[error("Disconnected")]
    Disconnected(T),
}

impl<T> std::fmt::Debug for SendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disconnected(_t) => f
                .debug_tuple("SendError::Disconnected")
                .finish_non_exhaustive(),
        }
    }
}

impl<T> From<mpmc::SendError<T>> for SendError<T> {
    fn from(e: mpmc::SendError<T>) -> Self {
        Self::Disconnected(e.0)
    }
}

/// Error returned when trying to send to a channel without blocking.
#[derive(thiserror::Error)]
pub enum TrySendError<T> {
    /// The channel is full.
    #[error("Full")]
    Full(T),
    /// The receiver has been dropped.
    #[error("Disconnected")]
    Disconnected(T),
}

impl<T> std::fmt::Debug for TrySendError<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full(_t) => f.debug_tuple("TrySendError::Full").finish_non_exhaustive(),
            Self::Disconnected(_t) => f
                .debug_tuple("TrySendError::Disconnected")
                .finish_non_exhaustive(),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_unbounded_channel_send_and_try_recv() {
        let (tx, mut rx) = unbounded::<i32>();

        // Send a value
        tx.send(42).unwrap();

        // Try to receive it
        let value = rx.try_recv().unwrap();
        assert_eq!(value, 42);
    }

    #[test_log::test]
    fn test_unbounded_channel_try_recv_empty() {
        let (_tx, mut rx) = unbounded::<i32>();

        // Should return Empty error when no messages
        let result = rx.try_recv();
        assert!(matches!(result, Err(mpmc::TryRecvError::Empty)));
    }

    #[test_log::test]
    fn test_unbounded_channel_try_recv_disconnected() {
        let (tx, mut rx) = unbounded::<i32>();

        // Drop the sender
        drop(tx);

        // Should return Disconnected error
        let result = rx.try_recv();
        assert!(matches!(result, Err(mpmc::TryRecvError::Disconnected)));
    }

    #[test_log::test]
    fn test_sender_send_after_receiver_dropped() {
        let (tx, rx) = unbounded::<i32>();

        // Drop the receiver
        drop(rx);

        // Should return Disconnected error
        let result = tx.send(42);
        assert!(matches!(result, Err(SendError::Disconnected(42))));
    }

    #[test_log::test]
    fn test_sender_try_send_after_receiver_dropped() {
        let (tx, rx) = unbounded::<i32>();

        // Drop the receiver
        drop(rx);

        // Should return Disconnected error
        let result = tx.try_send(99);
        assert!(matches!(result, Err(TrySendError::Disconnected(99))));
    }

    #[test_log::test]
    fn test_sender_clone() {
        let (tx1, mut rx) = unbounded::<i32>();
        let tx2 = tx1.clone();

        tx1.send(1).unwrap();
        tx2.send(2).unwrap();

        // Order is preserved - FIFO
        assert_eq!(rx.try_recv().unwrap(), 1);
        assert_eq!(rx.try_recv().unwrap(), 2);
    }

    #[test_log::test]
    fn test_multiple_messages() {
        let (tx, mut rx) = unbounded::<String>();

        tx.send("first".to_string()).unwrap();
        tx.send("second".to_string()).unwrap();
        tx.send("third".to_string()).unwrap();

        assert_eq!(rx.try_recv().unwrap(), "first");
        assert_eq!(rx.try_recv().unwrap(), "second");
        assert_eq!(rx.try_recv().unwrap(), "third");
        assert!(matches!(rx.try_recv(), Err(mpmc::TryRecvError::Empty)));
    }

    #[test_log::test]
    fn test_recv_timeout_returns_data_when_available() {
        let (tx, mut rx) = unbounded::<i32>();

        // Send data first
        tx.send(99).unwrap();

        // recv_timeout should return the data immediately
        let result = rx.recv_timeout(std::time::Duration::from_secs(1));
        assert_eq!(result.unwrap(), 99);
    }

    #[test_log::test]
    fn test_recv_timeout_returns_disconnected_when_senders_dropped() {
        let (tx, mut rx) = unbounded::<i32>();

        // Drop all senders
        drop(tx);

        // recv_timeout should return Disconnected
        let result = rx.recv_timeout(std::time::Duration::from_millis(10));
        assert!(matches!(result, Err(mpmc::RecvTimeoutError::Disconnected)));
    }

    #[test_log::test]
    fn test_try_send_error_conversion_from_send_error() {
        // Test the From<SendError<T>> for TrySendError<T> conversion
        let send_err: SendError<i32> = SendError::Disconnected(42);
        let try_send_err: TrySendError<i32> = send_err.into();
        assert!(matches!(try_send_err, TrySendError::Disconnected(42)));
    }

    #[test_log::test]
    fn test_try_send_error_conversion_from_mpmc_try_send_error() {
        // Test the From<mpmc::TrySendError<T>> for TrySendError<T> conversion with Full
        let mpmc_err = mpmc::TrySendError::Full(100);
        let try_send_err: TrySendError<i32> = mpmc_err.into();
        assert!(matches!(try_send_err, TrySendError::Full(100)));

        // Test with Disconnected
        let mpmc_err = mpmc::TrySendError::Disconnected(200);
        let try_send_err: TrySendError<i32> = mpmc_err.into();
        assert!(matches!(try_send_err, TrySendError::Disconnected(200)));
    }
}
