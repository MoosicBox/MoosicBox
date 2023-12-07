use std::cmp::min;
use std::io::{Read, Seek};

use bytes::Bytes;
use crossbeam_channel::{bounded, Receiver, Sender};
use futures::StreamExt;
use lazy_static::lazy_static;
use log::{debug, info};
use reqwest::Client;
use symphonia::core::io::MediaSource;
use tokio::runtime::{self, Runtime};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

lazy_static! {
    static ref RT: Runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

pub struct RemoteByteStream {
    url: String,
    finished: bool,
    size: Option<u64>,
    read_position: usize,
    fetcher: RemoteByteStreamFetcher,
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
}

impl RemoteByteStreamFetcher {
    pub fn new(url: String, start: u64, end: Option<u64>, autostart: bool) -> Self {
        let (tx, rx) = bounded(1);
        let (tx_ready, rx_ready) = bounded(1);

        let mut fetcher = RemoteByteStreamFetcher {
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
        let start = self.start;
        let end = self.end;
        let bytes_range = format!(
            "bytes={}-{}",
            start,
            end.map(|n| n.to_string()).unwrap_or("".into())
        );
        debug!("Starting fetch for byte stream with range {bytes_range}");

        self.abort_handle = Some(RT.spawn(async move {
            debug!("Fetching byte stream with range {bytes_range}");

            let mut stream = Client::new()
                .get(url.clone())
                .header("Range", bytes_range)
                .send()
                .await
                .unwrap()
                .bytes_stream();

            while let Some(item) = stream.next().await {
                if abort.is_cancelled() {
                    debug!("ABORTING");
                    break;
                }
                debug!("Received more bytes from stream");
                let bytes = item.unwrap();
                if sender.send(bytes).is_err() {
                    info!("Aborted byte stream read");
                    return;
                }
            }

            if abort.is_cancelled() {
                debug!("ABORTED");
            } else {
                debug!("Finished reading from stream");
                if sender.send(Bytes::new()).is_ok() && ready_receiver.recv().is_err() {
                    info!("Byte stream read has been aborted");
                }
            }
        }));
    }

    fn abort(&mut self) {
        self.abort.cancel();

        if let Some(handle) = &self.abort_handle {
            debug!("Aborting request");
            handle.abort();
            self.abort_handle = None;
        } else {
            debug!("No join handle for request");
        }
        self.abort = CancellationToken::new();
    }
}

impl Drop for RemoteByteStreamFetcher {
    fn drop(&mut self) {
        self.abort();
    }
}

impl RemoteByteStream {
    pub fn new(url: String, size: Option<u64>, autostart_fetch: bool) -> Self {
        RemoteByteStream {
            url: url.clone(),
            finished: false,
            size,
            read_position: 0,
            fetcher: RemoteByteStreamFetcher::new(url, 0, size, autostart_fetch),
        }
    }
}

impl Read for RemoteByteStream {
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

            debug!(
                "Read: read_pos[{}] write_max[{}] fetcher_start[{}] buffer_len[{}] written[{}]",
                read_position, write_max, fetcher_start, buffer_len, written
            );

            let bytes_written = if fetcher_start + buffer_len > read_position {
                let fetcher_buf_start = read_position - fetcher_start;
                let bytes_to_read_from_buf = buffer_len - fetcher_buf_start;
                debug!("Reading bytes from buffer: {bytes_to_read_from_buf} (max {write_max})");
                let bytes_to_write = min(bytes_to_read_from_buf, write_max);
                buf[written..written + bytes_to_write].copy_from_slice(
                    &fetcher.buffer[fetcher_buf_start..fetcher_buf_start + bytes_to_write],
                );
                bytes_to_write
            } else {
                debug!("Waiting for bytes...");
                let new_bytes = receiver.recv().unwrap();
                fetcher.buffer.extend_from_slice(&new_bytes);
                let len = new_bytes.len();
                debug!("Received bytes {len}");

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

impl Seek for RemoteByteStream {
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

        info!("Seeking: pos[{seek_position}] type[{pos:?}]");

        self.read_position = seek_position;
        self.fetcher =
            RemoteByteStreamFetcher::new(self.url.clone(), seek_position as u64, self.size, true);

        Ok(seek_position as u64)
    }
}

impl MediaSource for RemoteByteStream {
    fn is_seekable(&self) -> bool {
        self.size.is_some()
    }

    fn byte_len(&self) -> Option<u64> {
        self.size
    }
}
