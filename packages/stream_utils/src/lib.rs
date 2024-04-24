#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    sync::{Arc, RwLock},
    task::Poll,
};

use bytes::Bytes;
use stalled_monitor::StalledReadMonitor;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

pub mod stalled_monitor;

#[derive(Clone)]
pub struct ByteWriter {
    written: Arc<RwLock<u64>>,
    senders: Arc<RwLock<Vec<UnboundedSender<Bytes>>>>,
}

impl ByteWriter {
    pub fn stream(&self) -> ByteStream {
        ByteStream::from(self)
    }

    pub fn bytes_written(&self) -> u64 {
        *self.written.read().unwrap()
    }
}

impl Default for ByteWriter {
    fn default() -> Self {
        Self {
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
            *self.written.write().unwrap() += len as u64;

            if self.senders.read().unwrap().is_empty() {
                log::trace!("No senders associated with ByteWriter. Eating {len} bytes");
                return Ok(len);
            }
        }

        log::trace!("Sending bytes buf of size {len}");
        let bytes: Bytes = buf.to_vec().into();
        self.senders.write().unwrap().retain(|sender| {
            if sender.send(bytes.clone()).is_err() {
                log::debug!("Receiver has disconnected. Removing sender.");
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
    receiver: UnboundedReceiver<Bytes>,
}

impl ByteStream {
    pub fn stalled_monitor(self) -> StalledReadMonitor<Result<Bytes, std::io::Error>, ByteStream> {
        self.into()
    }
}

impl From<ByteStream> for StalledReadMonitor<Result<Bytes, std::io::Error>, ByteStream> {
    fn from(val: ByteStream) -> Self {
        StalledReadMonitor::new(val)
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
                log::trace!("Received bytes buf of size {}", response.len());
                Poll::Ready(Some(Ok(response)))
            }
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
        }
    }
}

impl From<&ByteWriter> for ByteStream {
    fn from(value: &ByteWriter) -> Self {
        let (sender, receiver) = unbounded_channel();
        value.senders.write().unwrap().push(sender);
        Self { receiver }
    }
}

#[derive(Clone)]
pub struct TypedWriter<T> {
    senders: Arc<RwLock<Vec<UnboundedSender<T>>>>,
}

impl<T> TypedWriter<T> {
    pub fn stream(&self) -> TypedStream<T> {
        TypedStream::from(self)
    }
}

impl<T: Clone> TypedWriter<T> {
    pub fn write(&self, buf: T) {
        let mut senders = self.senders.write().unwrap();
        let mut remove = vec![];
        let len = senders.len();
        for (i, sender) in senders.iter().enumerate() {
            if i == len - 1 {
                if sender.send(buf).is_err() {
                    log::debug!("Receiver has disconnected. Removing sender.");
                    remove.insert(0, i);
                }
                break;
            } else if sender.send(buf.clone()).is_err() {
                log::debug!("Receiver has disconnected. Removing sender.");
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
            senders: Arc::new(RwLock::new(vec![])),
        }
    }
}

pub struct TypedStream<T> {
    receiver: UnboundedReceiver<T>,
}

impl<T> TypedStream<T> {
    pub fn stalled_monitor(self) -> StalledReadMonitor<T, TypedStream<T>> {
        self.into()
    }
}

impl<T> From<TypedStream<T>> for StalledReadMonitor<T, TypedStream<T>> {
    fn from(val: TypedStream<T>) -> Self {
        StalledReadMonitor::new(val)
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

impl<T> From<&TypedWriter<T>> for TypedStream<T> {
    fn from(value: &TypedWriter<T>) -> Self {
        let (sender, receiver) = unbounded_channel();
        value.senders.write().unwrap().push(sender);
        Self { receiver }
    }
}
