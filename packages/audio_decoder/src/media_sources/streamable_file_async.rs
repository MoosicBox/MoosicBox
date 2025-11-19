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
