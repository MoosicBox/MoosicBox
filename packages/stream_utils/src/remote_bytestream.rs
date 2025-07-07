use std::cmp::min;
use std::io::{Read, Seek};

use bytes::Bytes;
use flume::{Receiver, Sender, bounded, unbounded};
use futures::StreamExt;
use switchy_http::Client;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct RemoteByteStream {
    url: String,
    pub finished: bool,
    pub seekable: bool,
    pub size: Option<u64>,
    read_position: u64,
    fetcher: RemoteByteStreamFetcher,
    abort: CancellationToken,
}

struct RemoteByteStreamFetcher {
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
}

impl RemoteByteStreamFetcher {
    pub fn new(
        url: String,
        start: u64,
        end: Option<u64>,
        autostart: bool,
        stream_abort: CancellationToken,
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
        let bytes_range = format!(
            "bytes={}-{}",
            start,
            end.map_or_else(String::new, |n| n.to_string())
        );
        let size_info = end.map_or_else(|| "unknown size".to_string(), |s| format!("{s} bytes"));
        log::debug!("Starting fetch for byte stream with range {bytes_range} ({size_info})");

        self.abort_handle = Some(moosicbox_task::spawn(
            "stream_utils: RemoteByteStream Fetcher",
            async move {
                log::debug!("Fetching byte stream with range {bytes_range}");

                let response = Client::new()
                    .get(&url)
                    .header("Range", &bytes_range)
                    .send()
                    .await;

                let mut response = match response {
                    Ok(response) => response,
                    Err(err) => {
                        log::error!("Failed to get stream response: {err:?}");
                        if let Err(err) = sender.send_async(Bytes::new()).await {
                            log::warn!("Failed to send empty bytes: {err:?}");
                        }
                        return;
                    }
                };

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
                        log::error!(
                            "Received error response ({}): {:?}",
                            response.status(),
                            response.text().await
                        );
                        if let Err(err) = sender.send_async(Bytes::new()).await {
                            log::warn!("Failed to send empty bytes: {err:?}");
                        }
                        return;
                    }
                }

                let mut stream = response.bytes_stream();

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

impl Drop for RemoteByteStreamFetcher {
    fn drop(&mut self) {
        log::trace!("Dropping RemoteByteStreamFetcher");
        self.abort();
    }
}

impl RemoteByteStream {
    #[must_use]
    pub fn new(
        url: String,
        size: Option<u64>,
        autostart_fetch: bool,
        seekable: bool,
        abort: CancellationToken,
    ) -> Self {
        Self {
            url: url.clone(),
            finished: false,
            seekable,
            size,
            read_position: 0,
            fetcher: RemoteByteStreamFetcher::new(url, 0, None, autostart_fetch, abort.clone()),
            abort,
        }
    }
}

impl Read for RemoteByteStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.finished {
            return Ok(0);
        }

        let mut written = 0;
        let mut read_position = usize::try_from(self.read_position).unwrap();
        let write_max = buf.len();

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
                if fetcher.abort.is_cancelled() {
                    return Ok(written);
                }
                fetcher.buffer.extend_from_slice(&new_bytes);
                let len = new_bytes.len();
                log::trace!("Received bytes {len}");

                if len == 0 {
                    // HTTP stream ended - check if we have all expected bytes in buffer
                    if let Some(expected_size) = self.size {
                        let total_received = fetcher.buffer.len() as u64;
                        #[allow(
                            clippy::cast_precision_loss,
                            clippy::cast_possible_truncation,
                            clippy::cast_sign_loss
                        )]
                        if total_received < expected_size {
                            log::warn!(
                                "Stream ended prematurely: received {} bytes, expected {} bytes ({}% complete)",
                                total_received,
                                expected_size,
                                (total_received as f64 / expected_size as f64 * 100.0) as u32
                            );
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::UnexpectedEof,
                                format!(
                                    "Incomplete download: got {total_received} of {expected_size} bytes"
                                ),
                            ));
                        }
                        log::debug!(
                            "Stream completed successfully: received {total_received} bytes as expected"
                        );
                    } else {
                        log::debug!("Stream ended with no expected size - assuming complete");
                    }
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

        self.read_position = read_position as u64;

        Ok(written)
    }
}

impl Seek for RemoteByteStream {
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
                "Seeking within received data: pos[{seek_position}] in range[{fetcher_start}..{fetcher_end})"
            );
            self.read_position = seek_position;
        } else {
            // Seeking outside received data - need new fetcher
            log::debug!(
                "Seeking outside received data: pos[{seek_position}] not in range[{fetcher_start}..{fetcher_end})"
            );
            self.read_position = seek_position;

            if self.size.is_some_and(|size| seek_position >= size) {
                log::debug!("Seeking past end of stream. Aborting fetcher.");
                self.fetcher.abort();
            } else {
                self.fetcher = RemoteByteStreamFetcher::new(
                    self.url.clone(),
                    seek_position,
                    None,
                    true,
                    self.abort.clone(),
                );
            }
        }

        Ok(seek_position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom};
    use tokio_util::sync::CancellationToken;

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
}
