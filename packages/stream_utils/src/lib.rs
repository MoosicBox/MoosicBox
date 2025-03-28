#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    sync::{Arc, RwLock, atomic::AtomicUsize},
    task::Poll,
};

use bytes::Bytes;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

#[cfg(feature = "remote-bytestream")]
pub mod remote_bytestream;
#[cfg(feature = "stalled-monitor")]
pub mod stalled_monitor;

static CUR_ID: AtomicUsize = AtomicUsize::new(1);

pub fn new_byte_writer_id() -> usize {
    CUR_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

#[derive(Clone)]
pub struct ByteWriter {
    pub id: usize,
    written: Arc<RwLock<u64>>,
    senders: Arc<RwLock<Vec<UnboundedSender<Bytes>>>>,
}

impl ByteWriter {
    #[must_use]
    pub fn stream(&self) -> ByteStream {
        ByteStream::from(self)
    }

    /// # Panics
    ///
    /// * If the internal `RwLock` is poisoned
    #[must_use]
    pub fn bytes_written(&self) -> u64 {
        *self.written.read().unwrap()
    }

    /// # Panics
    ///
    /// * If the internal `RwLock` is poisoned
    pub fn close(&self) {
        self.senders.write().unwrap().retain(|sender| {
            if sender.send(Bytes::new()).is_err() {
                log::debug!(
                    "Receiver has disconnected from writer id={}. Removing sender.",
                    self.id
                );
                false
            } else {
                true
            }
        });
    }
}

impl Default for ByteWriter {
    fn default() -> Self {
        Self {
            id: new_byte_writer_id(),
            written: Arc::new(RwLock::new(0)),
            senders: Arc::new(RwLock::new(vec![])),
        }
    }
}

impl std::io::Write for ByteWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }

        let len = buf.len();

        {
            let written = {
                let mut written = self.written.write().unwrap();
                *written += len as u64;
                *written
            };
            log::trace!("ByteWriter written={written}");

            if self.senders.read().unwrap().is_empty() {
                log::trace!(
                    "No senders associated with ByteWriter writer id={}. Eating {len} bytes",
                    self.id
                );
            }
        }

        log::trace!("Sending bytes buf of size {len} writer id={}", self.id);
        let bytes: Bytes = buf.to_vec().into();
        self.senders.write().unwrap().retain(|sender| {
            if sender.send(bytes.clone()).is_err() {
                log::debug!(
                    "Receiver has disconnected from writer id={}. Removing sender.",
                    self.id
                );
                false
            } else {
                true
            }
        });
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub struct ByteStream {
    id: usize,
    receiver: UnboundedReceiver<Bytes>,
}

#[cfg(feature = "stalled-monitor")]
impl ByteStream {
    #[must_use]
    pub fn stalled_monitor(
        self,
    ) -> stalled_monitor::StalledReadMonitor<Result<Bytes, std::io::Error>, Self> {
        self.into()
    }
}

#[cfg(feature = "stalled-monitor")]
impl From<ByteStream>
    for stalled_monitor::StalledReadMonitor<Result<Bytes, std::io::Error>, ByteStream>
{
    fn from(val: ByteStream) -> Self {
        Self::new(val)
    }
}

impl futures::Stream for ByteStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let stream = self.get_mut();
        match stream.receiver.poll_recv(cx) {
            Poll::Ready(Some(response)) => {
                log::trace!(
                    "Received bytes buf of size {} from writer id={}",
                    response.len(),
                    stream.id
                );
                Poll::Ready(Some(Ok(response)))
            }
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
        }
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<&ByteWriter> for ByteStream {
    fn from(value: &ByteWriter) -> Self {
        let (sender, receiver) = unbounded_channel();
        value.senders.write().unwrap().push(sender);
        Self {
            id: value.id,
            receiver,
        }
    }
}

#[derive(Clone)]
pub struct TypedWriter<T> {
    id: usize,
    senders: Arc<RwLock<Vec<UnboundedSender<T>>>>,
}

impl<T> TypedWriter<T> {
    #[must_use]
    pub fn stream(&self) -> TypedStream<T> {
        TypedStream::from(self)
    }
}

impl<T: Clone> TypedWriter<T> {
    /// # Panics
    ///
    /// * If the internal `RwLock` is poisoned
    pub fn write(&self, buf: T) {
        let mut senders = self.senders.write().unwrap();
        let mut remove = vec![];
        let len = senders.len();
        for (i, sender) in senders.iter().enumerate() {
            if i == len - 1 {
                if sender.send(buf).is_err() {
                    log::debug!(
                        "Receiver has disconnected from writer id={}. Removing sender.",
                        self.id
                    );
                    remove.insert(0, i);
                }
                break;
            } else if sender.send(buf.clone()).is_err() {
                log::debug!(
                    "Receiver has disconnected from writer id={}. Removing sender.",
                    self.id
                );
                remove.insert(0, i);
            }
        }
        for i in remove {
            senders.remove(i);
        }
    }
}

impl<T> Default for TypedWriter<T> {
    fn default() -> Self {
        Self {
            id: new_byte_writer_id(),
            senders: Arc::new(RwLock::new(vec![])),
        }
    }
}

pub struct TypedStream<T> {
    receiver: UnboundedReceiver<T>,
}

#[cfg(feature = "stalled-monitor")]
impl<T> TypedStream<T> {
    #[must_use]
    pub fn stalled_monitor(self) -> stalled_monitor::StalledReadMonitor<T, Self> {
        self.into()
    }
}

#[cfg(feature = "stalled-monitor")]
impl<T> From<TypedStream<T>> for stalled_monitor::StalledReadMonitor<T, TypedStream<T>> {
    fn from(val: TypedStream<T>) -> Self {
        Self::new(val)
    }
}

impl<T> futures::Stream for TypedStream<T> {
    type Item = T;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let stream = self.get_mut();
        match stream.receiver.poll_recv(cx) {
            Poll::Ready(Some(response)) => {
                log::trace!("Received item");
                Poll::Ready(Some(response))
            }
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
        }
    }
}

#[allow(clippy::fallible_impl_from)]
impl<T> From<&TypedWriter<T>> for TypedStream<T> {
    fn from(value: &TypedWriter<T>) -> Self {
        let (sender, receiver) = unbounded_channel();
        value.senders.write().unwrap().push(sender);
        Self { receiver }
    }
}
