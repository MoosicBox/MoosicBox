//! Multi-producer, multi-consumer channel implementation for tokio runtime.
//!
//! This wraps flume to provide additional methods needed by the codebase.

use std::task::{Context, Poll};

/// Receiving end of an MPMC channel.
///
/// This wraps a flume receiver to provide both blocking and async receive operations.
/// Multiple receivers can consume from the same channel, and values are distributed
/// among them.
pub struct Receiver<T> {
    inner: flume::Receiver<T>,
}

/// Sending end of an MPMC channel.
///
/// This wraps a flume sender to provide both blocking and async send operations.
/// Multiple senders can send to the same channel, and the channel remains open
/// as long as at least one sender or receiver exists.
pub struct Sender<T> {
    inner: flume::Sender<T>,
}

impl<T> std::fmt::Debug for Sender<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sender").finish_non_exhaustive()
    }
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
    pub fn poll_recv(&self, _cx: &mut Context<'_>) -> Poll<Option<T>> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_unbounded_channel_send_and_try_recv() {
        let (tx, rx) = unbounded::<i32>();

        // Send a value
        tx.send(42).unwrap();

        // Try to receive it
        let value = rx.try_recv().unwrap();
        assert_eq!(value, 42);
    }

    #[test_log::test]
    fn test_bounded_channel_send_and_try_recv() {
        let (tx, rx) = bounded::<i32>(10);

        tx.send(100).unwrap();

        let value = rx.try_recv().unwrap();
        assert_eq!(value, 100);
    }

    #[test_log::test]
    fn test_unbounded_channel_try_recv_empty() {
        let (_tx, rx) = unbounded::<i32>();

        // Should return Empty error when no messages
        let result = rx.try_recv();
        assert!(matches!(result, Err(TryRecvError::Empty)));
    }

    #[test_log::test]
    fn test_unbounded_channel_try_recv_disconnected() {
        let (tx, rx) = unbounded::<i32>();

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
        assert!(result.is_err());
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
        let (tx1, rx) = unbounded::<i32>();
        let tx2 = tx1.clone();

        tx1.send(1).unwrap();
        tx2.send(2).unwrap();

        // Order is preserved - FIFO
        assert_eq!(rx.try_recv().unwrap(), 1);
        assert_eq!(rx.try_recv().unwrap(), 2);
    }

    #[test_log::test]
    fn test_receiver_clone() {
        let (tx, rx1) = unbounded::<i32>();
        let rx2 = rx1.clone();

        tx.send(1).unwrap();
        tx.send(2).unwrap();

        // Either receiver can consume messages
        let v1 = rx1.try_recv().unwrap();
        let v2 = rx2.try_recv().unwrap();

        assert!(v1 == 1 || v1 == 2);
        assert!(v2 == 1 || v2 == 2);
        assert_ne!(v1, v2);
    }

    #[test_log::test]
    fn test_bounded_channel_full() {
        let (tx, _rx) = bounded::<i32>(2);

        tx.try_send(1).unwrap();
        tx.try_send(2).unwrap();

        // Should return Full error when at capacity
        let result = tx.try_send(3);
        assert!(matches!(result, Err(TrySendError::Full(3))));
    }

    #[test_log::test]
    fn test_multiple_messages() {
        let (tx, rx) = unbounded::<String>();

        tx.send("first".to_string()).unwrap();
        tx.send("second".to_string()).unwrap();
        tx.send("third".to_string()).unwrap();

        assert_eq!(rx.try_recv().unwrap(), "first");
        assert_eq!(rx.try_recv().unwrap(), "second");
        assert_eq!(rx.try_recv().unwrap(), "third");
        assert!(matches!(rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test_log::test]
    fn test_recv_timeout_returns_data_when_available() {
        let (tx, rx) = unbounded::<i32>();

        // Send data first
        tx.send(99).unwrap();

        // recv_timeout should return the data immediately
        let result = rx.recv_timeout(std::time::Duration::from_secs(1));
        assert_eq!(result.unwrap(), 99);
    }

    #[test_log::test]
    fn test_recv_timeout_returns_timeout_when_empty() {
        let (_tx, rx) = unbounded::<i32>();

        // recv_timeout should return Timeout when no data is available
        let result = rx.recv_timeout(std::time::Duration::from_millis(10));
        assert!(matches!(result, Err(RecvTimeoutError::Timeout)));
    }

    #[test_log::test]
    fn test_recv_timeout_returns_disconnected_when_senders_dropped() {
        let (tx, rx) = unbounded::<i32>();

        // Drop all senders
        drop(tx);

        // recv_timeout should return Disconnected
        let result = rx.recv_timeout(std::time::Duration::from_millis(10));
        assert!(matches!(result, Err(RecvTimeoutError::Disconnected)));
    }

    #[test_log::test]
    fn test_poll_recv_returns_ready_when_data_available() {
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(|data| RawWaker::new(data, &VTABLE), |_| {}, |_| {}, |_| {});

        let raw_waker = RawWaker::new(std::ptr::null(), &VTABLE);
        let waker = unsafe { Waker::from_raw(raw_waker) };
        let mut cx = Context::from_waker(&waker);

        let (tx, rx) = unbounded::<i32>();

        tx.send(42).unwrap();

        let result = rx.poll_recv(&mut cx);
        assert!(matches!(result, Poll::Ready(Some(42))));
    }

    #[test_log::test]
    fn test_poll_recv_returns_pending_when_empty() {
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(|data| RawWaker::new(data, &VTABLE), |_| {}, |_| {}, |_| {});

        let raw_waker = RawWaker::new(std::ptr::null(), &VTABLE);
        let waker = unsafe { Waker::from_raw(raw_waker) };
        let mut cx = Context::from_waker(&waker);

        let (_tx, rx) = unbounded::<i32>();

        let result = rx.poll_recv(&mut cx);
        assert!(matches!(result, Poll::Pending));
    }

    #[test_log::test]
    fn test_poll_recv_returns_none_when_disconnected() {
        use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(|data| RawWaker::new(data, &VTABLE), |_| {}, |_| {}, |_| {});

        let raw_waker = RawWaker::new(std::ptr::null(), &VTABLE);
        let waker = unsafe { Waker::from_raw(raw_waker) };
        let mut cx = Context::from_waker(&waker);

        let (tx, rx) = unbounded::<i32>();

        // Drop the sender to disconnect
        drop(tx);

        let result = rx.poll_recv(&mut cx);
        assert!(matches!(result, Poll::Ready(None)));
    }
}
