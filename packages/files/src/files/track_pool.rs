use std::{
    collections::HashMap,
    io::Write,
    pin::Pin,
    sync::{atomic::AtomicBool, Arc},
    time::{SystemTime, UNIX_EPOCH},
};

use futures::StreamExt;
use futures_core::Future;
use lazy_static::lazy_static;
use moosicbox_core::types::AudioFormat;
use moosicbox_stream_utils::{stalled_monitor::StalledReadMonitor, ByteWriter};
use once_cell::sync::Lazy;
use tokio::sync::{Mutex, RwLock, Semaphore};

use crate::BytesStream;

use super::track::{GetTrackBytesError, TrackBytes, TrackSource};

lazy_static! {
    static ref RT: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

static TRACK_SEMAPHORE: Lazy<RwLock<HashMap<String, Arc<Semaphore>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

static TRACK_POOL: Lazy<RwLock<HashMap<String, TrackBytesSource>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

struct TrackBytesSource {
    key: String,
    writers: Arc<Mutex<Vec<ByteWriter>>>,
    bytes: Arc<RwLock<Vec<u8>>>,
    size: Option<u64>,
    format: AudioFormat,
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
            format: self.format,
        })
    }
}

pub fn track_key(source: &TrackSource, output_format: AudioFormat) -> String {
    match source {
        TrackSource::LocalFilePath {
            format,
            path,
            track_id,
        } => {
            format!(
                "local:{format}:{id}:{output_format}",
                id = track_id
                    .map(|x| format!("id:{x}"))
                    .as_deref()
                    .unwrap_or(path)
            )
        }
        TrackSource::Tidal {
            format,
            url,
            track_id,
        } => format!(
            "tidal:{format}:{id}:{output_format}",
            id = track_id
                .map(|x| format!("id:{x}"))
                .as_deref()
                .unwrap_or(url)
        ),
        TrackSource::Qobuz {
            format,
            url,
            track_id,
        } => format!(
            "qobuz:{format}:{id}:{output_format}",
            id = track_id
                .map(|x| format!("id:{x}"))
                .as_deref()
                .unwrap_or(url)
        ),
    }
}

pub async fn get_or_fetch_track(
    source: &TrackSource,
    output_format: AudioFormat,
    fetch: impl FnOnce() -> Pin<Box<dyn Future<Output = Result<TrackBytes, GetTrackBytesError>> + Send>>,
) -> Result<TrackBytes, GetTrackBytesError> {
    let key = track_key(source, output_format);
    log::debug!("get_or_fetch_track key={key}");

    let semaphore = TRACK_SEMAPHORE
        .write()
        .await
        .entry(key.clone())
        .or_insert_with(|| Arc::new(Semaphore::new(1)))
        .clone();

    log::trace!("Attempting to acquire permit for key={key}");
    let permit = semaphore.acquire().await?;
    log::trace!("Acquired permit for key={key}");

    {
        if let Some(existing) = TRACK_POOL.read().await.get(&key) {
            let track_bytes = existing.to_track_bytes().await?;
            log::debug!(
                "Reusing existing track from pool for key={key} writer id={}",
                track_bytes.id
            );
            return Ok(track_bytes);
        }
    }

    let writers = Arc::new(Mutex::new(vec![]));
    let bytes = Arc::new(RwLock::new(vec![]));
    let finished = Arc::new(AtomicBool::new(false));

    log::debug!("Fetching track bytes for key={key}");
    let track_bytes = fetch().await?;
    let bytes_source = TrackBytesSource {
        key: key.clone(),
        writers: writers.clone(),
        bytes: bytes.clone(),
        size: track_bytes.size,
        format: track_bytes.format,
        created: std::time::SystemTime::now(),
        finished: finished.clone(),
    };

    let stream = track_bytes.stream;

    RT.spawn({
        let key = key.clone();
        async move {
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
        }
    });

    let bytes = bytes_source.to_track_bytes().await?;

    let mut pool = TRACK_POOL.write().await;

    if pool.len() > 10 {
        let entry_to_drop = {
            let mut entries = pool.iter().collect::<Vec<_>>();
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
        pool.remove(&entry_to_drop);
    }

    pool.insert(key, bytes_source);

    drop(permit);

    Ok(bytes)
}
