use std::{
    io::{Read, Seek},
    sync::Arc,
    time::Duration,
};

use bytes::Bytes;
use lazy_static::lazy_static;
use symphonia::core::io::MediaSource;
use tokio::sync::Mutex;
use tokio_stream::StreamExt as _;

use super::track::TrackBytes;

lazy_static! {
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

pub struct TrackBytesMediaSource {
    pub id: usize,
    inner: Arc<Mutex<TrackBytes>>,
    started: bool,
    finished: bool,
    buf: Vec<u8>,
    sender: flume::Sender<Bytes>,
    receiver: flume::Receiver<Bytes>,
}

impl TrackBytesMediaSource {
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

        RT.spawn(async move {
            log::trace!("Starting stream listen for track bytes for writer id={id}");
            loop {
                log::debug!("Acquiring lock for inner bytes for writer id={id}");
                let mut bytes = bytes.lock().await;
                log::debug!("Acquired lock for inner bytes for writer id={id}");

                tokio::select!(
                    _ = tokio::time::sleep(Duration::from_millis(5000)) => {
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
                            Some(Err(err)) | Some(Ok(Err(err))) => {
                                moosicbox_assert::die_or_error!(
                                    "Byte stream returned error: writer id={id} {err:?}"
                                );
                            }
                        }
                    }
                );
            }
        });
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
            .recv_timeout(Duration::from_millis(10 * 1000))
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

        log::debug!(
            "TrackBytesMediaSource::read end={end} bytes.len={} buf.len={} self.buf.len={} writer id={}",
            bytes.len(),
            buf.len(),
            self.buf.len(),
            self.id,
        );

        if end == 0 {
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
