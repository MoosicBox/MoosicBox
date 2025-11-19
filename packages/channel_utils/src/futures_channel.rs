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
