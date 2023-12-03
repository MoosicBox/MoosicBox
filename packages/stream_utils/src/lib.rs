#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{cell::RefCell, task::Poll};

use bytes::Bytes;
use futures::Stream;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};

#[derive(Clone)]
pub struct ByteWriter {
    senders: RefCell<Vec<UnboundedSender<Bytes>>>,
}

impl ByteWriter {
    pub fn stream(&self) -> ByteStream {
        ByteStream::from(self)
    }
}

impl Default for ByteWriter {
    fn default() -> Self {
        Self {
            senders: RefCell::new(vec![]),
        }
    }
}

impl std::io::Write for ByteWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        log::trace!("Sending bytes buf of size {}", buf.len());
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

impl Stream for ByteStream {
    type Item = Result<Bytes, Box<dyn std::error::Error>>;

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
