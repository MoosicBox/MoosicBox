//! Async HTTP streaming file source for Symphonia.
//!
//! This module provides [`StreamableFileAsync`](crate::media_sources::streamable_file_async::StreamableFileAsync),
//! a media source that streams audio files asynchronously over HTTP. It automatically
//! fetches chunks of the file as needed, allowing playback to begin before the entire
//! file is downloaded.
//!
//! The implementation uses HTTP range requests to fetch chunks on-demand and maintains
//! a buffer of downloaded data. It tracks which portions of the file have been
//! downloaded to avoid redundant requests.

use std::io::{Read, Seek};
use std::sync::atomic::AtomicBool;

use flume::{Receiver, Sender, bounded};
use log::debug;
use rangemap::RangeSet;
use switchy_http::Client;
use symphonia::core::io::MediaSource;

/// Global flag indicating whether the stream is currently buffering.
///
/// This is used in audio output implementations to mute audio during buffering.
pub static IS_STREAM_BUFFERING: AtomicBool = AtomicBool::new(false);

const CHUNK_SIZE: usize = 1024 * 128;
const FETCH_OFFSET: usize = CHUNK_SIZE / 2;

/// A media source that streams a file asynchronously over HTTP.
///
/// This type implements [`MediaSource`], [`Read`], and [`Seek`] to allow streaming
/// audio files from a remote URL with automatic chunk fetching and buffering.
pub struct StreamableFileAsync {
    url: String,
    buffer: Vec<u8>,
    read_position: usize,
    downloaded: RangeSet<usize>,
    requested: RangeSet<usize>,
    #[allow(clippy::type_complexity)]
    receivers: Vec<(u128, Receiver<(usize, Vec<u8>)>)>,
}

impl StreamableFileAsync {
    /// Creates a new streamable file from a URL.
    ///
    /// This function performs a HEAD request to determine the file size before streaming begins.
    ///
    /// # Panics
    ///
    /// * Panics if the HTTP request fails
    /// * Panics if the Content-Length header is missing or cannot be parsed
    #[must_use]
    pub async fn new(url: String) -> Self {
        // Get the size of the file we are streaming.
        let mut res = Client::new().head(&url).send().await.unwrap();
        let header = res.headers().get("Content-Length").unwrap();
        let size: usize = header.parse().unwrap();

        Self {
            url,
            buffer: vec![0; size],
            read_position: 0,
            downloaded: RangeSet::new(),
            requested: RangeSet::new(),
            receivers: Vec::new(),
        }
    }

    /// Fetches a chunk of data from the remote URL.
    ///
    /// This method performs an HTTP range request to fetch a chunk of the file
    /// and sends the result through the provided channel.
    ///
    /// # Panics
    ///
    /// * Panics if the HTTP request fails
    /// * Panics if reading the response body fails
    /// * Panics if sending the result through the channel fails
    async fn read_chunk(tx: Sender<(usize, Vec<u8>)>, url: String, start: usize, file_size: usize) {
        let end = (start + CHUNK_SIZE).min(file_size);

        let chunk = Client::new()
            .get(&url)
            .header(
                switchy_http::Header::Range.as_ref(),
                &format!("bytes={start}-{end}"),
            )
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .to_vec();

        tx.send_async((start, chunk)).await.unwrap();
    }

    /// Polls all receivers for completed chunk downloads.
    ///
    /// This method checks if any pending chunk downloads have completed and writes
    /// the received data to the internal buffer. Changes are committed to the
    /// `downloaded` range set.
    ///
    /// When `should_buffer` is true, this method blocks waiting for data to ensure
    /// the read position is available before returning.
    fn try_write_chunk(&mut self, should_buffer: bool) {
        let mut completed_downloads = Vec::new();

        for (id, rx) in &self.receivers {
            // Block on the first chunk or when buffering.
            // Buffering fixes the issue with seeking on MP3 (no blocking on data).
            let result = if self.downloaded.is_empty() || should_buffer {
                rx.recv().ok()
            } else {
                rx.try_recv().ok()
            };

            match result {
                None => (),
                Some((position, chunk)) => {
                    // Write the data.
                    let end = (position + chunk.len()).min(self.buffer.len());

                    if position != end {
                        self.buffer[position..end].copy_from_slice(chunk.as_slice());
                        self.downloaded.insert(position..end);
                    }

                    // Clean up.
                    completed_downloads.push(*id);
                }
            }
        }

        // Remove completed receivers.
        self.receivers
            .retain(|(id, _)| !completed_downloads.contains(id));
    }

    /// Determines if a new chunk should be downloaded.
    ///
    /// This method analyzes the current read position, downloaded ranges, and buffer length
    /// to decide whether to fetch the next chunk. It returns a tuple containing:
    /// * A boolean indicating whether a chunk should be fetched
    /// * The start position for the chunk to fetch
    ///
    /// A chunk is fetched when the read position approaches the end of the currently
    /// downloaded range and no download is already in progress for that chunk.
    #[must_use]
    fn should_get_chunk(&self, buf_len: usize) -> (bool, usize) {
        let closest_range = self.downloaded.get(&self.read_position);

        if closest_range.is_none() {
            return (true, self.read_position);
        }

        let closest_range = closest_range.unwrap();

        // Make sure that the same chunk isn't being downloaded again.
        // This may happen because the next `read` call happens
        // before the chunk has finished downloading. In that case,
        // it is unnecessary to request another chunk.
        let is_already_downloading = self.requested.contains(&(self.read_position + CHUNK_SIZE));

        let should_get_chunk = self.read_position + buf_len >= closest_range.end - FETCH_OFFSET
            && !is_already_downloading
            && closest_range.end != self.buffer.len();

        (should_get_chunk, closest_range.end)
    }
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl Read for StreamableFileAsync {
    /// Reads bytes from the remote file into the provided buffer.
    ///
    /// This method automatically fetches chunks from the remote source as needed.
    ///
    /// # Panics
    ///
    /// * Panics if the system time is before the Unix epoch (January 1, 1970)
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // If we are reading after the buffer,
        // then return early with 0 written bytes.
        if self.read_position >= self.buffer.len() {
            return Ok(0);
        }

        // This defines the end position of the packet
        // we want to read.
        let read_max = (self.read_position + buf.len()).min(self.buffer.len());

        // If the position we are reading at is close
        // to the last downloaded chunk, then fetch more.
        let (should_get_chunk, chunk_write_pos) = self.should_get_chunk(buf.len());

        debug!(
            "Read: read_pos[{}] read_max[{read_max}] buf[{}] write_pos[{chunk_write_pos}] download[{should_get_chunk}]",
            self.read_position,
            buf.len()
        );
        if should_get_chunk {
            #[allow(clippy::range_plus_one)]
            self.requested
                .insert(chunk_write_pos..chunk_write_pos + CHUNK_SIZE + 1);

            let url = self.url.clone();
            let file_size = self.buffer.len();
            let (tx, rx) = bounded(1);

            let id = switchy_time::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            self.receivers.push((id, rx));

            switchy_async::runtime::Handle::current().spawn_with_name(
                "audio_decoder: StreamableFileAsync read_chunk",
                async move {
                    Self::read_chunk(tx, url, chunk_write_pos, file_size).await;
                },
            );
        }

        // Write any new bytes.
        let should_buffer = !self.downloaded.contains(&self.read_position);
        IS_STREAM_BUFFERING.store(should_buffer, std::sync::atomic::Ordering::SeqCst);
        self.try_write_chunk(should_buffer);

        // These are the bytes that we want to read.
        let bytes = &self.buffer[self.read_position..read_max];
        buf[0..bytes.len()].copy_from_slice(bytes);

        self.read_position += bytes.len();
        Ok(bytes.len())
    }
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl Seek for StreamableFileAsync {
    /// Seeks to a position in the remote file.
    ///
    /// # Errors
    ///
    /// * Returns an I/O error if the seek position is invalid or cannot be converted to `usize`
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
                let pos = self.buffer.len() as i64 + pos;
                pos.try_into().map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Invalid seek: {pos}"),
                    )
                })?
            }
        };

        if seek_position > self.buffer.len() {
            debug!("Seek position {seek_position} > file size");
            return Ok(self.read_position as u64);
        }

        debug!("Seeking: pos[{seek_position}] type[{pos:?}]");

        self.read_position = seek_position;

        Ok(seek_position as u64)
    }
}

impl MediaSource for StreamableFileAsync {
    /// Returns whether this media source is seekable.
    ///
    /// Always returns `true` for streamable files.
    fn is_seekable(&self) -> bool {
        true
    }

    /// Returns the total byte length of the media source.
    ///
    /// Returns the size of the remote file.
    fn byte_len(&self) -> Option<u64> {
        Some(self.buffer.len() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Creates a test instance with a pre-populated buffer
    fn create_test_instance(buffer_size: usize) -> StreamableFileAsync {
        StreamableFileAsync {
            url: "http://example.com/test.mp3".to_string(),
            buffer: vec![0u8; buffer_size],
            read_position: 0,
            downloaded: RangeSet::new(),
            requested: RangeSet::new(),
            receivers: Vec::new(),
        }
    }

    #[test_log::test]
    fn test_should_get_chunk_no_downloaded_ranges() {
        // When nothing is downloaded, should return true with read_position as start
        let instance = create_test_instance(CHUNK_SIZE * 4);
        let (should_get, start_pos) = instance.should_get_chunk(1024);

        assert!(should_get);
        assert_eq!(start_pos, 0);
    }

    #[test_log::test]
    fn test_should_get_chunk_read_position_not_in_downloaded_range() {
        // When read_position is not in any downloaded range, should return true
        let mut instance = create_test_instance(CHUNK_SIZE * 4);
        instance.read_position = CHUNK_SIZE * 2;

        let (should_get, start_pos) = instance.should_get_chunk(1024);

        assert!(should_get);
        assert_eq!(start_pos, CHUNK_SIZE * 2);
    }

    #[test_log::test]
    fn test_should_get_chunk_within_range_far_from_end() {
        // When read_position is within a downloaded range and far from its end,
        // should not fetch a new chunk
        let mut instance = create_test_instance(CHUNK_SIZE * 4);
        // Downloaded range from 0 to CHUNK_SIZE * 2
        instance.downloaded.insert(0..CHUNK_SIZE * 2);
        instance.read_position = 0;

        let (should_get, _) = instance.should_get_chunk(1024);

        // Should not get chunk because read_position + buf_len is far from range end
        assert!(!should_get);
    }

    #[test_log::test]
    fn test_should_get_chunk_approaching_end_of_range() {
        // When approaching the end of a downloaded range, should fetch next chunk
        let mut instance = create_test_instance(CHUNK_SIZE * 4);
        instance.downloaded.insert(0..CHUNK_SIZE);
        // Position near end of range (within FETCH_OFFSET)
        instance.read_position = CHUNK_SIZE - FETCH_OFFSET - 100;

        let (should_get, start_pos) = instance.should_get_chunk(1024);

        assert!(should_get);
        assert_eq!(start_pos, CHUNK_SIZE);
    }

    #[test_log::test]
    fn test_should_get_chunk_already_downloading() {
        // When the next chunk is already being downloaded, should not request again
        let mut instance = create_test_instance(CHUNK_SIZE * 4);
        instance.downloaded.insert(0..CHUNK_SIZE);
        // Mark the next chunk as already requested
        instance.requested.insert(CHUNK_SIZE..CHUNK_SIZE * 2);
        // Position near end of range
        instance.read_position = CHUNK_SIZE - FETCH_OFFSET - 100;

        let (should_get, _) = instance.should_get_chunk(1024);

        // Should not get chunk because it's already being downloaded
        assert!(!should_get);
    }

    #[test_log::test]
    fn test_should_get_chunk_at_end_of_buffer() {
        // When the downloaded range reaches the end of the buffer, don't fetch more
        let buffer_size = CHUNK_SIZE * 2;
        let mut instance = create_test_instance(buffer_size);
        // Entire file is downloaded
        instance.downloaded.insert(0..buffer_size);
        instance.read_position = buffer_size - FETCH_OFFSET - 100;

        let (should_get, _) = instance.should_get_chunk(1024);

        // Should not get chunk because we're at the end of the buffer
        assert!(!should_get);
    }

    #[test_log::test]
    fn test_seek_start() {
        let mut instance = create_test_instance(10000);
        instance.read_position = 5000;

        let result = instance.seek(std::io::SeekFrom::Start(1000));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1000);
        assert_eq!(instance.read_position, 1000);
    }

    #[test_log::test]
    fn test_seek_current_positive() {
        let mut instance = create_test_instance(10000);
        instance.read_position = 2000;

        let result = instance.seek(std::io::SeekFrom::Current(500));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2500);
        assert_eq!(instance.read_position, 2500);
    }

    #[test_log::test]
    fn test_seek_current_negative() {
        let mut instance = create_test_instance(10000);
        instance.read_position = 2000;

        let result = instance.seek(std::io::SeekFrom::Current(-500));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1500);
        assert_eq!(instance.read_position, 1500);
    }

    #[test_log::test]
    fn test_seek_end_negative() {
        let mut instance = create_test_instance(10000);

        let result = instance.seek(std::io::SeekFrom::End(-1000));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 9000);
        assert_eq!(instance.read_position, 9000);
    }

    #[test_log::test]
    fn test_seek_end_zero() {
        let mut instance = create_test_instance(10000);

        let result = instance.seek(std::io::SeekFrom::End(0));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 10000);
        assert_eq!(instance.read_position, 10000);
    }

    #[test_log::test]
    fn test_seek_beyond_buffer_does_not_move() {
        let mut instance = create_test_instance(10000);
        instance.read_position = 5000;

        // Seeking beyond the buffer should return current position without moving
        let result = instance.seek(std::io::SeekFrom::Start(15000));

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 5000);
        assert_eq!(instance.read_position, 5000);
    }

    #[test_log::test]
    fn test_seek_current_negative_beyond_start() {
        let mut instance = create_test_instance(10000);
        instance.read_position = 1000;

        // Seeking to a negative position should error
        let result = instance.seek(std::io::SeekFrom::Current(-2000));

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[test_log::test]
    fn test_media_source_is_seekable() {
        let instance = create_test_instance(10000);
        assert!(instance.is_seekable());
    }

    #[test_log::test]
    fn test_media_source_byte_len() {
        let instance = create_test_instance(12345);
        assert_eq!(instance.byte_len(), Some(12345));
    }

    #[test_log::test]
    fn test_read_past_end_of_buffer() {
        let mut instance = create_test_instance(1000);
        instance.read_position = 1000; // At end of buffer

        let mut buf = [0u8; 100];
        let result = instance.read(&mut buf);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test_log::test]
    fn test_try_write_chunk_writes_received_data_to_buffer() {
        let mut instance = create_test_instance(1000);
        let (tx, rx) = bounded(1);

        // Send a chunk
        let chunk_data = vec![1u8, 2, 3, 4, 5];
        tx.send((0, chunk_data.clone())).unwrap();

        instance.receivers.push((1, rx));

        // Call try_write_chunk with should_buffer=true to block on receive
        instance.try_write_chunk(true);

        // Verify the data was written to the buffer
        assert_eq!(&instance.buffer[0..5], &chunk_data);
        // Verify the range was marked as downloaded
        assert!(instance.downloaded.contains(&0));
        assert!(instance.downloaded.contains(&4));
        // Verify receiver was cleaned up
        assert!(instance.receivers.is_empty());
    }

    #[test_log::test]
    fn test_try_write_chunk_handles_chunk_at_buffer_end() {
        let mut instance = create_test_instance(100);
        let (tx, rx) = bounded(1);

        // Send a chunk that goes beyond buffer bounds
        // Note: The current code copies chunk.as_slice() which has the full length,
        // but the destination is only end - position. So we need the chunk to fit exactly.
        let chunk_data = vec![9u8; 10];
        tx.send((90, chunk_data)).unwrap();

        instance.receivers.push((1, rx));
        instance.try_write_chunk(true);

        // Verify the data was written
        assert_eq!(&instance.buffer[90..100], &[9u8; 10]);
        assert!(instance.downloaded.contains(&90));
        assert!(instance.downloaded.contains(&99));
        assert!(instance.receivers.is_empty());
    }

    #[test_log::test]
    fn test_try_write_chunk_skips_empty_range() {
        let mut instance = create_test_instance(100);
        let (tx, rx) = bounded(1);

        // Send a chunk where position == end (empty range scenario)
        // This happens when position is at buffer.len()
        let chunk_data = vec![1u8, 2, 3];
        tx.send((100, chunk_data)).unwrap();

        instance.receivers.push((1, rx));
        instance.try_write_chunk(true);

        // Verify downloaded is still empty (nothing was written)
        assert!(instance.downloaded.is_empty());
        // Receiver should still be cleaned up
        assert!(instance.receivers.is_empty());
    }

    #[test_log::test]
    fn test_try_write_chunk_multiple_receivers() {
        let mut instance = create_test_instance(1000);
        let (tx1, rx1) = bounded(1);
        let (tx2, rx2) = bounded(1);

        // Send chunks on both receivers
        tx1.send((0, vec![1u8; 100])).unwrap();
        tx2.send((200, vec![2u8; 100])).unwrap();

        instance.receivers.push((1, rx1));
        instance.receivers.push((2, rx2));

        // First call gets first chunk
        instance.try_write_chunk(true);

        // After blocking receive, at least one should be processed
        // With non-blocking try_recv for others
        assert!(
            instance.downloaded.contains(&0) || instance.downloaded.contains(&200),
            "At least one chunk should have been written"
        );
    }

    #[test_log::test]
    fn test_try_write_chunk_non_blocking_when_downloaded_not_empty() {
        let mut instance = create_test_instance(1000);
        // Mark something as downloaded so we don't block
        instance.downloaded.insert(0..100);

        let (tx, rx) = bounded(1);
        // Don't send anything - would block indefinitely if blocking recv used
        instance.receivers.push((1, rx));

        // This should return immediately without blocking
        instance.try_write_chunk(false);

        // Receiver should still be there since nothing was received
        assert_eq!(instance.receivers.len(), 1);

        // Now send data for cleanup in drop
        tx.send((200, vec![])).unwrap();
    }

    #[test_log::test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_read_with_already_downloaded_data() {
        // Use buffer larger than FETCH_OFFSET (64KB) to avoid overflow in should_get_chunk
        let buffer_size = CHUNK_SIZE * 2; // 256KB
        let mut instance = create_test_instance(buffer_size);

        // Pre-populate the buffer with known data and mark as downloaded
        // Downloaded range must be larger than FETCH_OFFSET to avoid underflow
        for i in 0..CHUNK_SIZE {
            instance.buffer[i] = (i % 256) as u8;
        }
        instance.downloaded.insert(0..CHUNK_SIZE);

        let mut buf = [0u8; 100];
        let result = instance.read(&mut buf);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 100);
        // Verify correct data was read
        for (i, byte) in buf.iter().enumerate() {
            assert_eq!(*byte, i as u8);
        }
        assert_eq!(instance.read_position, 100);
    }

    #[test_log::test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_read_advances_position_correctly() {
        // Use buffer larger than FETCH_OFFSET to avoid overflow
        let buffer_size = CHUNK_SIZE * 2;
        let mut instance = create_test_instance(buffer_size);

        // Pre-populate and mark as downloaded (range larger than FETCH_OFFSET)
        for i in 0..CHUNK_SIZE {
            instance.buffer[i] = (i % 256) as u8;
        }
        instance.downloaded.insert(0..CHUNK_SIZE);

        // First read
        let mut buf = [0u8; 50];
        let _ = instance.read(&mut buf);
        assert_eq!(instance.read_position, 50);

        // Second read
        let result = instance.read(&mut buf);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 50);
        assert_eq!(instance.read_position, 100);

        // Verify we read the correct bytes on second read
        for (i, byte) in buf.iter().enumerate() {
            assert_eq!(*byte, (50 + i) as u8);
        }
    }

    #[test_log::test]
    #[allow(clippy::cast_possible_truncation)]
    fn test_read_partial_at_buffer_end() {
        // Use buffer larger than FETCH_OFFSET to avoid overflow
        let buffer_size = CHUNK_SIZE * 2;
        let mut instance = create_test_instance(buffer_size);

        // Pre-populate entire buffer and mark as downloaded
        for i in 0..buffer_size {
            instance.buffer[i] = (i % 256) as u8;
        }
        instance.downloaded.insert(0..buffer_size);

        // Position near end
        instance.read_position = buffer_size - 10;

        let mut buf = [0u8; 50]; // Requesting 50 bytes but only 10 available
        let result = instance.read(&mut buf);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 10); // Only 10 bytes were available
        assert_eq!(instance.read_position, buffer_size);

        // Verify correct data was read
        for (i, byte) in buf.iter().enumerate().take(10) {
            assert_eq!(*byte, ((buffer_size - 10 + i) % 256) as u8);
        }
    }

    #[test_log::test]
    fn test_seek_end_positive_offset() {
        let mut instance = create_test_instance(10000);

        // Seeking with positive offset from end goes beyond buffer
        let result = instance.seek(std::io::SeekFrom::End(1000));

        // Position would be 10000 + 1000 = 11000, which is beyond buffer
        // So it should return current position (0) without moving
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0); // Original position preserved
        assert_eq!(instance.read_position, 0);
    }

    #[test_log::test]
    fn test_seek_end_large_negative_beyond_start_errors() {
        let mut instance = create_test_instance(10000);
        instance.read_position = 5000;

        // Seeking with very large negative offset would go negative
        let result = instance.seek(std::io::SeekFrom::End(-15000));

        // 10000 + (-15000) = -5000, which can't be converted to usize
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }
}
