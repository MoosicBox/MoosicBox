use std::cmp::min;
use std::io::{Read, Seek};

use bytes::Bytes;
use flume::{Receiver, Sender, bounded};
use futures::{Stream, StreamExt};
use switchy_async::task::JoinHandle;
use switchy_async::util::CancellationToken;
use symphonia::core::io::MediaSource;

type ByteStreamType =
    Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + std::marker::Unpin>;

/// A media source that reads from a byte stream.
///
/// This type implements [`MediaSource`], [`Read`], and [`Seek`] to allow streaming audio data
/// from an asynchronous byte stream source.
pub struct ByteStreamSource {
    finished: bool,
    seekable: bool,
    size: Option<u64>,
    read_position: usize,
    fetcher: ByteStreamSourceFetcher,
    abort: CancellationToken,
}

/// Internal fetcher that manages reading from a byte stream in the background.
///
/// This struct handles the asynchronous fetching of data from a stream, buffering it,
/// and coordinating with the main [`ByteStreamSource`] through channels.
struct ByteStreamSourceFetcher {
    start: u64,
    end: Option<u64>,
    buffer: Vec<u8>,
    ready_receiver: Receiver<()>,
    ready: Sender<()>,
    receiver: Receiver<Bytes>,
    sender: Sender<Bytes>,
    abort_handle: Option<JoinHandle<()>>,
    abort: CancellationToken,
    stream_abort: CancellationToken,
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl ByteStreamSourceFetcher {
    /// Creates a new byte stream fetcher.
    ///
    /// # Parameters
    ///
    /// * `stream` - The byte stream to read from
    /// * `start` - The starting byte position
    /// * `end` - The ending byte position, if known
    /// * `autostart` - Whether to immediately start fetching data
    /// * `stream_abort` - Cancellation token to stop the stream
    pub fn new(
        stream: ByteStreamType,
        start: u64,
        end: Option<u64>,
        autostart: bool,
        stream_abort: CancellationToken,
    ) -> Self {
        let (tx, rx) = bounded(1);
        let (tx_ready, rx_ready) = bounded(1);

        let mut fetcher = Self {
            start,
            end,
            buffer: vec![],
            ready_receiver: rx_ready,
            ready: tx_ready,
            receiver: rx,
            sender: tx,
            abort_handle: None,
            abort: CancellationToken::new(),
            stream_abort,
        };

        if autostart {
            fetcher.start_fetch(stream);
        }

        fetcher
    }

    /// Starts fetching data from the stream in a background task.
    ///
    /// This method spawns an async task that reads from the stream and sends
    /// the received bytes through the internal channel.
    fn start_fetch(&mut self, mut stream: ByteStreamType) {
        let sender = self.sender.clone();
        let ready_receiver = self.ready_receiver.clone();
        let abort = self.abort.clone();
        let stream_abort = self.stream_abort.clone();
        let start = self.start;
        let end = self.end;
        log::debug!("Starting fetch for byte stream with range start={start} end={end:?}");

        self.abort_handle = Some(switchy_async::runtime::Handle::current().spawn_with_name(
            "audio_decoder: ByteStreamSource Fetcher",
            async move {
                log::debug!("Fetching byte stream with range start={start} end={end:?}");

                while let Some(item) = tokio::select! {
                    resp = stream.next() => resp,
                    () = abort.cancelled() => {
                        log::debug!("Aborted");
                        None
                    }
                    () = stream_abort.cancelled() => {
                        log::debug!("Stream aborted");
                        None
                    }
                } {
                    log::trace!("Received more bytes from stream");
                    let bytes = item.unwrap();
                    if let Err(err) = sender.send_async(bytes).await {
                        log::info!("Aborted byte stream read: {err:?}");
                        return;
                    }
                }

                log::debug!("Finished reading from stream");
                if sender.send_async(Bytes::new()).await.is_ok()
                    && ready_receiver.recv_async().await.is_err()
                {
                    log::info!("Byte stream read has been aborted");
                }
            },
        ));
    }

    /// Aborts the fetching task and resets the cancellation token.
    fn abort(&mut self) {
        self.abort.cancel();

        if let Some(handle) = &self.abort_handle {
            log::debug!("Aborting request");
            handle.abort();
            self.abort_handle = None;
        } else {
            log::debug!("No join handle for request");
        }
        self.abort = CancellationToken::new();
    }
}

impl Drop for ByteStreamSourceFetcher {
    fn drop(&mut self) {
        self.abort();
    }
}

impl ByteStreamSource {
    /// Creates a new byte stream source.
    ///
    /// # Parameters
    ///
    /// * `stream` - The byte stream to read from
    /// * `size` - The total size of the stream in bytes, if known
    /// * `autostart_fetch` - Whether to immediately start fetching data
    /// * `seekable` - Whether the stream supports seeking
    /// * `abort` - Cancellation token to stop the stream
    #[must_use]
    pub fn new(
        stream: ByteStreamType,
        size: Option<u64>,
        autostart_fetch: bool,
        seekable: bool,
        abort: CancellationToken,
    ) -> Self {
        Self {
            finished: false,
            seekable,
            size,
            read_position: 0,
            fetcher: ByteStreamSourceFetcher::new(stream, 0, size, autostart_fetch, abort.clone()),
            abort,
        }
    }
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl Read for ByteStreamSource {
    /// Reads bytes from the stream into the provided buffer.
    ///
    /// # Panics
    ///
    /// * Panics if the internal channel receiver is disconnected while waiting for data
    /// * Panics if the internal ready signal sender fails to send
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.finished {
            return Ok(0);
        }

        let mut written = 0;
        let mut read_position = self.read_position;
        let write_max = buf.len();

        while written < write_max {
            let receiver = self.fetcher.receiver.clone();
            let fetcher = &mut self.fetcher;
            let buffer_len = fetcher.buffer.len();
            #[allow(clippy::cast_possible_truncation)]
            let fetcher_start = fetcher.start as usize;

            log::debug!(
                "Read: read_pos[{read_position}] write_max[{write_max}] fetcher_start[{fetcher_start}] buffer_len[{buffer_len}] written[{written}]"
            );

            let bytes_written = if fetcher_start + buffer_len > read_position {
                let fetcher_buf_start = read_position - fetcher_start;
                let bytes_to_read_from_buf = buffer_len - fetcher_buf_start;
                log::trace!(
                    "Reading bytes from buffer: {bytes_to_read_from_buf} (max {write_max})"
                );
                let bytes_to_write = min(bytes_to_read_from_buf, write_max);
                buf[written..written + bytes_to_write].copy_from_slice(
                    &fetcher.buffer[fetcher_buf_start..fetcher_buf_start + bytes_to_write],
                );
                bytes_to_write
            } else {
                log::trace!("Waiting for bytes...");
                let new_bytes = receiver.recv().unwrap();
                if fetcher.abort.is_cancelled() || self.abort.is_cancelled() {
                    return Ok(written);
                }
                fetcher.buffer.extend_from_slice(&new_bytes);
                let len = new_bytes.len();
                log::trace!("Received bytes {len}");

                if len == 0 {
                    self.finished = true;
                    self.fetcher.ready.send(()).unwrap();
                    break;
                }

                let bytes_to_write = min(len, write_max - written);
                buf[written..written + bytes_to_write]
                    .copy_from_slice(&new_bytes[..bytes_to_write]);
                bytes_to_write
            };

            written += bytes_written;
            read_position += bytes_written;
        }

        self.read_position = read_position;

        Ok(written)
    }
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl Seek for ByteStreamSource {
    /// Seeks to a position in the stream.
    ///
    /// # Errors
    ///
    /// * Returns an I/O error if the seek position is invalid or cannot be converted to `usize`
    ///
    /// # Panics
    ///
    /// * Panics if seeking from end when the stream size is unknown
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let seek_position: usize = match pos {
            #[allow(clippy::cast_possible_truncation)]
            std::io::SeekFrom::Start(pos) => pos as usize,
            std::io::SeekFrom::Current(pos) => {
                #[allow(clippy::cast_possible_wrap)]
                let pos = self.read_position as i64 + pos;
                pos.try_into().map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Invalid seek: {pos}"),
                    )
                })?
            }
            std::io::SeekFrom::End(pos) => {
                #[allow(clippy::cast_possible_wrap)]
                let pos = self.size.unwrap() as i64 - pos;
                pos.try_into().map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Invalid seek: {pos}"),
                    )
                })?
            }
        };

        log::info!(
            "Seeking: pos[{seek_position}] current=[{}] type[{pos:?}]",
            self.read_position
        );

        self.read_position = seek_position;

        Ok(seek_position as u64)
    }
}

impl MediaSource for ByteStreamSource {
    /// Returns whether this media source is seekable.
    ///
    /// A byte stream is seekable only if both the `seekable` flag is set
    /// and the size is known.
    fn is_seekable(&self) -> bool {
        log::debug!("seekable={} size={:?}", self.seekable, self.size);
        self.seekable && self.size.is_some()
    }

    /// Returns the total byte length of the media source.
    ///
    /// Returns the size of the stream, if known.
    fn byte_len(&self) -> Option<u64> {
        log::debug!("byte_len={:?}", self.size);
        self.size
    }
}
