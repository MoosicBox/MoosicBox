//! Multi-producer, single-consumer channel implementation for simulator runtime.
//!
//! This module provides MPSC channels with deterministic execution for testing.

use std::task::{Context, Poll};

use tokio::sync::mpsc;

/// Receiving end of an MPSC channel.
///
/// This wraps the underlying runtime's unbounded receiver and provides a consistent
/// API for receiving values from multiple senders. Values are received in FIFO order.
#[derive(Debug)]
pub struct Receiver<T> {
    inner: mpsc::UnboundedReceiver<T>,
}

/// Sending end of an MPSC channel.
///
/// This wraps the underlying runtime's unbounded sender and can be cloned to create
/// multiple producers for a single consumer. The channel remains open as long as at
/// least one sender exists.
#[derive(Debug)]
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

impl<T> From<mpsc::error::SendError<T>> for SendError<T> {
    fn from(e: mpsc::error::SendError<T>) -> Self {
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
        assert!(matches!(result, Err(TryRecvError::Empty)));
    }

    #[test_log::test]
    fn test_unbounded_channel_try_recv_disconnected() {
        let (tx, mut rx) = unbounded::<i32>();

        // Drop the sender
        drop(tx);

        // Should return Disconnected error
        let result = rx.try_recv();
        assert!(matches!(result, Err(TryRecvError::Disconnected)));
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
    fn test_sender_try_send() {
        let (tx, mut rx) = unbounded::<i32>();

        // try_send should work like send for unbounded
        tx.try_send(100).unwrap();

        let value = rx.try_recv().unwrap();
        assert_eq!(value, 100);
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
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test_log::test(crate::internal_test(real_time))]
    async fn test_recv_async_success() {
        let (tx, mut rx) = unbounded::<i32>();

        tx.send(42).unwrap();

        let result = rx.recv_async().await;
        assert_eq!(result.unwrap(), 42);
    }

    #[test_log::test(crate::internal_test(real_time))]
    async fn test_recv_async_disconnected() {
        let (tx, mut rx) = unbounded::<i32>();

        // Drop sender
        drop(tx);

        let result = rx.recv_async().await;
        assert!(matches!(result, Err(RecvError::Disconnected)));
    }

    #[test_log::test(crate::internal_test(real_time))]
    async fn test_send_async() {
        let (tx, mut rx) = unbounded::<i32>();

        tx.send_async(99).await.unwrap();

        let value = rx.try_recv().unwrap();
        assert_eq!(value, 99);
    }

    #[test_log::test(crate::internal_test(real_time))]
    async fn test_recv_timeout_async_success() {
        let (tx, mut rx) = unbounded::<i32>();

        tx.send(123).unwrap();

        let result = rx
            .recv_timeout_async(std::time::Duration::from_millis(100))
            .await;
        assert_eq!(result.unwrap(), 123);
    }

    #[test_log::test(crate::internal_test(real_time))]
    async fn test_recv_timeout_async_timeout() {
        let (_tx, mut rx) = unbounded::<i32>();

        let result = rx
            .recv_timeout_async(std::time::Duration::from_millis(10))
            .await;
        assert!(matches!(result, Err(RecvTimeoutError::Timeout)));
    }

    #[test_log::test(crate::internal_test(real_time))]
    async fn test_recv_timeout_async_disconnected() {
        let (tx, mut rx) = unbounded::<i32>();

        // Drop sender
        drop(tx);

        let result = rx
            .recv_timeout_async(std::time::Duration::from_millis(100))
            .await;
        assert!(matches!(result, Err(RecvTimeoutError::Disconnected)));
    }

    #[test_log::test]
    fn test_try_send_disconnected() {
        let (tx, rx) = unbounded::<i32>();

        // Drop the receiver
        drop(rx);

        // Should return Disconnected error
        let result = tx.try_send(42);
        assert!(matches!(result, Err(TrySendError::Disconnected(42))));
    }

    #[test_log::test]
    fn test_recv_error_to_try_recv_error_conversion() {
        let recv_err = RecvError::Disconnected;
        let try_recv_err: TryRecvError = recv_err.into();
        assert!(matches!(try_recv_err, TryRecvError::Disconnected));
    }

    #[test_log::test]
    fn test_recv_timeout_error_to_try_recv_error_conversion() {
        let timeout_err = RecvTimeoutError::Timeout;
        let try_recv_err: TryRecvError = timeout_err.into();
        // RecvTimeoutError maps to Disconnected (both represent failure to receive)
        assert!(matches!(try_recv_err, TryRecvError::Disconnected));

        let disconnected_err = RecvTimeoutError::Disconnected;
        let try_recv_err2: TryRecvError = disconnected_err.into();
        assert!(matches!(try_recv_err2, TryRecvError::Disconnected));
    }

    #[test_log::test]
    fn test_recv_timeout_error_to_recv_error_conversion() {
        let timeout_err = RecvTimeoutError::Timeout;
        let recv_err: RecvError = timeout_err.into();
        assert!(matches!(recv_err, RecvError::Disconnected));
    }

    #[test_log::test]
    fn test_recv_error_to_recv_timeout_error_conversion() {
        let recv_err = RecvError::Disconnected;
        let timeout_err: RecvTimeoutError = recv_err.into();
        assert!(matches!(timeout_err, RecvTimeoutError::Disconnected));
    }

    #[test_log::test]
    fn test_send_error_debug_formatting() {
        let err: SendError<i32> = SendError::Disconnected(42);
        let debug_str = format!("{err:?}");
        // Debug should not leak the inner value but show the variant
        assert!(debug_str.contains("SendError::Disconnected"));
    }

    #[test_log::test]
    fn test_try_send_error_debug_formatting() {
        let full_err: TrySendError<i32> = TrySendError::Full(42);
        let disconnected_err: TrySendError<i32> = TrySendError::Disconnected(42);

        let full_debug = format!("{full_err:?}");
        let disconnected_debug = format!("{disconnected_err:?}");

        assert!(full_debug.contains("TrySendError::Full"));
        assert!(disconnected_debug.contains("TrySendError::Disconnected"));
    }

    #[test_log::test]
    fn test_send_error_to_try_send_error_conversion() {
        let send_err: SendError<i32> = SendError::Disconnected(99);
        let try_send_err: TrySendError<i32> = send_err.into();
        assert!(matches!(try_send_err, TrySendError::Disconnected(99)));
    }

    #[test_log::test]
    fn test_error_display_messages() {
        // RecvError
        let recv_err = RecvError::Disconnected;
        assert_eq!(recv_err.to_string(), "Disconnected");

        // TryRecvError
        let try_recv_empty = TryRecvError::Empty;
        let try_recv_disconnected = TryRecvError::Disconnected;
        assert_eq!(try_recv_empty.to_string(), "Empty");
        assert_eq!(try_recv_disconnected.to_string(), "Disconnected");

        // RecvTimeoutError
        let timeout_err = RecvTimeoutError::Timeout;
        let timeout_disconnected = RecvTimeoutError::Disconnected;
        assert_eq!(timeout_err.to_string(), "Timeout");
        assert_eq!(timeout_disconnected.to_string(), "Disconnected");

        // SendError
        let send_err: SendError<i32> = SendError::Disconnected(42);
        assert_eq!(send_err.to_string(), "Disconnected");

        // TrySendError
        let try_send_full: TrySendError<i32> = TrySendError::Full(42);
        let try_send_disconnected: TrySendError<i32> = TrySendError::Disconnected(42);
        assert_eq!(try_send_full.to_string(), "Full");
        assert_eq!(try_send_disconnected.to_string(), "Disconnected");
    }

    #[test_log::test]
    fn test_poll_recv_returns_pending_on_empty_channel() {
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(|data| RawWaker::new(data, &VTABLE), |_| {}, |_| {}, |_| {});

        let raw_waker = RawWaker::new(std::ptr::null(), &VTABLE);
        let waker = unsafe { Waker::from_raw(raw_waker) };
        let mut cx = Context::from_waker(&waker);

        let (_tx, mut rx) = unbounded::<i32>();

        // poll_recv on empty channel should return Pending
        let result = rx.poll_recv(&mut cx);
        assert!(matches!(result, Poll::Pending));
    }

    #[test_log::test]
    fn test_poll_recv_returns_none_on_disconnected_channel() {
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(|data| RawWaker::new(data, &VTABLE), |_| {}, |_| {}, |_| {});

        let raw_waker = RawWaker::new(std::ptr::null(), &VTABLE);
        let waker = unsafe { Waker::from_raw(raw_waker) };
        let mut cx = Context::from_waker(&waker);

        let (tx, mut rx) = unbounded::<i32>();
        drop(tx);

        // poll_recv on disconnected channel should return Ready(None)
        let result = rx.poll_recv(&mut cx);
        assert!(matches!(result, Poll::Ready(None)));
    }

    #[test_log::test]
    fn test_poll_recv_returns_value_on_available_data() {
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(|data| RawWaker::new(data, &VTABLE), |_| {}, |_| {}, |_| {});

        let raw_waker = RawWaker::new(std::ptr::null(), &VTABLE);
        let waker = unsafe { Waker::from_raw(raw_waker) };
        let mut cx = Context::from_waker(&waker);

        let (tx, mut rx) = unbounded::<i32>();
        tx.send(42).unwrap();

        // poll_recv should return Ready(Some(42))
        let result = rx.poll_recv(&mut cx);
        assert!(matches!(result, Poll::Ready(Some(42))));
    }
}
