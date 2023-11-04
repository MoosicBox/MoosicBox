use std::cmp::min;
use std::io::{Read, Seek};

use bytes::Bytes;
use crossbeam_channel::{bounded, Receiver, Sender};
use futures::StreamExt;
use lazy_static::lazy_static;
use log::debug;
use reqwest::Client;
use symphonia::core::io::MediaSource;
use tokio::runtime::{self, Runtime};

lazy_static! {
    static ref RT: Runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

pub struct RemoteByteStream {
    finished: bool,
    size: Option<u64>,
    buffer: Vec<u8>,
    read_position: usize,
    ready: Sender<()>,
    receiver: Receiver<Bytes>,
}

impl RemoteByteStream {
    pub fn new(url: String, size: Option<u64>) -> Self {
        let (tx, rx) = bounded(1);
        let (tx_ready, rx_ready) = bounded(1);

        RT.spawn(async move {
            let mut stream = Client::new().get(url).send().await.unwrap().bytes_stream();
            while let Some(item) = stream.next().await {
                debug!("Received more bytes from stream");
                let bytes = item.unwrap();
                tx.send(bytes).unwrap();
            }
            debug!("Finished reading from stream");
            tx.send(Bytes::new()).unwrap();
            rx_ready.recv().unwrap();
        });

        RemoteByteStream {
            finished: false,
            size,
            buffer: vec![],
            read_position: 0,
            ready: tx_ready,
            receiver: rx,
        }
    }
}

impl Read for RemoteByteStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.finished {
            return Ok(0);
        }

        let mut written = 0;
        let write_max = buf.len();

        debug!(
            "Read: read_pos[{}] buf[{}] buffer[{}]",
            self.read_position,
            write_max,
            self.buffer.len()
        );
        while written < write_max {
            if !self.buffer.is_empty() {
                let bytes_to_write = min(self.buffer.len(), write_max);
                buf[written..written + bytes_to_write]
                    .copy_from_slice(&self.buffer.drain(..bytes_to_write).collect::<Vec<_>>());
                written += bytes_to_write;
            } else {
                debug!("Waiting for bytes...");
                let new_bytes = self.receiver.recv().unwrap();
                let len = new_bytes.len();
                debug!("Received bytes {len}");

                if len == 0 {
                    self.finished = true;
                    self.ready.send(()).unwrap();
                    break;
                }

                let bytes_to_write = min(len, write_max - written);
                buf[written..written + bytes_to_write]
                    .copy_from_slice(&new_bytes[..bytes_to_write]);
                written += bytes_to_write;

                if len + written > write_max {
                    self.buffer.extend_from_slice(&new_bytes[bytes_to_write..]);
                    break;
                }
            }
        }

        self.read_position += written;
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
            std::io::SeekFrom::End(_pos) => todo!("Unsupported"),
        };

        debug!("Seeking: pos[{seek_position}] type[{pos:?}]");

        self.read_position = seek_position;

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
