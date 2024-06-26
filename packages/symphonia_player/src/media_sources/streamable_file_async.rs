use std::io::{Read, Seek};
use std::sync::atomic::AtomicBool;

use crossbeam_channel::{bounded, Receiver, Sender};
use log::debug;
use rangemap::RangeSet;
use reqwest::Client;
use symphonia::core::io::MediaSource;

// Used in cpal_output.rs to mute the stream when buffering.
pub static IS_STREAM_BUFFERING: AtomicBool = AtomicBool::new(false);

const CHUNK_SIZE: usize = 1024 * 128;
const FETCH_OFFSET: usize = CHUNK_SIZE / 2;

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
    pub async fn new(url: String) -> Self {
        // Get the size of the file we are streaming.
        let res = Client::new().head(&url).send().await.unwrap();
        let header = res.headers().get("Content-Length").unwrap();
        let size: usize = header.to_str().unwrap().parse().unwrap();

        StreamableFileAsync {
            url,
            buffer: vec![0; size],
            read_position: 0,
            downloaded: RangeSet::new(),
            requested: RangeSet::new(),
            receivers: Vec::new(),
        }
    }

    /// Gets the next chunk in the sequence.
    ///
    /// Returns the received bytes by sending them via `tx`.
    async fn read_chunk(tx: Sender<(usize, Vec<u8>)>, url: String, start: usize, file_size: usize) {
        let end = (start + CHUNK_SIZE).min(file_size);

        let chunk = Client::new()
            .get(url)
            .header("Range", format!("bytes={start}-{end}"))
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap()
            .to_vec();

        tx.send((start, chunk)).unwrap();
    }

    /// Polls all receivers.
    ///
    /// If there is data to receive, then write it to the buffer.
    ///
    /// Changes made are commited to `downloaded`.
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

    /// Determines if a chunk should be downloaded by getting
    /// the downloaded range that contains `self.read_position`.
    ///
    /// Returns `true` and the start index of the chunk
    /// if one should be downloaded.
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

impl Read for StreamableFileAsync {
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

        debug!("Read: read_pos[{}] read_max[{read_max}] buf[{}] write_pos[{chunk_write_pos}] download[{should_get_chunk}]", self.read_position, buf.len());
        if should_get_chunk {
            self.requested
                .insert(chunk_write_pos..chunk_write_pos + CHUNK_SIZE + 1);

            let url = self.url.clone();
            let file_size = self.buffer.len();
            let (tx, rx) = bounded(1);

            let id = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis();
            self.receivers.push((id, rx));

            tokio::spawn(async move {
                Self::read_chunk(tx, url, chunk_write_pos, file_size).await;
            });
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

impl Seek for StreamableFileAsync {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let seek_position: usize = match pos {
            std::io::SeekFrom::Start(pos) => pos as usize,
            std::io::SeekFrom::Current(pos) => {
                let pos = self.read_position as i64 + pos;
                pos.try_into().map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        format!("Invalid seek: {pos}"),
                    )
                })?
            }
            std::io::SeekFrom::End(pos) => {
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
    fn is_seekable(&self) -> bool {
        true
    }

    fn byte_len(&self) -> Option<u64> {
        Some(self.buffer.len() as u64)
    }
}
