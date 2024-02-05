#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    cell::RefCell,
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
    senders: RefCell<Vec<UnboundedSender<Bytes>>>,
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
            senders: RefCell::new(vec![]),
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

            if self.senders.borrow().is_empty() {
                log::trace!("No senders associated with ByteWriter. Eating {len} bytes");
                return Ok(len);
            }
        }

        log::trace!("Sending bytes buf of size {len}");
        let bytes: Bytes = buf.to_vec().into();
        self.senders.borrow_mut().retain(|sender| {
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
    pub fn stalled_monitor(self) -> StalledReadMonitor<Bytes, ByteStream> {
        self.into()
    }
}

impl Into<StalledReadMonitor<Bytes, ByteStream>> for ByteStream {
    fn into(self) -> StalledReadMonitor<Bytes, ByteStream> {
        StalledReadMonitor::new(self)
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
        value.senders.borrow_mut().push(sender);
        Self { receiver }
    }
}
