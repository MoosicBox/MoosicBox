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

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use std::io::Write;

    // ===== ByteWriter/ByteStream Tests =====

    #[test_log::test(switchy_async::test)]
    async fn test_byte_writer_multiple_streams() {
        // Test that multiple streams receive the same data
        let mut writer = ByteWriter::default();
        let mut stream1 = writer.stream();
        let mut stream2 = writer.stream();

        // Write data
        writer.write_all(b"hello").unwrap();
        writer.write_all(b" world").unwrap();
        writer.close();

        // Both streams should receive the same data
        let data1_chunk1 = stream1.next().await.unwrap().unwrap();
        let data1_chunk2 = stream1.next().await.unwrap().unwrap();
        let data1_end = stream1.next().await.unwrap().unwrap();

        let data2_chunk1 = stream2.next().await.unwrap().unwrap();
        let data2_chunk2 = stream2.next().await.unwrap().unwrap();
        let data2_end = stream2.next().await.unwrap().unwrap();

        assert_eq!(data1_chunk1, b"hello"[..]);
        assert_eq!(data1_chunk2, b" world"[..]);
        assert_eq!(data1_end.len(), 0); // Empty bytes from close()

        assert_eq!(data2_chunk1, b"hello"[..]);
        assert_eq!(data2_chunk2, b" world"[..]);
        assert_eq!(data2_end.len(), 0);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_byte_writer_close() {
        // Test that close sends empty bytes signal
        let mut writer = ByteWriter::default();
        let mut stream = writer.stream();

        writer.write_all(b"test").unwrap();
        writer.close();

        let data = stream.next().await.unwrap().unwrap();
        assert_eq!(data, b"test"[..]);

        let close_signal = stream.next().await.unwrap().unwrap();
        assert_eq!(close_signal.len(), 0, "close() should send empty bytes");
    }

    #[test_log::test]
    fn test_byte_writer_empty_write() {
        // Test that writing empty buffer returns 0
        let mut writer = ByteWriter::default();
        let result = writer.write(&[]).unwrap();
        assert_eq!(result, 0, "Writing empty buffer should return 0");
    }

    #[test_log::test]
    fn test_byte_writer_bytes_written() {
        // Test that bytes_written counter is accurate
        let mut writer = ByteWriter::default();
        assert_eq!(writer.bytes_written(), 0);

        writer.write_all(b"hello").unwrap();
        assert_eq!(writer.bytes_written(), 5);

        writer.write_all(b" world").unwrap();
        assert_eq!(writer.bytes_written(), 11);
    }

    #[test_log::test]
    fn test_byte_writer_flush() {
        // Test that flush is a no-op and doesn't error
        let mut writer = ByteWriter::default();
        writer.write_all(b"test").unwrap();
        assert!(writer.flush().is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_byte_stream_disconnection_cleanup() {
        // Test that disconnected receivers are removed from sender list
        let mut writer = ByteWriter::default();
        let stream1 = writer.stream();
        let stream2 = writer.stream();

        // Initially 2 senders
        assert_eq!(writer.senders.read().unwrap().len(), 2);

        // Drop stream1
        drop(stream1);

        // Write should trigger cleanup of disconnected receiver
        writer.write_all(b"test").unwrap();

        // Should have only 1 sender now
        assert_eq!(writer.senders.read().unwrap().len(), 1);

        // Drop stream2
        drop(stream2);

        // Write should cleanup the last sender
        writer.write_all(b"more").unwrap();
        assert_eq!(writer.senders.read().unwrap().len(), 0);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_byte_writer_no_streams() {
        // Test writing without any streams connected
        let mut writer = ByteWriter::default();

        // Should not panic or error when writing without streams
        let result = writer.write_all(b"data");
        assert!(result.is_ok());
        assert_eq!(writer.bytes_written(), 4);
    }

    #[test_log::test]
    fn test_byte_writer_id_uniqueness() {
        // Test that each writer gets a unique ID
        let writer1 = ByteWriter::default();
        let writer2 = ByteWriter::default();
        let writer3 = ByteWriter::default();

        assert_ne!(writer1.id, writer2.id);
        assert_ne!(writer2.id, writer3.id);
        assert_ne!(writer1.id, writer3.id);
    }

    // ===== TypedWriter/TypedStream Tests =====

    #[test_log::test(switchy_async::test)]
    async fn test_typed_writer_multiple_streams() {
        // Test that multiple typed streams receive the same data
        let writer = TypedWriter::<i32>::default();
        let mut stream1 = writer.stream();
        let mut stream2 = writer.stream();

        // Write values
        writer.write(42);
        writer.write(100);

        // Both streams should receive the same values
        let val1_1 = stream1.next().await.unwrap();
        let val1_2 = stream1.next().await.unwrap();

        let val2_1 = stream2.next().await.unwrap();
        let val2_2 = stream2.next().await.unwrap();

        assert_eq!(val1_1, 42);
        assert_eq!(val1_2, 100);
        assert_eq!(val2_1, 42);
        assert_eq!(val2_2, 100);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_typed_writer_disconnection_cleanup() {
        // Test that disconnected receivers are removed
        let writer = TypedWriter::<String>::default();
        let stream1 = writer.stream();
        let stream2 = writer.stream();

        // Initially 2 senders
        assert_eq!(writer.senders.read().unwrap().len(), 2);

        // Drop stream1
        drop(stream1);

        // Write should trigger cleanup
        writer.write("test".to_string());

        // Should have only 1 sender now
        assert_eq!(writer.senders.read().unwrap().len(), 1);

        // Drop stream2
        drop(stream2);

        // Write should cleanup the last sender
        writer.write("more".to_string());
        assert_eq!(writer.senders.read().unwrap().len(), 0);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_typed_writer_single_stream_no_clone() {
        // Test that with a single stream, value is moved (not cloned)
        // This is harder to verify directly, but we can test the behavior
        let writer = TypedWriter::<Vec<u8>>::default();
        let mut stream = writer.stream();

        writer.write(vec![1, 2, 3]);

        let received = stream.next().await.unwrap();
        assert_eq!(received, vec![1, 2, 3]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_typed_writer_different_types() {
        // Test TypedWriter with different types

        // String type
        let writer_string = TypedWriter::<String>::default();
        let mut stream_string = writer_string.stream();
        writer_string.write("hello".to_string());
        assert_eq!(stream_string.next().await.unwrap(), "hello");

        // Tuple type
        let writer_tuple = TypedWriter::<(i32, String)>::default();
        let mut stream_tuple = writer_tuple.stream();
        writer_tuple.write((42, "answer".to_string()));
        assert_eq!(
            stream_tuple.next().await.unwrap(),
            (42, "answer".to_string())
        );
    }

    #[test_log::test]
    fn test_typed_writer_id_uniqueness() {
        // Test that each typed writer gets a unique ID
        let writer1 = TypedWriter::<i32>::default();
        let writer2 = TypedWriter::<i32>::default();
        let writer3 = TypedWriter::<String>::default();

        assert_ne!(writer1.id, writer2.id);
        assert_ne!(writer2.id, writer3.id);
        assert_ne!(writer1.id, writer3.id);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_typed_writer_no_streams() {
        // Test writing without any streams connected
        let writer = TypedWriter::<i32>::default();

        // Should not panic when writing without streams
        writer.write(42);
        assert_eq!(writer.senders.read().unwrap().len(), 0);
    }

    #[cfg(feature = "stalled-monitor")]
    #[test_log::test]
    fn test_byte_stream_stalled_monitor_conversion() {
        // Test that ByteStream can be converted to StalledReadMonitor
        let writer = ByteWriter::default();
        let stream = writer.stream();

        let _monitor = stream.stalled_monitor();
        // If we get here without panic, the conversion works
    }

    #[cfg(feature = "stalled-monitor")]
    #[test_log::test]
    fn test_typed_stream_stalled_monitor_conversion() {
        // Test that TypedStream can be converted to StalledReadMonitor
        let writer = TypedWriter::<i32>::default();
        let stream = writer.stream();

        let _monitor = stream.stalled_monitor();
        // If we get here without panic, the conversion works
    }
}
