//! Capacity-limited async channels without implicit blocking or runtime timers.
//!
//! The queue delegates to executor-independent Tokio synchronization. Scheduling
//! belongs to the selected executor. No raw sender/receiver conversions or Deref
//! implementation are exposed, including through reserved permits.
//!
//! Blocking and backend-timed operations are deliberately unavailable:
//! ```compile_fail
//! use switchy_async::sync::mpsc::bounded;
//! let (sender, _) = bounded::channel(1);
//! sender.blocking_send(1).unwrap();
//! ```
//! Timed sends use the selected Switchy timer rather than Tokio's timer.
use std::task::{Context, Poll};

/// Errors from bounded channel operations.
pub use tokio::sync::mpsc::error;

/// Sending half of a bounded async channel.
#[derive(Debug)]
pub struct Sender<T>(tokio::sync::mpsc::Sender<T>);
/// Unique receiving half of a bounded async channel.
#[derive(Debug)]
pub struct Receiver<T>(tokio::sync::mpsc::Receiver<T>);
/// Borrowed reservation of one channel slot.
pub struct Permit<'a, T>(tokio::sync::mpsc::Permit<'a, T>);
/// Owned reservation of one channel slot.
pub struct OwnedPermit<T>(tokio::sync::mpsc::OwnedPermit<T>);

/// Creates a bounded channel.
///
/// # Panics
/// * If capacity is zero or exceeds the backend semaphore limit.
#[must_use]
pub fn channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let (sender, receiver) = tokio::sync::mpsc::channel(capacity);
    (Sender(sender), Receiver(receiver))
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Sender<T> {
    /// Sends a value, waiting for capacity.
    /// # Errors
    /// * Returns the value if the receiver is closed.
    pub async fn send(&self, value: T) -> Result<(), error::SendError<T>> {
        self.0.send(value).await
    }
    /// Sends a value before the selected backend's timeout expires.
    ///
    /// The value is retained while waiting for capacity. Timeout cancels the
    /// reservation without publishing it. Immediately available capacity wins
    /// over a zero timeout, matching ordinary send readiness.
    ///
    /// # Errors
    /// * Returns Closed with the original value if the receiver closes.
    /// * Returns Timeout with the original value if capacity is not acquired in time.
    #[cfg(feature = "time")]
    pub async fn send_timeout(
        &self,
        value: T,
        timeout: std::time::Duration,
    ) -> Result<(), error::SendTimeoutError<T>> {
        match crate::time::timeout(timeout, self.reserve()).await {
            Ok(Ok(permit)) => {
                permit.send(value);
                Ok(())
            }
            Ok(Err(_)) => Err(error::SendTimeoutError::Closed(value)),
            Err(_) => Err(error::SendTimeoutError::Timeout(value)),
        }
    }

    /// Attempts a send without waiting.
    /// # Errors
    /// * Returns Full or Closed, preserving the value.
    pub fn try_send(&self, value: T) -> Result<(), error::TrySendError<T>> {
        self.0.try_send(value)
    }
    /// Reserves capacity without sending a value.
    /// # Errors
    /// * If the receiver is closed.
    pub async fn reserve(&self) -> Result<Permit<'_, T>, error::SendError<()>> {
        self.0.reserve().await.map(Permit)
    }
    /// Reserves capacity while owning this sender.
    /// # Errors
    /// * If the receiver is closed.
    pub async fn reserve_owned(self) -> Result<OwnedPermit<T>, error::SendError<()>> {
        self.0.reserve_owned().await.map(OwnedPermit)
    }
    /// Returns remaining capacity, excluding reserved slots.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }
    /// Returns whether the receiver has closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.0.is_closed()
    }
    /// Waits until the receiver closes.
    pub async fn closed(&self) {
        self.0.closed().await;
    }
}

impl<T> Receiver<T> {
    /// Receives the next value, or None after closure and draining reservations.
    pub async fn recv(&mut self) -> Option<T> {
        self.0.recv().await
    }
    /// Polls for the next value without consuming it on Pending.
    pub fn poll_recv(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        self.0.poll_recv(cx)
    }
    /// Attempts a receive without waiting.
    /// # Errors
    /// * If empty or disconnected.
    pub fn try_recv(&mut self) -> Result<T, error::TryRecvError> {
        self.0.try_recv()
    }
    /// Prevents new sends while allowing buffered values and reservations to drain.
    pub fn close(&mut self) {
        self.0.close();
    }
}

impl<T> Permit<'_, T> {
    /// Publishes a value using the reserved slot, even after receiver close.
    pub fn send(self, value: T) {
        self.0.send(value);
    }
}
impl<T> OwnedPermit<T> {
    /// Publishes a value and returns the Switchy sender.
    #[must_use]
    pub fn send(self, value: T) -> Sender<T> {
        Sender(self.0.send(value))
    }
    /// Releases capacity and returns the Switchy sender.
    #[must_use]
    pub fn release(self) -> Sender<T> {
        Sender(self.0.release())
    }
}
