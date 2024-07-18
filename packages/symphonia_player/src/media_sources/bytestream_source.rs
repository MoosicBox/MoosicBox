use std::cmp::min;
use std::io::{Read, Seek};

use bytes::Bytes;
use flume::{bounded, Receiver, Sender};
use futures::{Stream, StreamExt};
use symphonia::core::io::MediaSource;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

type ByteStreamType =
    Box<dyn Stream<Item = Result<Bytes, std::io::Error>> + Send + std::marker::Unpin>;

pub struct ByteStreamSource {
    finished: bool,
    seekable: bool,
    size: Option<u64>,
    read_position: usize,
    fetcher: ByteStreamSourceFetcher,
    abort: CancellationToken,
}

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

impl ByteStreamSourceFetcher {
    pub fn new(
        stream: ByteStreamType,
        start: u64,
        end: Option<u64>,
        autostart: bool,
        stream_abort: CancellationToken,
    ) -> Self {
        let (tx, rx) = bounded(1);
        let (tx_ready, rx_ready) = bounded(1);

        let mut fetcher = ByteStreamSourceFetcher {
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

    fn start_fetch(&mut self, mut stream: ByteStreamType) {
        let sender = self.sender.clone();
        let ready_receiver = self.ready_receiver.clone();
        let abort = self.abort.clone();
        let stream_abort = self.stream_abort.clone();
        let start = self.start;
        let end = self.end;
        log::debug!("Starting fetch for byte stream with range start={start} end={end:?}");

        self.abort_handle = Some(
            tokio::task::Builder::new()
                .name("symphonia_player: ByteStreamSource Fetcher")
                .spawn(async move {
                    log::debug!("Fetching byte stream with range start={start} end={end:?}");

                    while let Some(item) = tokio::select! {
                        resp = stream.next() => resp,
                        _ = abort.cancelled() => {
                            log::debug!("Aborted");
                            None
                        }
                        _ = stream_abort.cancelled() => {
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
                })
                .unwrap(),
        );
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

impl Drop for ByteStreamSourceFetcher {
    fn drop(&mut self) {
        self.abort();
    }
}

impl ByteStreamSource {
    pub fn new(
        stream: ByteStreamType,
        size: Option<u64>,
        autostart_fetch: bool,
        seekable: bool,
        abort: CancellationToken,
    ) -> Self {
        ByteStreamSource {
            finished: false,
            seekable,
            size,
            read_position: 0,
            fetcher: ByteStreamSourceFetcher::new(stream, 0, size, autostart_fetch, abort.clone()),
            abort,
        }
    }
}

impl Read for ByteStreamSource {
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
            let fetcher_start = fetcher.start as usize;

            log::debug!(
                "Read: read_pos[{}] write_max[{}] fetcher_start[{}] buffer_len[{}] written[{}]",
                read_position,
                write_max,
                fetcher_start,
                buffer_len,
                written
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

impl Seek for ByteStreamSource {
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
    fn is_seekable(&self) -> bool {
        log::debug!("seekable={} size={:?}", self.seekable, self.size);
        self.seekable && self.size.is_some()
    }

    fn byte_len(&self) -> Option<u64> {
        log::debug!("byte_len={:?}", self.size);
        self.size
    }
}
