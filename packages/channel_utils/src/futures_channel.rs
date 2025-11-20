//! Priority-based channel implementations using `futures-channel`.
//!
//! This module provides [`PrioritizedSender`] and [`PrioritizedReceiver`], which extend
//! the standard unbounded channel with support for message prioritization. Messages can
//! be sent with priority ordering, where higher priority values are processed before
//! lower priority values.
//!
//! # Example
//!
//! ```rust
//! use moosicbox_channel_utils::futures_channel::unbounded;
//! use moosicbox_channel_utils::MoosicBoxSender;
//! use futures_core::Stream;
//!
//! # async fn example() {
//! // Create a prioritized channel
//! let (tx, mut rx) = unbounded();
//!
//! // Configure priority function (higher values = higher priority)
//! let tx = tx.with_priority(|msg: &String| msg.len());
//!
//! // Send messages - longer strings will be received first
//! tx.send("hi".to_string()).unwrap();
//! tx.send("hello world".to_string()).unwrap();
//! tx.send("hey".to_string()).unwrap();
//!
//! // Receive messages in priority order
//! // (Note: actual ordering depends on when the receiver polls)
//! # }
//! ```

use std::{
    ops::Deref,
    pin::{Pin, pin},
    sync::{Arc, RwLock, atomic::AtomicBool},
    task::{Context, Poll},
};

use futures_channel::mpsc::{TrySendError, UnboundedReceiver, UnboundedSender};
use futures_core::{FusedStream, Stream};

use crate::MoosicBoxSender;

/// A sender that can prioritize messages based on a user-provided function.
///
/// Messages can be sent with priority ordering, where higher priority values
/// are sent before lower priority values. Messages are buffered internally
/// and flushed when the receiver polls for new items.
pub struct PrioritizedSender<T: Send> {
    inner: UnboundedSender<T>,
    #[allow(clippy::type_complexity)]
    priority: Option<Arc<Box<dyn (Fn(&T) -> usize) + Send + Sync>>>,
    buffer: Arc<RwLock<Vec<(usize, T)>>>,
    ready_to_send: Arc<AtomicBool>,
}

impl<T: Send> PrioritizedSender<T> {
    /// Sets the priority function for this sender.
    ///
    /// The function receives a reference to each message and returns a priority value.
    /// Higher priority values are sent before lower priority values.
    ///
    /// # Returns
    ///
    /// Returns `self` with the priority function configured, allowing for method chaining.
    #[must_use]
    pub fn with_priority(mut self, func: impl (Fn(&T) -> usize) + Send + Sync + 'static) -> Self {
        self.priority.replace(Arc::new(Box::new(func)));
        self
    }

    /// Flushes the highest priority message from the buffer to the underlying channel.
    ///
    /// Removes and sends the highest priority message from the internal buffer.
    /// If the buffer is empty, marks the sender as ready to send directly.
    ///
    /// # Errors
    ///
    /// * Returns an error if the underlying channel is disconnected and cannot accept the message
    ///
    /// # Panics
    ///
    /// * If the internal priority buffer lock is poisoned (when another thread panicked while
    ///   holding the lock)
    fn flush(&self) -> Result<(), TrySendError<T>> {
        let empty_buffer = { self.buffer.read().unwrap().is_empty() };
        if empty_buffer {
            log::trace!("flush: already empty");
            self.ready_to_send
                .store(true, std::sync::atomic::Ordering::SeqCst);
            return Ok(());
        }

        let mut buffer = self.buffer.write().unwrap();

        let (priority, item) = buffer.remove(0);
        let remaining_buffer_len = buffer.len();

        drop(buffer);

        log::debug!(
            "flush: sending buffered item with priority={priority} remaining_buf_len={remaining_buffer_len}",
        );

        self.unbounded_send(item)?;

        Ok(())
    }
}

impl<T: Send> MoosicBoxSender<T, TrySendError<T>> for PrioritizedSender<T> {
    /// Sends a message through the prioritized channel.
    ///
    /// If a priority function is configured and the receiver is not ready, the message
    /// is buffered and will be sent in priority order when the receiver polls for items.
    /// Otherwise, the message is sent immediately.
    ///
    /// # Errors
    ///
    /// * If the channel is disconnected and cannot accept messages
    ///
    /// # Panics
    ///
    /// * If the internal priority buffer lock is poisoned (when another thread panicked while
    ///   holding the lock)
    fn send(&self, msg: T) -> Result<(), TrySendError<T>> {
        if !self
            .ready_to_send
            .swap(false, std::sync::atomic::Ordering::SeqCst)
            && let Some(priority) = &self.priority
        {
            let priority = priority(&msg);

            let mut buffer = self.buffer.write().unwrap();

            let index = buffer
                .iter()
                .enumerate()
                .find_map(|(i, (p, _item))| if priority > *p { Some(i) } else { None });

            if let Some(index) = index {
                buffer.insert(index, (priority, msg));
            } else {
                buffer.push((priority, msg));
            }

            drop(buffer);

            return Ok(());
        }

        self.unbounded_send(msg)?;

        Ok(())
    }
}

impl<T: Send> Clone for PrioritizedSender<T> {
    /// Creates a new sender that shares the same underlying channel and priority buffer.
    ///
    /// All clones share the same priority function, buffer, and ready state, allowing
    /// multiple senders to coordinate message prioritization.
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            priority: self.priority.clone(),
            buffer: self.buffer.clone(),
            ready_to_send: self.ready_to_send.clone(),
        }
    }
}

impl<T: Send> PrioritizedSender<T> {
    /// Sends a message directly through the underlying unbounded channel.
    ///
    /// This bypasses the priority buffering mechanism and sends the message immediately.
    ///
    /// # Errors
    ///
    /// * If the channel is disconnected and cannot accept messages
    pub fn unbounded_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        self.inner.unbounded_send(msg)
    }
}

impl<T: Send> Deref for PrioritizedSender<T> {
    type Target = UnboundedSender<T>;

    /// Returns a reference to the underlying unbounded sender.
    ///
    /// This allows accessing methods on the underlying `UnboundedSender` directly.
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

/// A receiver that works with [`PrioritizedSender`] to receive prioritized messages.
///
/// This receiver automatically flushes the sender's priority buffer when polling
/// for new messages, ensuring that buffered messages are sent in priority order.
pub struct PrioritizedReceiver<T: Send> {
    inner: UnboundedReceiver<T>,
    sender: PrioritizedSender<T>,
}

impl<T: Send> Deref for PrioritizedReceiver<T> {
    type Target = UnboundedReceiver<T>;

    /// Returns a reference to the underlying unbounded receiver.
    ///
    /// This allows accessing methods on the underlying `UnboundedReceiver` directly.
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: Send> FusedStream for PrioritizedReceiver<T> {
    /// Returns `true` if the stream has terminated and will never yield more items.
    ///
    /// This delegates to the underlying receiver's termination state.
    fn is_terminated(&self) -> bool {
        self.inner.is_terminated()
    }
}

impl<T: Send> Stream for PrioritizedReceiver<T> {
    type Item = T;

    /// Polls for the next message from the channel.
    ///
    /// After successfully receiving a message, this automatically flushes the sender's
    /// priority buffer to ensure the next highest-priority message is ready.
    ///
    /// # Panics
    ///
    /// * May panic or log an error if flushing the sender's buffer fails after receiving
    ///   a message (behavior depends on `moosicbox_assert` configuration)
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let this = self.get_mut();
        let inner = &mut this.inner;
        let stream = pin!(inner);
        let poll = stream.poll_next(cx);

        if let std::task::Poll::Ready(Some(_)) = &poll
            && let Err(e) = this.sender.flush()
        {
            moosicbox_assert::die_or_error!("Failed to flush sender: {e:?}");
        }

        poll
    }

    /// Returns the bounds on the remaining length of the stream.
    ///
    /// This delegates to the underlying receiver's size hint.
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

/// Creates an unbounded prioritized channel.
///
/// Returns a sender and receiver pair that can be used to send and receive
/// messages with optional priority ordering. Use [`PrioritizedSender::with_priority`]
/// to configure priority ordering on the sender.
///
/// # Examples
///
/// ```rust
/// use moosicbox_channel_utils::futures_channel::unbounded;
/// use moosicbox_channel_utils::MoosicBoxSender;
///
/// # async fn example() {
/// let (tx, rx) = unbounded::<i32>();
///
/// // Without priority, messages are sent in FIFO order
/// tx.send(1).unwrap();
/// tx.send(2).unwrap();
///
/// // With priority, higher values are sent first
/// let tx = tx.with_priority(|msg: &i32| *msg as usize);
/// tx.send(10).unwrap();
/// tx.send(5).unwrap();
/// # }
/// ```
#[must_use]
pub fn unbounded<T: Send>() -> (PrioritizedSender<T>, PrioritizedReceiver<T>) {
    let (tx, rx) = futures_channel::mpsc::unbounded();
    let ready_to_send = Arc::new(AtomicBool::new(true));

    let tx = PrioritizedSender {
        inner: tx,
        priority: None,
        buffer: Arc::new(RwLock::new(vec![])),
        ready_to_send,
    };

    let rx = PrioritizedReceiver {
        inner: rx,
        sender: tx.clone(),
    };

    (tx, rx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_core::Stream;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test_log::test(switchy_async::test)]
    async fn test_unbounded_send_without_priority() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        let (tx, mut rx) = unbounded::<i32>();

        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();

        // Without priority, messages should arrive in FIFO order
        let mut results = Vec::new();
        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);

        loop {
            match Pin::new(&mut rx).poll_next(&mut context) {
                Poll::Ready(Some(msg)) => results.push(msg),
                Poll::Ready(None) | Poll::Pending => break,
            }
        }

        assert_eq!(results, vec![1, 2, 3]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_priority_ordering_basic() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        let (tx, mut rx) = unbounded::<i32>();

        // Configure priority - higher values have higher priority
        let tx = tx.with_priority(|msg: &i32| *msg as usize);

        // Send messages in non-priority order
        tx.send(1).unwrap();
        tx.send(5).unwrap();
        tx.send(3).unwrap();
        tx.send(10).unwrap();

        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);

        // First message goes directly (ready_to_send is true initially)
        // Subsequent messages get buffered and sorted by priority
        let first = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(first, Poll::Ready(Some(1)))); // First message sent directly

        // Trigger flush by receiving - highest priority from buffer
        let second = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(second, Poll::Ready(Some(10))));

        let third = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(third, Poll::Ready(Some(5))));

        let fourth = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(fourth, Poll::Ready(Some(3))));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_priority_with_equal_values() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        let (tx, mut rx) = unbounded::<i32>();

        let tx = tx.with_priority(|msg: &i32| *msg as usize);

        // Send messages with equal priorities
        tx.send(5).unwrap();
        tx.send(5).unwrap();
        tx.send(10).unwrap();
        tx.send(5).unwrap();

        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);

        let first = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(first, Poll::Ready(Some(5)))); // First sent directly

        // Next should be highest priority (10)
        let second = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(second, Poll::Ready(Some(10))));

        // Remaining should be in FIFO order for equal priorities
        let third = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(third, Poll::Ready(Some(5))));

        let fourth = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(fourth, Poll::Ready(Some(5))));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_sender_clone_shares_buffer() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        let (tx, mut rx) = unbounded::<i32>();

        let tx = tx.with_priority(|msg: &i32| *msg as usize);

        // Clone the sender
        let tx2 = tx.clone();

        // Send from both senders
        tx.send(3).unwrap();
        tx2.send(7).unwrap();
        tx.send(1).unwrap();

        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);

        // All messages should share the same priority buffer
        let first = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(first, Poll::Ready(Some(3)))); // First sent directly

        let second = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(second, Poll::Ready(Some(7)))); // Highest priority from shared buffer

        let third = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(third, Poll::Ready(Some(1))));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_channel_disconnection_on_receiver_drop() {
        let (tx, rx) = unbounded::<i32>();

        // Drop the receiver
        drop(rx);

        // Sending should fail with disconnected error
        let result = tx.send(42);
        assert!(result.is_err());
        assert!(result.unwrap_err().is_disconnected());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_unbounded_send_bypasses_priority() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        let (tx, mut rx) = unbounded::<i32>();

        let tx = tx.with_priority(|msg: &i32| *msg as usize);

        // Use unbounded_send to bypass priority buffering
        tx.unbounded_send(10).unwrap();
        tx.unbounded_send(5).unwrap();
        tx.unbounded_send(20).unwrap();

        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);

        // Messages should arrive in send order, not priority order
        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Ready(Some(10))));
        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Ready(Some(5))));
        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Ready(Some(20))));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_buffer_flush_after_poll() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        let (tx, mut rx) = unbounded::<i32>();

        let tx = tx.with_priority(|msg: &i32| *msg as usize);

        // Send multiple messages to build up buffer
        tx.send(1).unwrap(); // Goes directly
        tx.send(10).unwrap(); // Buffered
        tx.send(5).unwrap(); // Buffered
        tx.send(3).unwrap(); // Buffered

        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);

        // Each poll should flush one message from buffer
        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Ready(Some(1))));
        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Ready(Some(10))));
        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Ready(Some(5))));
        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Ready(Some(3))));
        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Pending));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_concurrent_senders() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        let (tx, mut rx) = unbounded::<usize>();

        let tx = tx.with_priority(|msg: &usize| *msg);

        // Simulate concurrent sends from multiple tasks
        let tx1 = tx.clone();
        let tx2 = tx.clone();
        let tx3 = tx;

        let handle1 = switchy_async::task::spawn(async move {
            for i in 0..10 {
                tx1.send(i).unwrap();
            }
        });

        let handle2 = switchy_async::task::spawn(async move {
            for i in 10..20 {
                tx2.send(i).unwrap();
            }
        });

        let handle3 = switchy_async::task::spawn(async move {
            for i in 20..30 {
                tx3.send(i).unwrap();
            }
        });

        // Wait for all senders to complete
        handle1.await.unwrap();
        handle2.await.unwrap();
        handle3.await.unwrap();

        // Collect all messages
        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);
        let mut results = Vec::new();

        loop {
            match Pin::new(&mut rx).poll_next(&mut context) {
                Poll::Ready(Some(msg)) => results.push(msg),
                Poll::Ready(None) | Poll::Pending => break,
            }
        }

        // Should have received all 30 messages
        assert_eq!(results.len(), 30);

        // All values 0-29 should be present (order may vary due to concurrency)
        let mut sorted_results = results.clone();
        sorted_results.sort_unstable();
        assert_eq!(sorted_results, (0..30).collect::<Vec<_>>());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_priority_function_with_complex_type() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        #[derive(Debug, Clone)]
        struct Message {
            priority: usize,
            data: String,
        }

        let (tx, mut rx) = unbounded::<Message>();

        let tx = tx.with_priority(|msg: &Message| msg.priority);

        // Send messages with different priorities
        tx.send(Message {
            priority: 1,
            data: "low".to_string(),
        })
        .unwrap();
        tx.send(Message {
            priority: 10,
            data: "high".to_string(),
        })
        .unwrap();
        tx.send(Message {
            priority: 5,
            data: "medium".to_string(),
        })
        .unwrap();

        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);

        let first = Pin::new(&mut rx).poll_next(&mut context);
        if let Poll::Ready(Some(msg)) = first {
            assert_eq!(msg.data, "low"); // First sent directly
        } else {
            panic!("Expected message");
        }

        let second = Pin::new(&mut rx).poll_next(&mut context);
        if let Poll::Ready(Some(msg)) = second {
            assert_eq!(msg.data, "high"); // Highest priority
        } else {
            panic!("Expected message");
        }

        let third = Pin::new(&mut rx).poll_next(&mut context);
        if let Poll::Ready(Some(msg)) = third {
            assert_eq!(msg.data, "medium");
        } else {
            panic!("Expected message");
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_ready_to_send_state_transitions() {
        use std::pin::Pin;
        use std::task::Context;

        let (tx, mut rx) = unbounded::<i32>();

        let tx = tx.with_priority(|msg: &i32| *msg as usize);

        // Initially ready_to_send is true
        tx.send(1).unwrap(); // Goes directly, sets ready_to_send to false
        tx.send(5).unwrap(); // Buffered
        tx.send(3).unwrap(); // Buffered

        // Receive first message using Stream::poll_next which triggers flush
        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);

        let first = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(first, std::task::Poll::Ready(Some(1))));

        // After flush, highest priority message (5) is sent
        let second = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(second, std::task::Poll::Ready(Some(5))));

        // Next flush sends the remaining message
        let third = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(third, std::task::Poll::Ready(Some(3))));

        // Buffer is now empty, ready_to_send should be true again
        // Next send should go directly
        tx.send(10).unwrap();
        let fourth = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(fourth, std::task::Poll::Ready(Some(10))));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_stream_poll_next_with_priority() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        let (tx, mut rx) = unbounded::<i32>();
        let tx = tx.with_priority(|msg: &i32| *msg as usize);

        // Send messages to buffer
        tx.send(1).unwrap();
        tx.send(10).unwrap();
        tx.send(5).unwrap();

        // Use Stream::poll_next to receive messages
        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);

        // Poll for first message
        let poll_result = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(poll_result, Poll::Ready(Some(1))));

        // Poll for highest priority from buffer
        let poll_result = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(poll_result, Poll::Ready(Some(10))));

        // Poll for remaining message
        let poll_result = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(poll_result, Poll::Ready(Some(5))));

        // No more messages
        let poll_result = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(poll_result, Poll::Pending));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_zero_priority_values() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        let (tx, mut rx) = unbounded::<i32>();

        let tx = tx.with_priority(|_msg: &i32| 0); // All messages have same priority (0)

        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();

        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);

        // With equal priorities, should maintain FIFO order
        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Ready(Some(1))));
        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Ready(Some(2))));
        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Ready(Some(3))));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_priority_function_called_correct_number_of_times() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        let (tx, mut rx) = unbounded::<i32>();

        let call_count = Arc::new(AtomicUsize::new(0));
        let call_count_clone = Arc::clone(&call_count);

        let tx = tx.with_priority(move |msg: &i32| {
            call_count_clone.fetch_add(1, Ordering::SeqCst);
            *msg as usize
        });

        // First send goes directly (ready_to_send is true), no priority function called
        tx.send(1).unwrap();
        assert_eq!(call_count.load(Ordering::SeqCst), 0);

        // Subsequent sends are buffered, priority function is called
        tx.send(5).unwrap();
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        tx.send(3).unwrap();
        assert_eq!(call_count.load(Ordering::SeqCst), 2);

        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);

        // Receive messages to verify they were buffered correctly
        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Ready(Some(1))));
        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Ready(Some(5))));
        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Ready(Some(3))));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_large_number_of_messages() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        let (tx, mut rx) = unbounded::<usize>();

        let tx = tx.with_priority(|msg: &usize| *msg);

        // Send many messages in reverse priority order
        for i in (0..1000).rev() {
            tx.send(i).unwrap();
        }

        let waker = futures_util::task::noop_waker();
        let mut context = Context::from_waker(&waker);

        // First message (999) goes directly
        let first = Pin::new(&mut rx).poll_next(&mut context);
        assert!(matches!(first, Poll::Ready(Some(999))));

        // Remaining messages should come out in descending priority order
        for i in (0..999).rev() {
            let received = Pin::new(&mut rx).poll_next(&mut context);
            assert!(matches!(received, Poll::Ready(Some(val)) if val == i), "Expected {i}, got {received:?}");
        }

        assert!(matches!(Pin::new(&mut rx).poll_next(&mut context), Poll::Pending));
    }
}
