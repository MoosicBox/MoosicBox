use std::cmp::min;
use std::io::{Read, Seek};

use bytes::Bytes;
use flume::{Receiver, Sender, bounded, unbounded};
use futures::StreamExt;
use moosicbox_http::Client;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub struct RemoteByteStream {
    url: String,
    pub finished: bool,
    pub seekable: bool,
    pub size: Option<u64>,
    read_position: usize,
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
        log::debug!("Starting fetch for byte stream with range {bytes_range}");

        self.abort_handle = Some(moosicbox_task::spawn(
            "stream_utils: RemoteByteStream Fetcher",
            async move {
                log::debug!("Fetching byte stream with range {bytes_range}");

                let response = Client::new()
                    .get(&url)
                    .header("Range", &bytes_range)
                    .send()
                    .await;

                let response = match response {
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
                    moosicbox_http::StatusCode::OK
                    | moosicbox_http::StatusCode::PARTIAL_CONTENT => {}
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
            fetcher: RemoteByteStreamFetcher::new(url, 0, size, autostart_fetch, abort.clone()),
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
        let mut read_position = self.read_position;
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
            std::io::SeekFrom::Start(pos) => usize::try_from(pos).unwrap(),
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

        if self
            .size
            .is_some_and(|size| seek_position >= usize::try_from(size).unwrap())
        {
            self.fetcher.abort();
        } else {
            self.fetcher = RemoteByteStreamFetcher::new(
                self.url.clone(),
                seek_position as u64,
                self.size,
                true,
                self.abort.clone(),
            );
        }

        Ok(seek_position as u64)
    }
}
