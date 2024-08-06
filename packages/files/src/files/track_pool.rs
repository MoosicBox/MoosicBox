use std::{
    collections::HashMap,
    fmt::Display,
    io::Write,
    pin::Pin,
    sync::{atomic::AtomicBool, Arc, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};

use flume::{bounded, SendError, Sender};
use futures::StreamExt;
use futures_core::Future;
use lazy_static::lazy_static;
use moosicbox_core::types::AudioFormat;
use moosicbox_music_api::TrackSource;
use moosicbox_stream_utils::{stalled_monitor::StalledReadMonitor, ByteWriter};
use strum_macros::AsRefStr;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::{
    files::{filename_from_path_str, track_pool::service::Commander},
    BytesStream,
};

use super::track::{BytesStreamItem, GetTrackBytesError, TrackBytes};

pub static HANDLE: OnceLock<service::Handle> = OnceLock::new();

lazy_static! {
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

type FetchTrackBytesFunc = Box<
    dyn Fn(
            Option<u64>,
            Option<u64>,
            Option<u64>,
        ) -> Pin<Box<dyn Future<Output = Result<TrackBytes, GetTrackBytesError>> + Send>>
        + Send,
>;

impl From<SendError<TrackBytes>> for TrackPoolError {
    fn from(_value: SendError<TrackBytes>) -> Self {
        Self::Send
    }
}

#[derive(Debug, Error)]
pub enum TrackPoolError {
    #[error(transparent)]
    GetTrackBytes(#[from] GetTrackBytesError),
    #[error("Failed to send")]
    Send,
}

#[derive(AsRefStr)]
pub enum Command {
    FetchTrackBytes {
        tx: Sender<TrackBytes>,
        source: TrackSource,
        output_format: AudioFormat,
        size: Option<u64>,
        start: Option<u64>,
        end: Option<u64>,
        fetch: FetchTrackBytesFunc,
    },
    StartFetchTrackBytes {
        key: String,
        stream: StalledReadMonitor<BytesStreamItem, BytesStream>,
        size: Option<u64>,
        start: Option<u64>,
        end: Option<u64>,
    },
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

#[derive(Default)]
pub struct Context {
    handle: Option<service::Handle>,
    semaphore: HashMap<String, Arc<Semaphore>>,
    pool: HashMap<String, TrackBytesSource>,
    token: Option<CancellationToken>,
}

impl Context {
    pub fn new() -> Self {
        Self::default()
    }

    async fn fetch_track_bytes(
        &mut self,
        source: TrackSource,
        output_format: AudioFormat,
        size: Option<u64>,
        start: Option<u64>,
        end: Option<u64>,
        fetch: FetchTrackBytesFunc,
    ) -> Result<TrackBytes, GetTrackBytesError> {
        let key = track_key(&source, output_format);
        log::debug!("get_or_fetch_track key={key}");

        let semaphore = self
            .semaphore
            .entry(key.clone())
            .or_insert_with(|| Arc::new(Semaphore::new(1)))
            .clone();

        log::trace!("Attempting to acquire permit for key={key}");
        let permit = semaphore.acquire().await?;
        log::trace!("Acquired permit for key={key}");

        {
            if let Some(existing) = self.pool.get(&key) {
                let track_bytes = existing.to_track_bytes().await?;
                log::debug!(
                    "Reusing existing track from pool for key={key} writer id={}",
                    track_bytes.id
                );

                return Ok(track_bytes);
            } else {
                log::trace!("No existing track in pool for key={key}",);
            }
        }

        let filename = match source {
            TrackSource::LocalFilePath { path, .. } => filename_from_path_str(&path),
            TrackSource::RemoteUrl { .. } => None,
        };

        let writers = Arc::new(Mutex::new(vec![]));
        let bytes = Arc::new(RwLock::new(vec![]));
        let finished = Arc::new(AtomicBool::new(false));

        log::debug!("fetch_track_bytes: Fetching track bytes for key={key}");
        let track_bytes = fetch(start, end, size).await?;
        log::debug!("fetch_track_bytes: Fetched track bytes for key={key} bytes={track_bytes:?}");
        let bytes_source = TrackBytesSource {
            key: key.clone(),
            writers: writers.clone(),
            bytes: bytes.clone(),
            size: track_bytes.size,
            format: track_bytes.format,
            created: std::time::SystemTime::now(),
            finished: finished.clone(),
            filename,
        };

        let stream = track_bytes.stream;

        let bytes = bytes_source.to_track_bytes().await?;

        if self.pool.len() > 10 {
            let entry_to_drop = {
                let mut entries = self.pool.iter().collect::<Vec<_>>();
                entries.sort_by(|a, b| {
                    a.1.created
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                        .cmp(&b.1.created.duration_since(UNIX_EPOCH).unwrap().as_millis())
                });
                moosicbox_assert::assert!(!entries.is_empty(), "Entries were empty");
                moosicbox_assert::assert!(
                    entries.first().unwrap().0.as_str() != key.as_str(),
                    "Dropped track that was just added"
                );
                entries.first().unwrap().0.clone()
            };
            self.pool.remove(&entry_to_drop);
        }

        self.pool.insert(key.clone(), bytes_source);

        self.handle
            .clone()
            .unwrap()
            .send_command_async(Command::StartFetchTrackBytes {
                key,
                stream,
                size,
                start,
                end,
            })
            .await?;

        drop(permit);

        Ok(bytes)
    }
}

pub mod service {
    moosicbox_async_service::async_service!(super::Command, super::Context, super::TrackPoolError);
}

#[moosicbox_async_service::async_trait]
impl service::Processor for service::Service {
    type Error = TrackPoolError;

    async fn on_start(&mut self) -> Result<(), Self::Error> {
        let mut ctx = self.ctx.write().await;
        ctx.token.replace(self.token.clone());
        ctx.handle.replace(self.handle());
        Ok(())
    }

    async fn on_shutdown(_ctx: Arc<RwLock<Context>>) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn process_command(
        ctx: Arc<RwLock<Context>>,
        command: Command,
    ) -> Result<(), Self::Error> {
        let cmd_str = command.as_ref().to_string();
        match command {
            Command::FetchTrackBytes {
                tx,
                source,
                output_format,
                size,
                start,
                end,
                fetch,
            } => {
                tx.send_async(
                    ctx.write()
                        .await
                        .fetch_track_bytes(source, output_format, size, start, end, fetch)
                        .await?,
                )
                .await?;
            }
            Command::StartFetchTrackBytes {
                key,
                stream,
                size,
                start,
                end,
            } => {
                if let Some(track_bytes_source) = ctx.read().await.pool.get(&key) {
                    let mut track_bytes_source = track_bytes_source.clone();
                    moosicbox_task::spawn(
                        &format!("files: track_pool process_command {cmd_str}"),
                        async move {
                            track_bytes_source
                                .start_fetch_track_bytes(key, stream, size, start, end)
                                .await
                        },
                    );
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct TrackBytesSource {
    key: String,
    writers: Arc<Mutex<Vec<ByteWriter>>>,
    bytes: Arc<RwLock<Vec<u8>>>,
    size: Option<u64>,
    format: AudioFormat,
    filename: Option<String>,
    created: SystemTime,
    finished: Arc<AtomicBool>,
}

impl TrackBytesSource {
    async fn to_stream(&self) -> Result<(usize, BytesStream), std::io::Error> {
        let writer = ByteWriter::default();
        let finished = self.finished.load(std::sync::atomic::Ordering::SeqCst);
        let stream = writer.stream();
        let mut id_writer = writer;
        let id = id_writer.id;
        let key = &self.key;
        let mut writers = self.writers.lock().await;

        log::debug!("Created TrackBytesSource stream with writer id={id} for key={key}");

        {
            let bytes = self.bytes.read().await;

            if !bytes.is_empty() {
                log::debug!(
                    "Writing {} existing bytes to writer id={id} for key {key}",
                    bytes.len(),
                );
                id_writer.write_all(bytes.as_ref())?;

                if finished {
                    id_writer.close();
                }
            }
        }

        if finished {
            log::debug!(
                "Not adding writer to finished TrackBytesSource with writer id={id} for key={key}"
            );
        } else {
            log::debug!(
                "Adding writer to TrackBytesSource with writer id={id} for key={key} (this is writer #{})",
                writers.len() + 1,
            );
            writers.push(id_writer);
        }

        Ok((id, stream.boxed()))
    }

    async fn to_track_bytes(&self) -> Result<TrackBytes, std::io::Error> {
        let (id, stream) = self.to_stream().await?;
        Ok(TrackBytes {
            id,
            stream: StalledReadMonitor::new(stream),
            size: self.size,
            original_size: self.size,
            format: self.format,
            filename: self.filename.clone(),
        })
    }

    async fn start_fetch_track_bytes(
        &mut self,
        key: String,
        stream: StalledReadMonitor<BytesStreamItem, BytesStream>,
        _size: Option<u64>,
        _start: Option<u64>,
        _end: Option<u64>,
    ) -> Result<(), GetTrackBytesError> {
        let finished = self.finished.clone();
        let writers = self.writers.clone();
        let bytes = self.bytes.clone();
        log::trace!("Starting stream listen for track bytes for key={key}");
        stream
            .filter(|_| async { !finished.load(std::sync::atomic::Ordering::SeqCst) })
            .filter_map(|x| async { x.ok() })
            .for_each(|result| async {
                match result {
                    Ok(new_bytes) => {
                        log::trace!("Received {} track bytes for key={key}", new_bytes.len());
                        bytes.write().await.extend_from_slice(&new_bytes);
                        let mut writers = writers.lock().await;
                        log::trace!(
                            "Track pool entry key={key} has {} writer{}: {}",
                            writers.len(),
                            if writers.len() == 1 { "" } else { "s" },
                            writers
                                .iter()
                                .map(|x| format!("writer id={}", x.id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        writers.retain_mut(|x| {
                            log::debug!(
                                "Writing {} track bytes to writer id={} for key={key}",
                                new_bytes.len(),
                                x.id
                            );
                            x.write_all(&new_bytes).is_ok()
                        });
                        if writers.is_empty() {
                            log::debug!("All writers have been dropped. Finished.");
                            finished.store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                    Err(err) => {
                        moosicbox_assert::die_or_error!(
                            "Error during track bytes fetch for writer key={key}: {err:?}"
                        );
                    }
                }
            })
            .await;

        finished.store(true, std::sync::atomic::Ordering::SeqCst);
        writers.lock().await.retain_mut(|x| {
            log::debug!("Closing writer id={}", x.id);
            x.close();
            false
        });

        Ok(())
    }
}

pub fn track_key(source: &TrackSource, output_format: AudioFormat) -> String {
    match source {
        TrackSource::LocalFilePath {
            format,
            path,
            track_id,
            source,
        } => {
            format!(
                "local:{source}:{format}:{id}:{output_format}",
                id = track_id
                    .as_ref()
                    .map(|x| format!("id:{x}"))
                    .as_deref()
                    .unwrap_or(path)
            )
        }
        TrackSource::RemoteUrl {
            format,
            url,
            track_id,
            source,
        } => format!(
            "remote:{source}:{format}:{id}:{output_format}",
            id = track_id
                .as_ref()
                .map(|x| format!("id:{x}"))
                .as_deref()
                .unwrap_or(url)
        ),
    }
}

pub async fn get_or_fetch_track(
    source: &TrackSource,
    output_format: AudioFormat,
    size: Option<u64>,
    start: Option<u64>,
    end: Option<u64>,
    fetch: impl Fn(
            Option<u64>,
            Option<u64>,
            Option<u64>,
        ) -> Pin<Box<dyn Future<Output = Result<TrackBytes, GetTrackBytesError>> + Send>>
        + Send
        + 'static,
) -> Result<TrackBytes, GetTrackBytesError> {
    log::debug!("get_or_fetch_track: start={start:?} end={end:?} size={size:?}");
    if start.is_some_and(|x| x != 0) || end.is_some_and(|x| !size.is_some_and(|s| s == x)) {
        log::debug!("get_or_fetch_track: Requested a specific range, eagerly fetching bytes");
        return fetch(start, end, size).await;
    }

    let Some(handle) = HANDLE.get() else {
        log::debug!("get_or_fetch_track: No service handle, eagerly fetching bytes");
        return fetch(start, end, size).await;
    };

    log::debug!("get_or_fetch_track: Fetching bytes from cache");
    let (tx, rx) = bounded(1);
    handle
        .send_command_async(Command::FetchTrackBytes {
            tx,
            source: source.clone(),
            output_format,
            size,
            start,
            end,
            fetch: Box::new(fetch),
        })
        .await?;

    let bytes = rx.recv_async().await?;

    log::debug!("get_or_fetch_track: Fetched bytes from cache: bytes={bytes:?}");

    Ok(bytes)
}
