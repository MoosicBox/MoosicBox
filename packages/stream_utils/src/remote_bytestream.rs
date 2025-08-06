use std::cmp::min;
use std::io::{Read, Seek};

use bytes::Bytes;
use flume::{Receiver, Sender, bounded, unbounded};
use futures::StreamExt;
use switchy_async::task::JoinHandle;
use switchy_async::util::CancellationToken;
use switchy_http::Client;

// Trait for HTTP fetching to enable dependency injection in tests
#[async_trait::async_trait]
pub trait HttpFetcher: Send + Sync + Clone + 'static {
    async fn fetch_range(
        &self,
        url: &str,
        start: u64,
        end: Option<u64>,
    ) -> Result<
        Box<
            dyn futures::Stream<Item = Result<Bytes, Box<dyn std::error::Error + Send + Sync>>>
                + Send
                + Unpin,
        >,
        Box<dyn std::error::Error + Send + Sync>,
    >;
}

// Default implementation using switchy_http::Client
#[derive(Clone)]
pub struct DefaultHttpFetcher;

#[async_trait::async_trait]
impl HttpFetcher for DefaultHttpFetcher {
    async fn fetch_range(
        &self,
        url: &str,
        start: u64,
        end: Option<u64>,
    ) -> Result<
        Box<
            dyn futures::Stream<Item = Result<Bytes, Box<dyn std::error::Error + Send + Sync>>>
                + Send
                + Unpin,
        >,
        Box<dyn std::error::Error + Send + Sync>,
    > {
        let bytes_range = format!(
            "bytes={}-{}",
            start,
            end.map_or_else(String::new, |n| n.to_string())
        );

        log::debug!("Fetching byte stream with range {bytes_range}");

        let mut response = Client::new()
            .get(url)
            .header("Range", &bytes_range)
            .send()
            .await
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)?;

        match response.status() {
            switchy_http::models::StatusCode::Ok
            | switchy_http::models::StatusCode::PartialContent => {
                // Log the actual Content-Length from server
                if let Some(content_length) = response.headers().get("content-length") {
                    log::debug!("Server reports Content-Length: {content_length:?}");
                } else {
                    log::debug!("No Content-Length header in response");
                }
            }
            _ => {
                let error_msg = format!("Received error response ({})", response.status());
                log::error!("{error_msg}");
                return Err(error_msg.into());
            }
        }

        let stream = response.bytes_stream();
        Ok(Box::new(Box::pin(stream.map(|item| {
            item.map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send + Sync>)
        }))))
    }
}

pub struct RemoteByteStream<F: HttpFetcher = DefaultHttpFetcher> {
    url: String,
    pub finished: bool,
    pub seekable: bool,
    pub size: Option<u64>,
    pub read_position: u64,
    fetcher: RemoteByteStreamFetcher<F>,
    abort: CancellationToken,
}

struct RemoteByteStreamFetcher<F: HttpFetcher> {
    url: String,
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
    http_fetcher: F,
}

impl<F: HttpFetcher> RemoteByteStreamFetcher<F> {
    pub fn new(
        url: String,
        start: u64,
        end: Option<u64>,
        autostart: bool,
        stream_abort: CancellationToken,
        http_fetcher: F,
    ) -> Self {
        let (tx, rx) = unbounded();
        let (tx_ready, rx_ready) = bounded(1);

        let mut fetcher = Self {
            url,
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
            http_fetcher,
        };

        if autostart {
            fetcher.start_fetch();
        }

        fetcher
    }

    fn start_fetch(&mut self) {
        let url = self.url.clone();
        let sender = self.sender.clone();
        let ready_receiver = self.ready_receiver.clone();
        let abort = self.abort.clone();
        let stream_abort = self.stream_abort.clone();
        let start = self.start;
        let end = self.end;
        let http_fetcher = self.http_fetcher.clone();
        let bytes_range = format!(
            "bytes={}-{}",
            start,
            end.map_or_else(String::new, |n| n.to_string())
        );
        let size_info = end.map_or_else(|| "unknown size".to_string(), |s| format!("{s} bytes"));
        log::debug!("Starting fetch for byte stream with range {bytes_range} ({size_info})");

        self.abort_handle = Some(switchy_async::runtime::Handle::current().spawn_with_name(
            "stream_utils: RemoteByteStream Fetcher",
            async move {
                let mut stream = match http_fetcher.fetch_range(&url, start, end).await {
                    Ok(stream) => stream,
                    Err(err) => {
                        log::error!("Failed to get stream response: {err:?}");
                        if let Err(err) = sender.send_async(Bytes::new()).await {
                            log::warn!("Failed to send empty bytes: {err:?}");
                        }
                        return;
                    }
                };

                while let Some(item) = switchy_async::select! {
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
                    let bytes = match item {
                        Ok(bytes) => bytes,
                        Err(err) => {
                            log::info!("Aborted byte stream read (no bytes received): {err:?}");
                            return;
                        }
                    };
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

impl<F: HttpFetcher> Drop for RemoteByteStreamFetcher<F> {
    fn drop(&mut self) {
        log::trace!("Dropping RemoteByteStreamFetcher");
        self.abort();
    }
}

impl<F: HttpFetcher> RemoteByteStream<F> {
    #[must_use]
    pub fn new_with_fetcher(
        url: String,
        size: Option<u64>,
        autostart_fetch: bool,
        seekable: bool,
        abort: CancellationToken,
        http_fetcher: F,
    ) -> Self {
        Self {
            url: url.clone(),
            finished: false,
            seekable,
            size,
            read_position: 0,
            fetcher: RemoteByteStreamFetcher::new(
                url,
                0,
                None,
                autostart_fetch,
                abort.clone(),
                http_fetcher,
            ),
            abort,
        }
    }
}

impl RemoteByteStream<DefaultHttpFetcher> {
    #[must_use]
    pub fn new(
        url: String,
        size: Option<u64>,
        autostart_fetch: bool,
        seekable: bool,
        abort: CancellationToken,
    ) -> Self {
        Self::new_with_fetcher(
            url,
            size,
            autostart_fetch,
            seekable,
            abort,
            DefaultHttpFetcher,
        )
    }
}

impl<F: HttpFetcher> Read for RemoteByteStream<F> {
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // Check if stream has been finished for a grace period
        if self.finished {
            let read_position = usize::try_from(self.read_position).unwrap();
            let fetcher_start = usize::try_from(self.fetcher.start).unwrap();
            let buffer_len = self.fetcher.buffer.len();

            let remaining_in_buffer = if fetcher_start + buffer_len > read_position {
                let fetcher_buf_start = read_position - fetcher_start;
                buffer_len - fetcher_buf_start
            } else {
                0
            };

            if remaining_in_buffer == 0 {
                log::debug!(
                    "Read attempted on finished stream with no remaining buffer data - returning 0 bytes (read_position: {}, stream_size: {:?})",
                    self.read_position,
                    self.size
                );
                return Ok(0);
            }

            log::debug!(
                "Read attempted on finished stream but {remaining_in_buffer} bytes remain in buffer - continuing read"
            );
        }

        let mut written = 0;
        let mut read_position = usize::try_from(self.read_position).unwrap();
        let write_max = buf.len();
        let mut http_stream_ended = false;

        while written < write_max {
            let receiver = self.fetcher.receiver.clone();
            let fetcher = &mut self.fetcher;
            let buffer_len = fetcher.buffer.len();
            let fetcher_start = usize::try_from(fetcher.start).unwrap();

            log::debug!(
                "Read: read_pos[{read_position}] write_max[{write_max}] fetcher_start[{fetcher_start}] buffer_len[{buffer_len}] written[{written}]"
            );

            let bytes_written = if fetcher_start + buffer_len > read_position {
                let fetcher_buf_start = read_position - fetcher_start;
                let bytes_to_read_from_buf = buffer_len - fetcher_buf_start;
                log::trace!(
                    "Reading bytes from buffer: {} (max {})",
                    bytes_to_read_from_buf,
                    write_max - written
                );
                let bytes_to_write = min(bytes_to_read_from_buf, write_max - written);
                buf[written..written + bytes_to_write].copy_from_slice(
                    &fetcher.buffer[fetcher_buf_start..fetcher_buf_start + bytes_to_write],
                );
                bytes_to_write
            } else {
                // No more data in buffer - if stream is finished, we're done
                if self.finished {
                    log::debug!(
                        "No more data in buffer and stream is finished - ending read with {written} bytes"
                    );
                    break;
                }

                log::trace!("Waiting for bytes...");
                let new_bytes = receiver.recv().unwrap();
                if fetcher.abort.is_cancelled() {
                    log::debug!("Fetcher aborted during read - returning {written} bytes");
                    return Ok(written);
                }
                let len = new_bytes.len();
                log::trace!("Received bytes {len}");

                if len == 0 {
                    // HTTP stream ended - check if we have all expected bytes from fetcher start to file end
                    http_stream_ended = true;
                    let total_buffer_bytes = fetcher.buffer.len() as u64;
                    let fetcher_start_u64 = fetcher.start;
                    let fetcher_end_position = fetcher_start_u64 + total_buffer_bytes;

                    if let Some(expected_size) = self.size {
                        // When seeking, we only need data from fetcher start to file end
                        // The fetcher should contain all data from its start position to EOF
                        if fetcher_end_position < expected_size {
                            log::warn!(
                                "Stream ended prematurely: fetcher starts at {}, has {} bytes, reaches position {}, but file size is {} bytes (missing {} bytes)",
                                fetcher_start_u64,
                                total_buffer_bytes,
                                fetcher_end_position,
                                expected_size,
                                expected_size - fetcher_end_position
                            );

                            return Err(std::io::Error::new(
                                std::io::ErrorKind::UnexpectedEof,
                                format!(
                                    "Stream ended prematurely: got {total_buffer_bytes} bytes from position {fetcher_start_u64}, expected {expected_size} bytes total (reaches {fetcher_end_position}/{expected_size})"
                                ),
                            ));
                        }

                        log::debug!(
                            "HTTP stream completed successfully: fetcher received {total_buffer_bytes} bytes from position {fetcher_start_u64}, reaches file end at {fetcher_end_position} (file size {expected_size})"
                        );
                    }

                    // HTTP stream has ended - break out of waiting loop
                    // We'll check if stream should be finished after reading all available data
                    break;
                }

                fetcher.buffer.extend_from_slice(&new_bytes);
                // Continue the loop to read from the buffer
                continue;
            };

            written += bytes_written;
            read_position += bytes_written;
        }

        self.read_position = read_position as u64;

        // Check if stream should be marked as finished now that we've read all available data
        if !self.finished {
            // Only mark as finished if HTTP stream ended and no more data available
            let fetcher_start = usize::try_from(self.fetcher.start).unwrap();
            let buffer_len = self.fetcher.buffer.len();
            let current_read_position = usize::try_from(self.read_position).unwrap();

            let remaining_in_buffer = if fetcher_start + buffer_len > current_read_position {
                let fetcher_buf_start = current_read_position - fetcher_start;
                buffer_len - fetcher_buf_start
            } else {
                0
            };

            // Use the flag we set when we received 0 bytes from the HTTP stream

            if http_stream_ended && remaining_in_buffer == 0 {
                log::debug!(
                    "HTTP stream finished and all buffer data consumed - marking stream as finished"
                );
                self.finished = true;
                self.fetcher.ready.send(()).unwrap();
            } else if remaining_in_buffer > 0 {
                log::debug!(
                    "HTTP stream finished but {remaining_in_buffer} bytes remain unread in buffer - NOT marking as finished yet"
                );
            }
        }

        log::debug!(
            "Read completed: returned {} bytes, new read_position: {}, finished: {}",
            written,
            self.read_position,
            self.finished
        );

        Ok(written)
    }
}

impl<F: HttpFetcher> Seek for RemoteByteStream<F> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let seek_position = match pos {
            std::io::SeekFrom::Start(pos) => pos,
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

        // Check if we're seeking within already received data
        let fetcher_start = self.fetcher.start;
        let fetcher_end = fetcher_start + self.fetcher.buffer.len() as u64;

        if seek_position >= fetcher_start && seek_position < fetcher_end {
            // Seeking within already received data - just update read position
            log::debug!(
                "Seeking within already downloaded data - preserving fetcher (start={fetcher_start}, end={fetcher_end})"
            );
            self.read_position = seek_position;
            self.finished = false;
        } else {
            // Seeking outside already received data - need new fetcher
            if seek_position > self.read_position {
                log::debug!(
                    "Seeking forward outside downloaded data - creating new fetcher (current={}, target={})",
                    self.read_position,
                    seek_position
                );
            } else {
                log::debug!(
                    "Seeking backward - creating new fetcher (current={}, target={})",
                    self.read_position,
                    seek_position
                );
            }

            self.read_position = seek_position;
            self.finished = false;
            self.fetcher.abort();

            // Create a new fetcher to handle the seek
            if seek_position < self.size.unwrap_or(u64::MAX) {
                self.fetcher = RemoteByteStreamFetcher::new(
                    self.url.clone(),
                    seek_position,
                    None,
                    true,
                    self.abort.clone(),
                    self.fetcher.http_fetcher.clone(),
                );
            } else {
                self.fetcher.abort();
            }
        }

        Ok(seek_position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom};
    use switchy_async::util::CancellationToken;

    #[test]
    fn test_remote_bytestream_construction() {
        // Test that RemoteByteStream can be constructed with proper parameters
        let abort_token = CancellationToken::new();
        let stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            Some(1000),
            false, // Don't auto-start fetch for this test
            true,  // Seekable
            abort_token,
        );

        assert_eq!(stream.url, "https://example.com/file.mp3");
        assert_eq!(stream.size, Some(1000));
        assert_eq!(stream.read_position, 0);
        assert!(!stream.finished);
        assert!(stream.seekable);
    }

    #[tokio::test]
    async fn test_seek_functionality() {
        // Test seeking functionality
        let abort_token = CancellationToken::new();
        let mut stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            Some(1000),
            false, // Don't auto-start fetch
            true,  // Seekable
            abort_token,
        );

        // Test seeking to start
        let pos = stream.seek(SeekFrom::Start(100)).unwrap();
        assert_eq!(pos, 100);
        assert_eq!(stream.read_position, 100);

        // Test seeking from current position
        let pos = stream.seek(SeekFrom::Current(50)).unwrap();
        assert_eq!(pos, 150);
        assert_eq!(stream.read_position, 150);

        // Test seeking from end
        let pos = stream.seek(SeekFrom::End(100)).unwrap();
        assert_eq!(pos, 900); // 1000 - 100
        assert_eq!(stream.read_position, 900);
    }

    #[test]
    fn test_seek_past_end_aborts_fetcher() {
        // Test that seeking past end of file aborts the fetcher
        let abort_token = CancellationToken::new();
        let mut stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            Some(1000),
            false, // Don't auto-start fetch
            true,  // Seekable
            abort_token,
        );

        // Seek past end of file
        let pos = stream.seek(SeekFrom::Start(1500)).unwrap();
        assert_eq!(pos, 1500);
        assert_eq!(stream.read_position, 1500);

        // The fetcher should be aborted (we can't easily test this without mocking)
        // But we can verify the seek position was set correctly
    }

    #[tokio::test]
    async fn test_seek_error_handling() {
        // Test seek error handling for invalid positions
        let abort_token = CancellationToken::new();
        let mut stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            Some(1000),
            false, // Don't auto-start fetch
            true,  // Seekable
            abort_token,
        );

        // Test seeking with negative current position (should fail)
        let result = stream.seek(SeekFrom::Current(-2000));
        assert!(result.is_err(), "Seeking to negative position should fail");

        // Test seeking from end with positive offset that would result in negative position
        let result = stream.seek(SeekFrom::End(2000));
        assert!(
            result.is_err(),
            "Seeking with end offset larger than file size should fail"
        );
    }

    #[test]
    fn test_finished_stream_read_behavior() {
        // Test that finished streams return 0 bytes on read
        let abort_token = CancellationToken::new();
        let mut stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            Some(1000),
            false, // Don't auto-start fetch
            true,  // Seekable
            abort_token,
        );

        // Manually mark as finished
        stream.finished = true;

        // Reading from finished stream should return 0
        let mut buf = [0u8; 100];
        let result = stream.read(&mut buf).unwrap();
        assert_eq!(result, 0, "Finished stream should return 0 bytes");
    }

    #[tokio::test]
    async fn test_range_request_construction() {
        // Test that range requests are constructed correctly
        let abort_token = CancellationToken::new();

        // Test full file download (should use None as end)
        let stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            Some(1000),
            false, // Don't auto-start fetch
            true,  // Seekable
            abort_token.clone(),
        );

        // The fetcher should be created with start=0, end=None
        assert_eq!(stream.fetcher.start, 0);
        assert_eq!(stream.fetcher.end, None);

        // Test seeking creates new fetcher with correct start
        let mut stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            Some(1000),
            false, // Don't auto-start fetch
            true,  // Seekable
            abort_token,
        );

        stream.seek(SeekFrom::Start(500)).unwrap();
        assert_eq!(stream.fetcher.start, 500);
        assert_eq!(stream.fetcher.end, None);
    }

    #[test]
    fn test_abort_token_propagation() {
        // Test that abort tokens are properly propagated
        let abort_token = CancellationToken::new();
        let stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            Some(1000),
            false, // Don't auto-start fetch
            true,  // Seekable
            abort_token.clone(),
        );

        // The stream should hold a reference to the same abort token
        assert!(!stream.abort.is_cancelled());

        // Cancelling the original token should affect the stream's token
        abort_token.cancel();
        assert!(stream.abort.is_cancelled());
    }

    #[tokio::test]
    async fn test_fetcher_abort_and_recreation() {
        // Test that fetchers are properly aborted and recreated on seek
        let abort_token = CancellationToken::new();
        let mut stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            Some(1000),
            false, // Don't auto-start fetch
            true,  // Seekable
            abort_token,
        );

        let original_start = stream.fetcher.start;
        assert_eq!(original_start, 0);

        // Seeking should create a new fetcher with different start position
        stream.seek(SeekFrom::Start(200)).unwrap();
        assert_eq!(stream.fetcher.start, 200);
        assert_ne!(stream.fetcher.start, original_start);
    }

    #[test]
    fn test_size_none_handling() {
        // Test streams with no known size
        let abort_token = CancellationToken::new();
        let stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            None,  // No known size
            false, // Don't auto-start fetch
            true,  // Seekable
            abort_token,
        );

        assert_eq!(stream.size, None);

        // Can't easily test seeking from end when size is unknown because it panics
        // This is a known limitation of the current implementation
    }

    #[test]
    #[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
    fn test_seek_from_end_panics_when_size_unknown() {
        // Test that seeking from end panics when size is unknown
        let abort_token = CancellationToken::new();
        let mut stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            None,  // No known size
            false, // Don't auto-start fetch
            true,  // Seekable
            abort_token,
        );

        // This should panic because size is None
        stream.seek(SeekFrom::End(100)).unwrap();
    }

    #[test]
    fn test_buffer_initialization() {
        // Test that the fetcher buffer is properly initialized
        let abort_token = CancellationToken::new();
        let stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            Some(1000),
            false, // Don't auto-start fetch
            true,  // Seekable
            abort_token,
        );

        // Buffer should start empty
        assert_eq!(stream.fetcher.buffer.len(), 0);
        assert_eq!(stream.fetcher.start, 0);
        assert_eq!(stream.fetcher.end, None);
    }

    #[test]
    fn test_non_seekable_stream() {
        // Test non-seekable stream behavior
        let abort_token = CancellationToken::new();
        let stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            Some(1000),
            false, // Don't auto-start fetch
            false, // Not seekable
            abort_token,
        );

        assert!(!stream.seekable);
        // Note: The current implementation doesn't actually restrict seeking based on this flag
        // but this test documents the intended behavior
    }

    #[test]
    fn test_seek_within_downloaded_data_preserves_fetcher() {
        // Test that seeking within already downloaded data doesn't create a new fetcher
        let abort_token = CancellationToken::new();
        let mut stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            Some(1000),
            false, // Don't auto-start fetch
            true,  // Seekable
            abort_token,
        );

        // Simulate some downloaded data
        stream.fetcher.start = 0;
        stream.fetcher.buffer = vec![0u8; 500]; // 500 bytes downloaded starting from position 0

        // Seek within the downloaded data
        let pos = stream.seek(SeekFrom::Start(100)).unwrap();
        assert_eq!(pos, 100);
        assert_eq!(stream.read_position, 100);

        // Fetcher should still have the same start position and buffer
        assert_eq!(stream.fetcher.start, 0);
        assert_eq!(stream.fetcher.buffer.len(), 500);

        // Seek to another position within downloaded data
        let pos = stream.seek(SeekFrom::Start(250)).unwrap();
        assert_eq!(pos, 250);
        assert_eq!(stream.read_position, 250);

        // Fetcher should still be the same
        assert_eq!(stream.fetcher.start, 0);
        assert_eq!(stream.fetcher.buffer.len(), 500);
    }

    #[tokio::test]
    async fn test_seek_outside_downloaded_data_creates_new_fetcher() {
        // Test that seeking outside downloaded data creates a new fetcher
        let abort_token = CancellationToken::new();
        let mut stream = RemoteByteStream::new(
            "https://example.com/file.mp3".to_string(),
            Some(1000),
            false, // Don't auto-start fetch
            true,  // Seekable
            abort_token,
        );

        // Simulate some downloaded data
        stream.fetcher.start = 0;
        stream.fetcher.buffer = vec![0u8; 500]; // 500 bytes downloaded starting from position 0

        // Seek outside the downloaded data
        let pos = stream.seek(SeekFrom::Start(600)).unwrap();
        assert_eq!(pos, 600);
        assert_eq!(stream.read_position, 600);

        // Fetcher should have been recreated with new start position
        assert_eq!(stream.fetcher.start, 600);
        assert_eq!(stream.fetcher.buffer.len(), 0); // New fetcher starts with empty buffer
    }

    // ==== REGRESSION TESTS FOR STREAM FINISHING LOGIC ====
    // These tests prevent the race condition bug where streams would be marked as finished
    // when HTTP stream ended but there was still data in the buffer to be consumed.
    // The bug caused tracks to end prematurely (about 0.5 seconds early) in audio playback.
    // Key scenarios tested:
    // 1. Stream NOT finished when HTTP ends but buffer has data
    // 2. Stream finished when HTTP ends and buffer is empty
    // 3. Multiple reads working correctly when HTTP ends during one
    // 4. Reading all data even when HTTP ends during the call

    // Test HTTP fetcher that allows controlled data delivery
    use futures::stream;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct TestHttpFetcher {
        data_chunks: Arc<Mutex<Vec<Bytes>>>,
        current_index: Arc<Mutex<usize>>,
    }

    impl TestHttpFetcher {
        pub fn new(data_chunks: Vec<Bytes>) -> Self {
            Self {
                data_chunks: Arc::new(Mutex::new(data_chunks)),
                current_index: Arc::new(Mutex::new(0)),
            }
        }
    }

    #[async_trait::async_trait]
    impl HttpFetcher for TestHttpFetcher {
        async fn fetch_range(
            &self,
            _url: &str,
            _start: u64,
            _end: Option<u64>,
        ) -> Result<
            Box<
                dyn futures::Stream<Item = Result<Bytes, Box<dyn std::error::Error + Send + Sync>>>
                    + Send
                    + Unpin,
            >,
            Box<dyn std::error::Error + Send + Sync>,
        > {
            let data_chunks = self.data_chunks.clone();
            let current_index = self.current_index.clone();

            let stream = stream::unfold((), move |()| {
                let data_chunks = data_chunks.clone();
                let current_index = current_index.clone();

                async move {
                    let mut index = current_index.lock().unwrap();
                    let chunks = data_chunks.lock().unwrap();

                    if *index < chunks.len() {
                        let chunk = chunks[*index].clone();
                        drop(chunks);
                        *index += 1;
                        drop(index);
                        Some((Ok(chunk), ()))
                    } else {
                        None
                    }
                }
            });

            Ok(Box::new(Box::pin(stream)))
        }
    }

    /// Test that stream is NOT marked as finished when HTTP stream ends but buffer has data
    #[tokio::test]
    async fn test_regression_stream_not_finished_with_buffer_data() {
        let abort_token = CancellationToken::new();
        let fetcher = TestHttpFetcher::new(vec![Bytes::from("hello world test data")]);
        let mut stream = RemoteByteStream::new_with_fetcher(
            "https://example.com/file.mp3".to_string(),
            Some(21), // Total size: 21 bytes (length of "hello world test data")
            true,     // Auto-start fetch
            true,     // Seekable
            abort_token,
            fetcher,
        );

        // Give time for fetch to start
        switchy_async::time::sleep(switchy_async::time::Duration::from_millis(10)).await;

        // Read only part of the data (first 10 bytes)
        let mut buf = [0u8; 10];
        let bytes_read = stream.read(&mut buf).unwrap();
        assert_eq!(bytes_read, 10);
        assert_eq!(&buf[..bytes_read], b"hello worl");

        // Stream should NOT be finished because there's still data in buffer
        assert!(
            !stream.finished,
            "Stream should not be finished when buffer has data"
        );

        // Read the remaining data
        let mut buf2 = [0u8; 15];
        let bytes_read2 = stream.read(&mut buf2).unwrap();
        assert_eq!(bytes_read2, 11);
        assert_eq!(&buf2[..bytes_read2], b"d test data");

        // NOW the stream should be marked as finished
        assert!(
            stream.finished,
            "Stream should be finished after all buffer data is consumed"
        );
    }

    /// Test that stream IS marked as finished when HTTP stream ends and buffer is empty
    #[tokio::test]
    async fn test_regression_stream_finished_with_empty_buffer() {
        let abort_token = CancellationToken::new();
        let fetcher = TestHttpFetcher::new(vec![Bytes::from("hello test")]);
        let mut stream = RemoteByteStream::new_with_fetcher(
            "https://example.com/file.mp3".to_string(),
            Some(10), // Total size: 10 bytes
            true,     // Auto-start fetch
            true,     // Seekable
            abort_token,
            fetcher,
        );

        // Give time for fetch to start
        switchy_async::time::sleep(switchy_async::time::Duration::from_millis(10)).await;

        // Read all the data
        let mut buf = [0u8; 10];
        let bytes_read = stream.read(&mut buf).unwrap();
        assert_eq!(bytes_read, 10);
        assert_eq!(&buf[..bytes_read], b"hello test");

        // Stream should be finished when all data is consumed
        // (The real stream will mark as finished when HTTP stream ends and buffer is empty)
        // This happens after reading all data when the stream is properly sized

        // Try to read again - should get 0 bytes
        let mut buf2 = [0u8; 10];
        let bytes_read2 = stream.read(&mut buf2).unwrap();
        assert_eq!(bytes_read2, 0);

        // After attempting to read from finished stream, it should definitely be finished
        assert!(
            stream.finished,
            "Stream should be finished after reading all data"
        );
    }

    /// Test that multiple reads work correctly when HTTP stream ends during one of them
    #[tokio::test]
    async fn test_regression_multiple_reads_with_http_end() {
        let abort_token = CancellationToken::new();
        let fetcher =
            TestHttpFetcher::new(vec![Bytes::from("first chunk"), Bytes::from(" second end")]);
        let mut stream = RemoteByteStream::new_with_fetcher(
            "https://example.com/file.mp3".to_string(),
            Some(22), // Total size: 22 bytes ("first chunk second end")
            true,     // Auto-start fetch
            true,     // Seekable
            abort_token,
            fetcher,
        );

        // Give time for fetch to start
        switchy_async::time::sleep(switchy_async::time::Duration::from_millis(10)).await;

        // First read - should get all available data at once (both chunks)
        let mut buf1 = [0u8; 25];
        let bytes_read1 = stream.read(&mut buf1).unwrap();
        assert_eq!(bytes_read1, 22);
        assert_eq!(&buf1[..bytes_read1], b"first chunk second end");
        assert!(
            stream.finished,
            "Stream should be finished after consuming all data"
        );

        // Second read - should return 0 bytes
        let mut buf2 = [0u8; 20];
        let bytes_read2 = stream.read(&mut buf2).unwrap();
        assert_eq!(bytes_read2, 0);
        assert!(stream.finished, "Stream should remain finished");
    }

    /// Test that read returns all available data even when HTTP stream ends during the call
    #[tokio::test]
    async fn test_regression_read_all_data_on_http_end() {
        let abort_token = CancellationToken::new();
        let fetcher = TestHttpFetcher::new(vec![
            Bytes::from("chunk1"),
            Bytes::from("chunk2"),
            Bytes::from("chunk3"),
        ]);
        let mut stream = RemoteByteStream::new_with_fetcher(
            "https://example.com/file.mp3".to_string(),
            Some(18), // Total size: 18 bytes ("chunk1chunk2chunk3")
            true,     // Auto-start fetch
            true,     // Seekable
            abort_token,
            fetcher,
        );

        // Give time for fetch to start
        switchy_async::time::sleep(switchy_async::time::Duration::from_millis(10)).await;

        // Read only part of available data
        let mut buf1 = [0u8; 10];
        let bytes_read1 = stream.read(&mut buf1).unwrap();
        assert_eq!(bytes_read1, 10);
        assert_eq!(&buf1[..bytes_read1], b"chunk1chun");

        // This read should get all remaining data in one call
        let mut buf2 = [0u8; 20];
        let bytes_read2 = stream.read(&mut buf2).unwrap();
        assert_eq!(bytes_read2, 8);
        assert_eq!(&buf2[..bytes_read2], b"k2chunk3");

        // Stream should be finished since all data was consumed
        assert!(
            stream.finished,
            "Stream should be finished after consuming all buffered data"
        );
    }

    /// Test the exact bug scenario: stream finishing logic race condition
    #[tokio::test]
    async fn test_regression_stream_finishing_race_condition() {
        let abort_token = CancellationToken::new();
        let fetcher = TestHttpFetcher::new(vec![Bytes::from("test data"), Bytes::from("end")]);
        let mut stream = RemoteByteStream::new_with_fetcher(
            "https://example.com/file.mp3".to_string(),
            Some(12), // Total size: 12 bytes
            true,     // Auto-start fetch
            true,     // Seekable
            abort_token,
            fetcher,
        );

        // Give time for fetch to start
        switchy_async::time::sleep(switchy_async::time::Duration::from_millis(10)).await;

        // Read some data but not all
        let mut buf1 = [0u8; 5];
        let bytes_read1 = stream.read(&mut buf1).unwrap();
        assert_eq!(bytes_read1, 5);
        assert_eq!(&buf1[..bytes_read1], b"test ");

        // This was the critical bug: stream would be marked finished prematurely
        // even though there was still data in the buffer
        let mut buf2 = [0u8; 10];
        let bytes_read2 = stream.read(&mut buf2).unwrap();
        assert_eq!(bytes_read2, 7);
        assert_eq!(&buf2[..bytes_read2], b"dataend");

        // Only now should the stream be marked as finished
        assert!(
            stream.finished,
            "Stream should be finished only after all data is consumed"
        );

        // Verify no more data available
        let mut buf3 = [0u8; 10];
        let bytes_read3 = stream.read(&mut buf3).unwrap();
        assert_eq!(bytes_read3, 0);
    }

    /// Test that finished stream with remaining buffer data continues to return data
    #[tokio::test]
    async fn test_regression_finished_stream_with_buffer_data() {
        let abort_token = CancellationToken::new();
        let fetcher = TestHttpFetcher::new(vec![Bytes::from("testdata12")]);
        let mut stream = RemoteByteStream::new_with_fetcher(
            "https://example.com/file.mp3".to_string(),
            Some(10), // Total size: 10 bytes
            true,     // Auto-start fetch
            true,     // Seekable
            abort_token,
            fetcher,
        );

        // Give time for fetch to start
        switchy_async::time::sleep(switchy_async::time::Duration::from_millis(10)).await;

        // Read only part of the data
        let mut buf1 = [0u8; 4];
        let bytes_read1 = stream.read(&mut buf1).unwrap();
        assert_eq!(bytes_read1, 4);
        assert_eq!(&buf1[..bytes_read1], b"test");

        // At this point, stream should NOT be finished because there's still buffer data
        assert!(
            !stream.finished,
            "Stream should not be finished with remaining buffer data"
        );

        // Continue reading
        let mut buf2 = [0u8; 10];
        let bytes_read2 = stream.read(&mut buf2).unwrap();
        assert_eq!(bytes_read2, 6);
        assert_eq!(&buf2[..bytes_read2], b"data12");

        // Now stream should be finished
        assert!(
            stream.finished,
            "Stream should be finished after consuming all buffer data"
        );
    }
}
