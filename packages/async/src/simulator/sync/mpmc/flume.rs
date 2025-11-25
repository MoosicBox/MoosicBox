//! Multi-producer, multi-consumer channel implementation for simulator runtime.
//!
//! This provides cooperative yielding to avoid deadlocks in deterministic execution.
//! The API is designed to be compatible with flume with Arc-based reference counting.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Waker;
use std::time::Duration;

use switchy_time::instant_now;

// Re-export flume error types for compatibility
pub use flume::{RecvError, RecvTimeoutError, SendError, TryRecvError, TrySendError};

/// Shared reference counting between senders and receivers.
///
/// This tracks how many senders and receivers exist for a channel, allowing
/// proper disconnection detection when all senders or receivers are dropped.
struct SharedCounts {
    sender_count: AtomicUsize,
    receiver_count: AtomicUsize,
}

/// Internal channel state.
///
/// This contains the actual message queue and waker lists for coordinating
/// between senders and receivers in an async context.
struct ChannelInner<T> {
    /// Queue of pending messages
    queue: VecDeque<T>,
    /// Maximum capacity (None for unbounded)
    capacity: Option<usize>,
    /// Wakers waiting for data to arrive
    receiver_wakers: Vec<Waker>,
    /// Wakers waiting for space to become available
    sender_wakers: Vec<Waker>,
}

impl<T> ChannelInner<T> {
    /// Creates a new channel inner state with the specified capacity.
    const fn new(capacity: Option<usize>) -> Self {
        Self {
            queue: VecDeque::new(),
            capacity,
            receiver_wakers: Vec::new(),
            sender_wakers: Vec::new(),
        }
    }

    /// Checks if the channel is at capacity.
    fn is_full(&self) -> bool {
        self.capacity.is_some_and(|cap| self.queue.len() >= cap)
    }

    /// Checks if the channel has no messages.
    fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Returns the number of messages in the channel.
    fn len(&self) -> usize {
        self.queue.len()
    }
}

/// Receiver for simulator runtime with cooperative yielding
pub struct Receiver<T> {
    inner: Arc<Mutex<ChannelInner<T>>>,
    counts: Arc<SharedCounts>,
}

/// Sender for simulator runtime
pub struct Sender<T> {
    inner: Arc<Mutex<ChannelInner<T>>>,
    counts: Arc<SharedCounts>,
}

/// Unbounded receiver - alias for Receiver
pub type UnboundedReceiver<T> = Receiver<T>;

/// Unbounded sender - alias for Sender
pub type UnboundedSender<T> = Sender<T>;

impl<T> Receiver<T> {
    /// Check if the channel is disconnected (all senders dropped)
    fn is_disconnected(&self) -> bool {
        self.counts.sender_count.load(Ordering::Relaxed) == 0
    }

    /// Receive a value, using cooperative yielding in simulator runtime.
    ///
    /// # Errors
    ///
    /// * Returns `RecvError::Disconnected` if all senders have been dropped
    pub fn recv(&self) -> Result<T, RecvError> {
        log::trace!("Channel recv() called");
        let mut iteration = 0;
        loop {
            // Try to get data without blocking
            match self.try_recv() {
                Ok(item) => {
                    log::trace!("Channel recv() got data after {iteration} iterations");
                    return Ok(item);
                }
                Err(TryRecvError::Disconnected) => {
                    log::trace!("Channel recv() disconnected after {iteration} iterations");
                    return Err(RecvError::Disconnected);
                }
                Err(TryRecvError::Empty) => {
                    cooperative_yield_with_backoff(iteration);
                    iteration += 1;
                }
            }
        }
    }

    /// Try to receive a value without blocking.
    ///
    /// # Errors
    ///
    /// * Returns `TryRecvError::Empty` if no data is available
    /// * Returns `TryRecvError::Disconnected` if all senders have been dropped
    ///
    /// # Panics
    ///
    /// * If the internal mutex is poisoned
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let mut inner = self.inner.lock().unwrap();

        inner.queue.pop_front().map_or_else(
            || {
                if self.is_disconnected() {
                    Err(TryRecvError::Disconnected)
                } else {
                    Err(TryRecvError::Empty)
                }
            },
            |item| Ok(item),
        )
    }

    /// Receive a value with a timeout (for compatibility).
    ///
    /// # Errors
    ///
    /// * Returns `RecvTimeoutError::Timeout` if timeout expires
    /// * Returns `RecvTimeoutError::Disconnected` if all senders have been dropped
    pub fn recv_timeout(&self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        let start = instant_now();
        let mut iteration = 0;

        loop {
            match self.try_recv() {
                Ok(item) => return Ok(item),
                Err(TryRecvError::Disconnected) => return Err(RecvTimeoutError::Disconnected),
                Err(TryRecvError::Empty) => {
                    if start.elapsed() >= timeout {
                        return Err(RecvTimeoutError::Timeout);
                    }
                    cooperative_yield_with_backoff(iteration);
                    iteration += 1;
                }
            }
        }
    }

    /// Poll to receive a value (for async contexts).
    ///
    /// # Panics
    ///
    /// * If the internal `Mutex` is poisoned
    pub fn poll_recv(&self, cx: &mut std::task::Context<'_>) -> std::task::Poll<Option<T>> {
        match self.try_recv() {
            Ok(value) => {
                // Wake up any waiting senders since we freed up space
                let mut inner = self.inner.lock().unwrap();
                for waker in inner.sender_wakers.drain(..) {
                    waker.wake();
                }
                drop(inner);
                std::task::Poll::Ready(Some(value))
            }
            Err(TryRecvError::Empty) => {
                // Register waker for when data becomes available
                let mut inner = self.inner.lock().unwrap();
                inner.receiver_wakers.push(cx.waker().clone());
                std::task::Poll::Pending
            }
            Err(TryRecvError::Disconnected) => std::task::Poll::Ready(None),
        }
    }

    /// Receive a value by polling the channel in an async context.
    ///
    /// # Errors
    ///
    /// * Returns `RecvError::Disconnected` if all senders have been dropped
    pub async fn recv_async(&self) -> Result<T, RecvError> {
        std::future::poll_fn(|cx: &mut std::task::Context<'_>| self.poll_recv(cx))
            .await
            .ok_or(RecvError::Disconnected)
    }

    /// Check if the channel is empty
    ///
    /// # Panics
    ///
    /// * If the internal `Mutex` is poisoned
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }

    /// Get the number of messages in the channel
    ///
    /// # Panics
    ///
    /// * If the internal `Mutex` is poisoned
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    /// Get the number of senders
    #[must_use]
    pub fn sender_count(&self) -> usize {
        self.counts.sender_count.load(Ordering::Relaxed)
    }

    /// Get the number of receivers
    #[must_use]
    pub fn receiver_count(&self) -> usize {
        self.counts.receiver_count.load(Ordering::Relaxed)
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        // Increment receiver count atomically
        let old_count = self.counts.receiver_count.fetch_add(1, Ordering::Relaxed);
        log::debug!(
            "Receiver cloned: receiver_count {} -> {}",
            old_count,
            old_count + 1
        );

        Self {
            inner: Arc::clone(&self.inner),
            counts: Arc::clone(&self.counts),
        }
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        // Decrement receiver count atomically
        let old_count = self.counts.receiver_count.fetch_sub(1, Ordering::AcqRel);
        log::debug!(
            "Receiver dropped: receiver_count {} -> {}",
            old_count,
            old_count - 1
        );

        // Wake up any waiting senders since they should now get disconnected errors
        if old_count == 1 {
            // Last receiver dropped
            if let Ok(mut inner) = self.inner.lock() {
                for waker in inner.sender_wakers.drain(..) {
                    waker.wake();
                }
            }
        }
    }
}

impl<T> Sender<T> {
    /// Check if the channel is disconnected (all receivers dropped)
    fn is_disconnected(&self) -> bool {
        self.counts.receiver_count.load(Ordering::Relaxed) == 0
    }

    /// Get the channel capacity
    fn capacity(&self) -> Option<usize> {
        self.inner.lock().unwrap().capacity
    }

    /// Send a value.
    ///
    /// # Errors
    ///
    /// * Returns `SendError` if all receivers have been dropped
    ///
    /// # Panics
    ///
    /// * If the internal mutex is poisoned
    pub fn send(&self, mut value: T) -> Result<(), SendError<T>> {
        log::trace!("Channel send() called");

        // Check disconnection first (like flume)
        if self.is_disconnected() {
            log::trace!("Channel send() failed: no receivers");
            return Err(SendError(value));
        }

        // For bounded channels, block until space or disconnection
        if self.capacity().is_some() {
            let mut iteration = 0;
            loop {
                match self.try_send(value) {
                    Ok(()) => {
                        log::trace!("Channel send() succeeded after {iteration} iterations");
                        return Ok(());
                    }
                    Err(TrySendError::Disconnected(v)) => {
                        log::trace!("Channel send() failed: disconnected");
                        return Err(SendError(v));
                    }
                    Err(TrySendError::Full(v)) => {
                        value = v;
                        cooperative_yield_with_backoff(iteration);
                        iteration += 1;

                        // Check disconnection again after yielding
                        if self.is_disconnected() {
                            log::trace!("Channel send() failed after blocking: no receivers");
                            return Err(SendError(value));
                        }
                    }
                }
            }
        } else {
            // Unbounded - just try once
            match self.try_send(value) {
                Ok(()) => {
                    log::trace!("Channel send() succeeded immediately");
                    Ok(())
                }
                Err(TrySendError::Disconnected(v)) => {
                    log::trace!("Channel send() failed: no receivers");
                    Err(SendError(v))
                }
                Err(TrySendError::Full(_)) => unreachable!("Unbounded channel cannot be full"),
            }
        }
    }

    /// Send a value asynchronously.
    ///
    /// # Errors
    ///
    /// * Returns `SendError` if all receivers have been dropped
    pub async fn send_async(&self, value: T) -> Result<(), SendError<T>> {
        // In simulator, just use sync send but yield to maintain async behavior
        let result = self.send(value);
        // Yield once to allow other tasks to run
        crate::task::yield_now().await;
        result
    }

    /// Try to send a value without blocking.
    ///
    /// # Errors
    ///
    /// * Returns `TrySendError::Full` if the channel is at capacity
    /// * Returns `TrySendError::Disconnected` if all receivers have been dropped
    ///
    /// # Panics
    ///
    /// * If the internal `Mutex` is poisoned
    pub fn try_send(&self, value: T) -> Result<(), TrySendError<T>> {
        log::trace!("Channel try_send() called");

        // Check disconnection first
        if self.is_disconnected() {
            log::trace!("Channel try_send() failed: no receivers");
            return Err(TrySendError::Disconnected(value));
        }

        let mut inner = self.inner.lock().unwrap();

        // Double-check after acquiring lock
        if self.is_disconnected() {
            log::trace!("Channel try_send() failed: no receivers (double-check)");
            return Err(TrySendError::Disconnected(value));
        }

        // Check capacity
        if inner.is_full() {
            log::trace!("Channel try_send() failed: channel full");
            return Err(TrySendError::Full(value));
        }

        inner.queue.push_back(value);
        log::trace!(
            "Channel try_send() succeeded (queue len: {})",
            inner.queue.len()
        );

        // Wake up any waiting receivers since we added data
        for waker in inner.receiver_wakers.drain(..) {
            waker.wake();
        }

        drop(inner);

        Ok(())
    }

    /// Check if the channel is empty
    ///
    /// # Panics
    ///
    /// * If the internal `Mutex` is poisoned
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }

    /// Check if the channel is full
    ///
    /// # Panics
    ///
    /// * If the internal `Mutex` is poisoned
    #[must_use]
    pub fn is_full(&self) -> bool {
        self.inner.lock().unwrap().is_full()
    }

    /// Get the number of messages in the channel
    ///
    /// # Panics
    ///
    /// * If the internal `Mutex` is poisoned
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    /// Get the number of senders
    #[must_use]
    pub fn sender_count(&self) -> usize {
        self.counts.sender_count.load(Ordering::Relaxed)
    }

    /// Get the number of receivers
    #[must_use]
    pub fn receiver_count(&self) -> usize {
        self.counts.receiver_count.load(Ordering::Relaxed)
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        // Increment sender count atomically
        let old_count = self.counts.sender_count.fetch_add(1, Ordering::Relaxed);
        log::debug!(
            "Sender cloned: sender_count {} -> {}",
            old_count,
            old_count + 1
        );

        Self {
            inner: Arc::clone(&self.inner),
            counts: Arc::clone(&self.counts),
        }
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        // Decrement sender count atomically
        let old_count = self.counts.sender_count.fetch_sub(1, Ordering::AcqRel);
        log::debug!(
            "Sender dropped: sender_count {} -> {}",
            old_count,
            old_count - 1
        );

        // Wake up any waiting receivers since they should now get disconnected/EOF
        if old_count == 1 {
            // Last sender dropped
            if let Ok(mut inner) = self.inner.lock() {
                for waker in inner.receiver_wakers.drain(..) {
                    waker.wake();
                }
            }
        }
    }
}

/// Create an unbounded channel - compatible with `flume::unbounded`
#[must_use]
pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Mutex::new(ChannelInner::new(None)));
    let counts = Arc::new(SharedCounts {
        sender_count: AtomicUsize::new(1),
        receiver_count: AtomicUsize::new(1),
    });

    let sender = Sender {
        inner: Arc::clone(&inner),
        counts: Arc::clone(&counts),
    };

    let receiver = Receiver { inner, counts };

    log::debug!("Created unbounded channel");
    (sender, receiver)
}

/// Create an unbounded channel - alias for unbounded
#[must_use]
pub fn unbounded_channel<T>() -> (UnboundedSender<T>, UnboundedReceiver<T>) {
    unbounded()
}

/// Create a bounded channel - compatible with `flume::bounded`
#[must_use]
pub fn bounded<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Mutex::new(ChannelInner::new(Some(capacity))));
    let counts = Arc::new(SharedCounts {
        sender_count: AtomicUsize::new(1),
        receiver_count: AtomicUsize::new(1),
    });

    let sender = Sender {
        inner: Arc::clone(&inner),
        counts: Arc::clone(&counts),
    };

    let receiver = Receiver { inner, counts };

    log::debug!("Created bounded channel with capacity {capacity}");
    (sender, receiver)
}

/// Create a bounded channel - alias for bounded
#[must_use]
pub fn bounded_channel<T>(capacity: usize) -> (Sender<T>, Receiver<T>) {
    bounded(capacity)
}

/// Cooperative yielding with backoff strategy to prevent busy-waiting.
///
/// This function implements an escalating backoff strategy:
/// - First 10 iterations: just yield the thread
/// - 11-100 iterations: yield to simulator runtime and thread
/// - 101-1000 iterations: sleep briefly (nanoseconds)
/// - 1000+ iterations: sleep longer (microseconds) and log warnings
///
/// This prevents busy-waiting while allowing quick responses when data is available.
fn cooperative_yield_with_backoff(iteration: usize) {
    match iteration {
        0..=10 => {
            // First few iterations: just yield thread
            std::thread::yield_now();
        }
        11..=100 => {
            // Medium iterations: yield to simulator runtime
            if let Some(runtime) = crate::simulator::runtime::Runtime::current() {
                let processed = runtime.process_next_task();
                log::trace!("Cooperative yield: processed task = {processed}");
            }
            std::thread::yield_now();
        }
        101..=1000 => {
            // Many iterations: sleep briefly
            std::thread::sleep(Duration::from_nanos(1));
        }
        _ => {
            // Too many iterations: longer sleep and warning
            if iteration.is_multiple_of(1000) {
                log::warn!("Channel operation spinning excessively: {iteration} iterations");
            }
            std::thread::sleep(Duration::from_micros(1));
        }
    }
}

/// Re-export error types for compatibility
pub mod error {
    pub use flume::{RecvError, RecvTimeoutError, SendError, TryRecvError, TrySendError};
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_basic_send_recv() {
        let (tx, rx) = bounded::<i32>(10);

        tx.try_send(42).unwrap();
        assert_eq!(rx.try_recv().unwrap(), 42);
    }

    #[test]
    fn test_clone_behavior() {
        let (tx, rx) = bounded::<i32>(10);
        assert_eq!(tx.sender_count(), 1);
        assert_eq!(rx.receiver_count(), 1);

        let tx2 = tx.clone();
        assert_eq!(tx.sender_count(), 2);
        assert_eq!(tx2.sender_count(), 2);

        let rx2 = rx.clone();
        assert_eq!(rx.receiver_count(), 2);
        assert_eq!(rx2.receiver_count(), 2);

        drop(tx2);
        assert_eq!(tx.sender_count(), 1);

        drop(rx2);
        assert_eq!(rx.receiver_count(), 1);
    }

    #[test]
    fn test_disconnection_behavior() {
        let (tx, rx) = bounded::<i32>(10);

        // Should not be disconnected initially
        assert!(!tx.is_disconnected());

        // Drop receiver - sender should detect disconnection
        drop(rx);
        assert!(tx.is_disconnected());

        // Send should fail
        assert!(matches!(
            tx.try_send(42),
            Err(TrySendError::Disconnected(42))
        ));
    }

    #[test]
    fn test_tcp_stream_scenario() {
        // Simulate the TCP stream creation pattern
        struct MockTcpStream {
            tx: Sender<Vec<u8>>,
            rx: Receiver<Vec<u8>>,
        }

        // Recreate the exact TCP stream scenario that was failing
        let (tx1, rx1) = bounded::<Vec<u8>>(16);
        let (tx2, rx2) = bounded::<Vec<u8>>(16);

        let stream1 = MockTcpStream { tx: tx1, rx: rx2 };
        let stream2 = MockTcpStream { tx: tx2, rx: rx1 };

        // Both streams should remain connected
        assert!(!stream1.tx.is_disconnected());
        assert!(!stream2.tx.is_disconnected());

        // Should be able to send data
        stream1.tx.try_send(b"hello".to_vec()).unwrap();
        assert_eq!(stream2.rx.try_recv().unwrap(), b"hello".to_vec());

        stream2.tx.try_send(b"world".to_vec()).unwrap();
        assert_eq!(stream1.rx.try_recv().unwrap(), b"world".to_vec());
    }

    #[test]
    fn test_reference_counting_stress() {
        let (tx, rx) = bounded::<i32>(100);
        let rx = Arc::new(rx);

        // Spawn threads that clone and drop senders
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let tx = tx.clone();
                thread::spawn(move || {
                    for j in 0..10 {
                        let tx_clone = tx.clone();
                        tx_clone.try_send(i * 10 + j).ok();
                    }
                })
            })
            .collect();

        // Receiver thread
        let rx_clone = Arc::clone(&rx);
        let recv_handle = thread::spawn(move || {
            let mut count = 0;
            while count < 100 {
                if rx_clone.try_recv().is_ok() {
                    count += 1;
                }
                std::thread::yield_now();
            }
            count
        });

        for handle in handles {
            handle.join().unwrap();
        }

        drop(tx); // Drop original sender
        let received = recv_handle.join().unwrap();
        assert_eq!(received, 100);
    }

    #[test]
    fn test_capacity_and_full_behavior() {
        let (tx, _rx) = bounded::<i32>(2);

        assert_eq!(tx.capacity(), Some(2));
        assert!(tx.is_empty());
        assert!(!tx.is_full());

        tx.try_send(1).unwrap();
        assert_eq!(tx.len(), 1);
        assert!(!tx.is_empty());
        assert!(!tx.is_full());

        tx.try_send(2).unwrap();
        assert_eq!(tx.len(), 2);
        assert!(tx.is_full());

        // Should be full now
        assert!(matches!(tx.try_send(3), Err(TrySendError::Full(3))));
    }

    #[test]
    fn test_unbounded_channel() {
        let (tx, rx) = unbounded::<i32>();

        assert_eq!(tx.capacity(), None);
        assert!(tx.is_empty());
        assert!(!tx.is_full()); // Unbounded is never full

        // Should be able to send many items
        for i in 0..1000 {
            tx.try_send(i).unwrap();
        }

        assert_eq!(tx.len(), 1000);
        assert!(!tx.is_full());

        // Should be able to receive all items
        for i in 0..1000 {
            assert_eq!(rx.try_recv().unwrap(), i);
        }

        assert!(tx.is_empty());
    }
}
