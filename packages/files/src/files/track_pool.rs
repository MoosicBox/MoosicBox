//! Track byte stream pooling and caching service.
//!
//! Provides a service for caching and sharing track audio byte streams across multiple concurrent
//! consumers. Prevents redundant downloads/reads by maintaining a pool of active track streams and
//! allowing multiple clients to tap into the same source stream.

#![allow(clippy::module_name_repetitions)]

use std::{
    collections::BTreeMap,
    fmt::Display,
    io::Write,
    pin::Pin,
    sync::{Arc, LazyLock, atomic::AtomicBool},
    time::{SystemTime, UNIX_EPOCH},
};

use flume::{SendError, Sender, bounded};
use futures::StreamExt;
use futures_core::Future;
use moosicbox_music_api::models::TrackSource;
use moosicbox_music_models::AudioFormat;
use moosicbox_stream_utils::{ByteWriter, stalled_monitor::StalledReadMonitor};
use strum_macros::AsRefStr;
use switchy_async::util::CancellationToken;
use thiserror::Error;
use tokio::sync::{Mutex, RwLock, Semaphore};

use crate::{
    BytesStream,
    files::{filename_from_path_str, track_pool::service::Commander},
};

use super::track::{BytesStreamItem, GetTrackBytesError, TrackBytes};

pub static HANDLE: LazyLock<Arc<RwLock<Option<service::Handle>>>> =
    LazyLock::new(|| Arc::new(RwLock::new(None)));

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

/// Errors that can occur in the track pool service.
#[derive(Debug, Error)]
pub enum TrackPoolError {
    /// Error retrieving track bytes
    #[error(transparent)]
    GetTrackBytes(#[from] GetTrackBytesError),
    /// Failed to send message through channel
    #[error("Failed to send")]
    Send,
}

/// Commands that can be sent to the track pool service.
#[derive(AsRefStr)]
pub enum Command {
    /// Fetch track bytes from cache or source
    FetchTrackBytes {
        /// Channel to send the result
        tx: Sender<TrackBytes>,
        /// Track source location
        source: TrackSource,
        /// Desired output audio format
        output_format: AudioFormat,
        /// Optional total size in bytes
        size: Option<u64>,
        /// Optional start byte offset
        start: Option<u64>,
        /// Optional end byte offset
        end: Option<u64>,
        /// Function to fetch track bytes if not cached
        fetch: FetchTrackBytesFunc,
    },
    /// Start fetching and distributing track bytes to writers
    StartFetchTrackBytes {
        /// Cache key for the track
        key: String,
        /// Stream of incoming bytes
        stream: StalledReadMonitor<BytesStreamItem, BytesStream>,
        /// Optional total size in bytes
        size: Option<u64>,
        /// Optional start byte offset
        start: Option<u64>,
        /// Optional end byte offset
        end: Option<u64>,
    },
}

impl Display for Command {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Service context for managing the track byte pool and caching.
#[derive(Default)]
pub struct Context {
    handle: Option<service::Handle>,
    semaphore: BTreeMap<String, Arc<Semaphore>>,
    pool: BTreeMap<String, TrackBytesSource>,
    token: Option<CancellationToken>,
}

impl Context {
    /// Creates a new track pool context.
    #[must_use]
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
            }

            log::trace!("No existing track in pool for key={key}",);
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
            created: switchy_time::now(),
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
        drop(ctx);
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
                let ctx = ctx.read().await;
                if let Some(track_bytes_source) = ctx.pool.get(&key) {
                    let track_bytes_source = track_bytes_source.clone();
                    switchy_async::runtime::Handle::current().spawn_with_name(
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
                drop(bytes);

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

        drop(writers);

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
        &self,
        key: String,
        stream: StalledReadMonitor<BytesStreamItem, BytesStream>,
        _size: Option<u64>,
        _start: Option<u64>,
        _end: Option<u64>,
    ) -> Result<(), GetTrackBytesError> {
        let finished = self.finished.clone();
        let writers = self.writers.clone();
        let bytes = self.bytes.clone();
        log::debug!("Starting stream processing for track bytes for key={key}");
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

                        let initial_writer_count = writers.len();
                        writers.retain_mut(|x| {
                            log::trace!(
                                "Writing {} track bytes to writer id={} for key={key}",
                                new_bytes.len(),
                                x.id
                            );
                            let write_result = x.write_all(&new_bytes);
                            if let Err(ref err) = write_result {
                                log::warn!("Writer id={} failed to write {} bytes for key={key}: {:?} - dropping writer", x.id, new_bytes.len(), err);
                            }
                            write_result.is_ok()
                        });

                        let remaining_writer_count = writers.len();
                        if remaining_writer_count < initial_writer_count {
                            log::warn!("Track pool key={key}: {} writer(s) dropped, {} remaining",
                                initial_writer_count - remaining_writer_count, remaining_writer_count);
                        }

                        if writers.is_empty() {
                            log::error!("Track pool key={key}: All writers have been dropped - marking stream as finished prematurely! This may cause audio playback to end early.");
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
        let mut final_writers = writers.lock().await;
        log::debug!(
            "Track pool stream processing completed for key={key} - {} writer(s) remaining",
            final_writers.len()
        );
        final_writers.retain_mut(|x| {
            log::debug!("Closing writer id={} for key={key}", x.id);
            x.close();
            false
        });
        drop(final_writers);

        log::debug!("Track pool stream processing fully finished for key={key}");
        Ok(())
    }
}

/// Generates a unique cache key for a track based on its source and output format.
///
/// The key includes the source type (local/remote), API source, format, track ID or path,
/// output format, and any HTTP headers for remote sources.
#[must_use]
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
            headers,
        } => format!(
            "remote:{source}:{format}:{id}:{output_format}:{headers}",
            id = track_id
                .as_ref()
                .map(|x| format!("id:{x}"))
                .as_deref()
                .unwrap_or(url),
            headers = headers
                .as_ref()
                .map(|x| x
                    .iter()
                    .map(|(k, v)| format!("{k}:{v}"))
                    .collect::<Vec<_>>()
                    .join(","))
                .as_deref()
                .unwrap_or("")
        ),
    }
}

/// Retrieves track bytes from cache or fetches them if not cached.
///
/// This is the main entry point for the track pooling system. It checks if the track is already
/// being processed or cached, and reuses existing streams when possible. Falls back to calling
/// the provided fetch function for cache misses.
///
/// # Errors
///
/// * `GetTrackBytesError::NotFound` - If the track was not found
/// * `GetTrackBytesError::IO` - If an IO error occurs
/// * `GetTrackBytesError::Commander` - If track pool service error occurs
/// * `GetTrackBytesError::Recv` - If channel receive fails
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
    if start.is_some_and(|x| x != 0) || end.is_some_and(|x| size.is_none_or(|s| s != x)) {
        log::debug!("get_or_fetch_track: Requested a specific range, eagerly fetching bytes");
        return fetch(start, end, size).await;
    }

    let Some(handle) = HANDLE.read().await.clone() else {
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

#[cfg(test)]
mod tests {
    use super::*;
    use moosicbox_music_models::{ApiSource, TrackApiSource};

    #[test]
    fn test_track_key_local_file_with_id() {
        let source = TrackSource::LocalFilePath {
            format: AudioFormat::Flac,
            path: "/music/track.flac".to_string(),
            track_id: Some("123".into()),
            source: TrackApiSource::Api(ApiSource::library()),
        };
        let key = track_key(&source, AudioFormat::Flac);
        assert_eq!(key, "local:API:Library:FLAC:id:123:FLAC");
    }

    #[test]
    fn test_track_key_local_file_without_id() {
        let source = TrackSource::LocalFilePath {
            format: AudioFormat::Flac,
            path: "/music/track.flac".to_string(),
            track_id: None,
            source: TrackApiSource::Api(ApiSource::library()),
        };
        let key = track_key(&source, AudioFormat::Flac);
        assert_eq!(key, "local:API:Library:FLAC:/music/track.flac:FLAC");
    }

    #[cfg(feature = "format-aac")]
    #[test]
    fn test_track_key_format_conversion() {
        let source = TrackSource::LocalFilePath {
            format: AudioFormat::Flac,
            path: "/music/track.flac".to_string(),
            track_id: Some("456".into()),
            source: TrackApiSource::Api(ApiSource::library()),
        };
        let key = track_key(&source, AudioFormat::Aac);
        assert_eq!(key, "local:API:Library:FLAC:id:456:AAC");
    }

    #[test]
    fn test_track_key_remote_url_with_id() {
        let source = TrackSource::RemoteUrl {
            format: AudioFormat::Flac,
            url: "https://example.com/track.flac".to_string(),
            track_id: Some("789".into()),
            source: TrackApiSource::Api(ApiSource::library()),
            headers: None,
        };
        let key = track_key(&source, AudioFormat::Flac);
        assert_eq!(key, "remote:API:Library:FLAC:id:789:FLAC:");
    }

    #[test]
    fn test_track_key_remote_url_without_id() {
        let source = TrackSource::RemoteUrl {
            format: AudioFormat::Flac,
            url: "https://example.com/track.flac".to_string(),
            track_id: None,
            source: TrackApiSource::Api(ApiSource::library()),
            headers: None,
        };
        let key = track_key(&source, AudioFormat::Flac);
        assert_eq!(
            key,
            "remote:API:Library:FLAC:https://example.com/track.flac:FLAC:"
        );
    }

    #[test]
    fn test_track_key_remote_url_with_headers() {
        let headers = vec![
            ("Authorization".to_string(), "Bearer token".to_string()),
            ("X-Custom".to_string(), "value".to_string()),
        ];
        let source = TrackSource::RemoteUrl {
            format: AudioFormat::Flac,
            url: "https://example.com/track.flac".to_string(),
            track_id: Some("999".into()),
            source: TrackApiSource::Api(ApiSource::library()),
            headers: Some(headers),
        };
        let key = track_key(&source, AudioFormat::Flac);
        assert_eq!(
            key,
            "remote:API:Library:FLAC:id:999:FLAC:Authorization:Bearer token,X-Custom:value"
        );
    }

    #[test]
    fn test_track_key_different_sources_different_keys() {
        let source1 = TrackSource::LocalFilePath {
            format: AudioFormat::Flac,
            path: "/music/track1.flac".to_string(),
            track_id: Some("1".into()),
            source: TrackApiSource::Api(ApiSource::library()),
        };
        let source2 = TrackSource::LocalFilePath {
            format: AudioFormat::Flac,
            path: "/music/track2.flac".to_string(),
            track_id: Some("2".into()),
            source: TrackApiSource::Api(ApiSource::library()),
        };

        let key1 = track_key(&source1, AudioFormat::Flac);
        let key2 = track_key(&source2, AudioFormat::Flac);

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_track_key_same_source_same_key() {
        let source = TrackSource::LocalFilePath {
            format: AudioFormat::Flac,
            path: "/music/track.flac".to_string(),
            track_id: Some("123".into()),
            source: TrackApiSource::Api(ApiSource::library()),
        };

        let key1 = track_key(&source, AudioFormat::Flac);
        let key2 = track_key(&source, AudioFormat::Flac);

        assert_eq!(key1, key2);
    }
}
