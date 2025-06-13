use std::{
    ops::Deref,
    pin::{Pin, pin},
    sync::{Arc, RwLock, atomic::AtomicBool},
    task::{Context, Poll},
};

use futures_channel::mpsc::{TrySendError, UnboundedReceiver, UnboundedSender};
use futures_core::{FusedStream, Stream};

use crate::MoosicBoxSender;

pub struct PrioritizedSender<T: Send> {
    inner: UnboundedSender<T>,
    #[allow(clippy::type_complexity)]
    priority: Option<Arc<Box<dyn (Fn(&T) -> usize) + Send + Sync>>>,
    buffer: Arc<RwLock<Vec<(usize, T)>>>,
    ready_to_send: Arc<AtomicBool>,
}

impl<T: Send> PrioritizedSender<T> {
    #[must_use]
    pub fn with_priority(mut self, func: impl (Fn(&T) -> usize) + Send + Sync + 'static) -> Self {
        self.priority.replace(Arc::new(Box::new(func)));
        self
    }

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
    fn send(&self, msg: T) -> Result<(), TrySendError<T>> {
        if !self
            .ready_to_send
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            if let Some(priority) = &self.priority {
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
        }

        self.unbounded_send(msg)?;

        Ok(())
    }
}

impl<T: Send> Clone for PrioritizedSender<T> {
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
    /// # Errors
    ///
    /// * If the send failed
    pub fn unbounded_send(&self, msg: T) -> Result<(), TrySendError<T>> {
        self.inner.unbounded_send(msg)
    }
}

impl<T: Send> Deref for PrioritizedSender<T> {
    type Target = UnboundedSender<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

pub struct PrioritizedReceiver<T: Send> {
    inner: UnboundedReceiver<T>,
    sender: PrioritizedSender<T>,
}

impl<T: Send> Deref for PrioritizedReceiver<T> {
    type Target = UnboundedReceiver<T>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T: Send> FusedStream for PrioritizedReceiver<T> {
    fn is_terminated(&self) -> bool {
        self.inner.is_terminated()
    }
}

impl<T: Send> Stream for PrioritizedReceiver<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let this = self.get_mut();
        let inner = &mut this.inner;
        let stream = pin!(inner);
        let poll = stream.poll_next(cx);

        if let std::task::Poll::Ready(Some(_)) = &poll {
            if let Err(e) = this.sender.flush() {
                moosicbox_assert::die_or_error!("Failed to flush sender: {e:?}");
            }
        }

        poll
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

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
