//! Utilities for broadcasting and streaming data to multiple consumers.
//!
//! This crate provides primitives for writing data once and broadcasting it to multiple readers:
//!
//! * [`ByteWriter`] and [`ByteStream`] - Broadcast raw bytes to multiple streams implementing [`futures::Stream`]
//! * [`TypedWriter`] and [`TypedStream`] - Broadcast typed values to multiple streams
//! * [`remote_bytestream::RemoteByteStream`] - Seekable HTTP streaming with on-demand range requests (requires `remote-bytestream` feature)
//! * [`stalled_monitor::StalledReadMonitor`] - Timeout and throttling for streams (requires `stalled-monitor` feature)
//!
//! # Examples
//!
//! Broadcasting bytes to multiple readers:
//!
//! ```rust
//! use moosicbox_stream_utils::ByteWriter;
//! use std::io::Write;
//!
//! # fn main() -> std::io::Result<()> {
//! let mut writer = ByteWriter::default();
//! let stream1 = writer.stream();
//! let stream2 = writer.stream();
//!
//! writer.write_all(b"hello world")?;
//! // Both stream1 and stream2 will receive the same data
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    sync::{Arc, RwLock, atomic::AtomicUsize},
    task::Poll,
};

use bytes::Bytes;
use switchy_async::sync::mpsc::{Receiver, Sender, unbounded};

#[cfg(feature = "remote-bytestream")]
pub mod remote_bytestream;
#[cfg(feature = "stalled-monitor")]
pub mod stalled_monitor;

static CUR_ID: AtomicUsize = AtomicUsize::new(1);

/// Generates a unique ID for byte writers.
///
/// Returns a monotonically increasing identifier that can be used to track
/// and distinguish different byte writer instances.
#[must_use]
pub fn new_byte_writer_id() -> usize {
    CUR_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
}

/// A writer that broadcasts bytes to multiple stream readers.
///
/// Implements the [`std::io::Write`] trait and allows multiple [`ByteStream`] instances
/// to receive the same data being written. Each stream receives its own copy of the data.
#[derive(Clone)]
pub struct ByteWriter {
    /// Unique identifier for this writer instance.
    pub id: usize,
    written: Arc<RwLock<u64>>,
    senders: Arc<RwLock<Vec<Sender<Bytes>>>>,
}

impl ByteWriter {
    /// Creates a new stream that will receive bytes written to this writer.
    ///
    /// Multiple streams can be created from the same writer, and each will receive
    /// a copy of all data written.
    #[must_use]
    pub fn stream(&self) -> ByteStream {
        ByteStream::from(self)
    }

    /// Returns the total number of bytes written so far.
    ///
    /// # Panics
    ///
    /// * If the internal `RwLock` is poisoned
    #[must_use]
    pub fn bytes_written(&self) -> u64 {
        *self.written.read().unwrap()
    }

    /// Closes the writer by sending an empty bytes signal to all connected streams.
    ///
    /// This notifies all streams that no more data will be written. Disconnected
    /// receivers are removed from the internal list.
    ///
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
    /// Writes bytes to the writer and broadcasts them to all connected streams.
    ///
    /// Empty buffers are ignored. Disconnected receivers are automatically removed.
    ///
    /// # Errors
    ///
    /// * This implementation never returns errors (always returns `Ok`)
    ///
    /// # Panics
    ///
    /// * If the internal `RwLock` is poisoned
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

    /// Flushes the writer.
    ///
    /// This is a no-op for `ByteWriter` as data is immediately sent to streams.
    ///
    /// # Errors
    ///
    /// * This implementation never returns errors (always returns `Ok`)
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A stream that receives bytes from a [`ByteWriter`].
///
/// Implements the [`futures::Stream`] trait, yielding `Result<Bytes, std::io::Error>` items.
pub struct ByteStream {
    id: usize,
    receiver: Receiver<Bytes>,
}

#[cfg(feature = "stalled-monitor")]
impl ByteStream {
    /// Wraps this stream in a stalled read monitor for timeout detection.
    ///
    /// The returned monitor can detect when the stream stalls (no data received)
    /// and enforce timeout or throttling policies.
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
    /// Creates a new stream from a byte writer reference.
    ///
    /// # Panics
    ///
    /// * If the internal `RwLock` is poisoned
    fn from(value: &ByteWriter) -> Self {
        let (sender, receiver) = unbounded();
        value.senders.write().unwrap().push(sender);
        Self {
            id: value.id,
            receiver,
        }
    }
}

/// A writer that broadcasts typed values to multiple stream readers.
///
/// Similar to [`ByteWriter`] but works with any cloneable type `T` instead of just bytes.
/// Each connected [`TypedStream`] receives its own copy of the data.
#[derive(Clone)]
pub struct TypedWriter<T> {
    id: usize,
    senders: Arc<RwLock<Vec<Sender<T>>>>,
}

impl<T> TypedWriter<T> {
    /// Creates a new stream that will receive values written to this writer.
    ///
    /// Multiple streams can be created from the same writer, and each will receive
    /// a copy of all data written.
    #[must_use]
    pub fn stream(&self) -> TypedStream<T> {
        TypedStream::from(self)
    }
}

impl<T: Clone> TypedWriter<T> {
    /// Writes a value to the writer and broadcasts it to all connected streams.
    ///
    /// The value is cloned for each connected stream except the last one, which
    /// receives the original value. Disconnected receivers are automatically removed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use moosicbox_stream_utils::TypedWriter;
    /// use futures::StreamExt;
    ///
    /// # async fn example() {
    /// let writer = TypedWriter::<String>::default();
    /// let mut stream1 = writer.stream();
    /// let mut stream2 = writer.stream();
    ///
    /// writer.write("hello".to_string());
    ///
    /// // Both streams receive the same value
    /// assert_eq!(stream1.next().await, Some("hello".to_string()));
    /// assert_eq!(stream2.next().await, Some("hello".to_string()));
    /// # }
    /// ```
    ///
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

/// A stream that receives typed values from a [`TypedWriter`].
///
/// Implements the [`futures::Stream`] trait, yielding items of type `T`.
pub struct TypedStream<T> {
    receiver: Receiver<T>,
}

#[cfg(feature = "stalled-monitor")]
impl<T> TypedStream<T> {
    /// Wraps this stream in a stalled read monitor for timeout detection.
    ///
    /// The returned monitor can detect when the stream stalls (no data received)
    /// and enforce timeout or throttling policies.
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
    /// Creates a new typed stream from a typed writer reference.
    ///
    /// # Panics
    ///
    /// * If the internal `RwLock` is poisoned
    fn from(value: &TypedWriter<T>) -> Self {
        let (sender, receiver) = unbounded();
        value.senders.write().unwrap().push(sender);
        Self { receiver }
    }
}
