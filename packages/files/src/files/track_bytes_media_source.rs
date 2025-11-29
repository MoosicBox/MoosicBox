//! Media source adapter for wrapping `TrackBytes` as Symphonia `MediaSource`.
//!
//! Provides a bridge between track byte streams and the Symphonia audio decoder by implementing
//! the required traits (`Read`, `Seek`, `MediaSource`) for stream-based decoding.

use std::{
    io::{Read, Seek},
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use switchy_async::sync::Mutex;
use symphonia::core::io::MediaSource;
use tokio_stream::StreamExt as _;

use super::track::TrackBytes;

/// Media source adapter that wraps `TrackBytes` for use with Symphonia decoder.
///
/// Implements `Read`, `Seek`, and `MediaSource` traits to allow streaming track bytes
/// to be used as a decoder input. Buffers incoming bytes and provides them on-demand.
pub struct TrackBytesMediaSource {
    /// Unique identifier for this media source
    pub id: usize,
    inner: Arc<Mutex<TrackBytes>>,
    started: bool,
    finished: bool,
    buf: Vec<u8>,
    sender: flume::Sender<Bytes>,
    receiver: flume::Receiver<Bytes>,
}

impl TrackBytesMediaSource {
    /// Creates a new media source from track bytes.
    #[must_use]
    pub fn new(track_bytes: TrackBytes) -> Self {
        let (sender, receiver) = flume::unbounded();
        Self {
            id: track_bytes.id,
            inner: Arc::new(Mutex::new(track_bytes)),
            started: false,
            finished: false,
            buf: vec![],
            sender,
            receiver,
        }
    }

    fn start_listening(&self) {
        let bytes = self.inner.clone();
        let sender = self.sender.clone();
        let id = self.id;

        switchy_async::runtime::Handle::current().spawn_with_name(
            "files: TrackBytesMediaSource",
            async move {
                log::trace!("Starting stream listen for track bytes for writer id={id}");
                loop {
                    log::trace!("Acquiring lock for inner bytes for writer id={id}");
                    let mut bytes = bytes.lock().await;
                    log::trace!("Acquired lock for inner bytes for writer id={id}");

                    switchy_async::select!(
                        () = switchy_async::time::sleep(Duration::from_millis(15000)) => {
                            moosicbox_assert::die_or_error!(
                                "Timed out waiting for bytes from stream for writer id={id}"
                            );
                            break;
                        }
                        response = bytes.stream.next() => {
                            match response {
                                Some(Ok(Ok(bytes))) => {
                                    log::trace!("Sending {} bytes to writer id={id}", bytes.len());
                                    if sender.send_async(bytes).await.is_err() {
                                        log::debug!("Receiver has dropped for writer id={id}");
                                        break;
                                    }
                                }
                                None => {
                                    log::trace!("Sending empty bytes to writer id={id}");
                                    if sender.send_async(Bytes::new()).await.is_err() {
                                        log::debug!("Receiver has dropped for writer id={id}");
                                    }
                                    break;
                                }
                                Some(Err(err) | Ok(Err(err))) => {
                                    moosicbox_assert::die_or_error!(
                                        "Byte stream returned error: writer id={id} {err:?}"
                                    );
                                }
                            }
                        }
                    );

                    drop(bytes);
                }
            },
        );
    }
}

impl Seek for TrackBytesMediaSource {
    fn seek(&mut self, _pos: std::io::SeekFrom) -> std::io::Result<u64> {
        moosicbox_assert::assert!(false, "Seeking is not allowed for writer id={}", self.id);
        panic!("Seeking is not allowed for writer id={}", self.id)
    }
}

impl Read for TrackBytesMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.finished {
            return Ok(0);
        }
        if !self.started {
            self.started = true;
            self.start_listening();
        }

        if !self.buf.is_empty() {
            let end = std::cmp::min(buf.len(), self.buf.len());
            buf[..end].copy_from_slice(&self.buf.drain(..end).collect::<Vec<_>>());
            return Ok(end);
        }

        let bytes = self
            .receiver
            .recv_timeout(Duration::from_millis(15 * 1000))
            .map_err(|e| {
                moosicbox_assert::die_or_error!(
                    "Timed out waiting for bytes buf.len={} self.buf.len={} writer id={}: {e:?}",
                    buf.len(),
                    self.buf.len(),
                    self.id,
                );
                std::io::Error::new(std::io::ErrorKind::TimedOut, e)
            })?;
        let end = std::cmp::min(buf.len(), bytes.len());
        buf[..end].copy_from_slice(&bytes[..end]);
        self.buf.extend_from_slice(&bytes[end..]);

        log::trace!(
            "TrackBytesMediaSource::read end={end} bytes.len={} buf.len={} self.buf.len={} writer id={}",
            bytes.len(),
            buf.len(),
            self.buf.len(),
            self.id,
        );

        // Only mark as finished if we received empty bytes (end of stream) AND there's no more buffered data
        if bytes.is_empty() && self.buf.is_empty() {
            self.finished = true;
        }

        Ok(end)
    }
}

impl Drop for TrackBytesMediaSource {
    fn drop(&mut self) {
        log::debug!("Dropping TrackBytesMediaSource writer id={}", self.id);
    }
}

impl MediaSource for TrackBytesMediaSource {
    fn is_seekable(&self) -> bool {
        false
    }

    fn byte_len(&self) -> Option<u64> {
        None
    }
}
