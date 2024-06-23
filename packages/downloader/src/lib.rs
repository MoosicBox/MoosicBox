#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    error::Error,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use crate::{db::models::DownloadApiSource, queue::GenericProgressEvent};
use async_recursion::async_recursion;
use async_trait::async_trait;
use atomic_float::AtomicF64;
use audiotags::Tag;
use db::{
    create_download_task, get_download_location,
    models::{CreateDownloadTask, DownloadItem, DownloadTask},
};
use futures::StreamExt;
use id3::Timestamp;
use moosicbox_config::get_config_dir_path;
use moosicbox_core::{
    integer_range::{parse_integer_ranges, ParseIntegersError},
    sqlite::{
        db::{
            get_album_tracks, get_artist, get_artist_by_album_id, get_track, get_tracks, DbError,
        },
        menu::{get_album, GetAlbumError},
        models::{ApiSource, LibraryTrack, TrackApiSource},
    },
    types::AudioFormat,
};
use moosicbox_database::Database;
use moosicbox_files::{
    files::{
        album::{get_library_album_cover_bytes, AlbumCoverError},
        artist::{get_library_artist_cover_bytes, ArtistCoverError},
        track::{
            get_track_bytes, get_track_source, GetTrackBytesError, TrackAudioQuality,
            TrackSourceError,
        },
    },
    get_content_length, sanitize_filename, save_bytes_stream_to_file_with_speed_listener,
    GetContentLengthError, SaveBytesStreamToFileError,
};
use queue::ProgressListener;
use thiserror::Error;
use tokio::select;

#[cfg(feature = "api")]
pub mod api;

pub mod db;
pub mod queue;

#[derive(Debug, Error)]
pub enum GetDownloadPathError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("Failed to get config directory")]
    FailedToGetConfigDirectory,
    #[error("Not found")]
    NotFound,
}

pub async fn get_download_path(
    db: &dyn Database,
    location_id: Option<u64>,
) -> Result<PathBuf, GetDownloadPathError> {
    Ok(if let Some(location_id) = location_id {
        PathBuf::from_str(
            &get_download_location(db, location_id)
                .await?
                .ok_or(GetDownloadPathError::NotFound)?
                .path,
        )
        .unwrap()
    } else {
        get_config_dir_path()
            .ok_or(GetDownloadPathError::FailedToGetConfigDirectory)?
            .join("downloads")
    })
}

#[derive(Debug, Error)]
pub enum GetCreateDownloadTasksError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    GetAlbum(#[from] GetAlbumError),
    #[error(transparent)]
    ParseIntegers(#[from] ParseIntegersError),
    #[error("Invalid source")]
    InvalidSource,
    #[error("Not found")]
    NotFound,
}

#[allow(clippy::too_many_arguments)]
pub async fn get_create_download_tasks(
    db: &dyn Database,
    download_path: &Path,
    track_id: Option<u64>,
    track_ids: Option<String>,
    album_id: Option<u64>,
    album_ids: Option<String>,
    download_album_cover: bool,
    download_artist_cover: bool,
    quality: Option<TrackAudioQuality>,
    source: Option<DownloadApiSource>,
) -> Result<Vec<CreateDownloadTask>, GetCreateDownloadTasksError> {
    let mut tasks = vec![];

    if let Some(track_id) = track_id {
        tasks.extend(
            get_create_download_tasks_for_track_ids(
                db,
                &[track_id],
                download_path,
                source,
                quality,
            )
            .await?,
        );
    }

    if let Some(track_ids) = &track_ids {
        let track_ids = parse_integer_ranges(track_ids)?;

        tasks.extend(
            get_create_download_tasks_for_track_ids(db, &track_ids, download_path, source, quality)
                .await?,
        );
    }

    if let Some(album_id) = album_id {
        tasks.extend(
            get_create_download_tasks_for_album_ids(
                db,
                &[album_id],
                download_path,
                source,
                quality,
                download_album_cover,
                download_artist_cover,
            )
            .await?,
        );
    }

    if let Some(album_ids) = &album_ids {
        let album_ids = parse_integer_ranges(album_ids)?;

        tasks.extend(
            get_create_download_tasks_for_album_ids(
                db,
                &album_ids,
                download_path,
                source,
                quality,
                download_album_cover,
                download_artist_cover,
            )
            .await?,
        );
    }

    Ok(tasks)
}

pub async fn get_create_download_tasks_for_track_ids(
    db: &dyn Database,
    track_ids: &[u64],
    download_path: &Path,
    source: Option<DownloadApiSource>,
    quality: Option<TrackAudioQuality>,
) -> Result<Vec<CreateDownloadTask>, GetCreateDownloadTasksError> {
    let tracks = get_tracks(db, Some(&track_ids.to_vec())).await?;

    get_create_download_tasks_for_tracks(&tracks, download_path, source, quality)
}

pub fn get_create_download_tasks_for_tracks(
    tracks: &[LibraryTrack],
    download_path: &Path,
    source: Option<DownloadApiSource>,
    quality: Option<TrackAudioQuality>,
) -> Result<Vec<CreateDownloadTask>, GetCreateDownloadTasksError> {
    tracks
        .iter()
        .map(|track| {
            let source = if let Some(source) = source {
                source
            } else if track.qobuz_id.is_some() {
                log::debug!("Falling back to Qobuz DownloadApiSource");
                DownloadApiSource::Qobuz
            } else if track.tidal_id.is_some() {
                log::debug!("Falling back to Tidal DownloadApiSource");
                DownloadApiSource::Tidal
            } else {
                return Err(GetCreateDownloadTasksError::InvalidSource);
            };

            let quality = quality.unwrap_or(TrackAudioQuality::FlacHighestRes);

            Ok(CreateDownloadTask {
                file_path: download_path
                    .join(sanitize_filename(&track.artist))
                    .join(sanitize_filename(&track.album))
                    .join(get_filename_for_track(track))
                    .to_str()
                    .unwrap()
                    .to_string(),
                item: DownloadItem::Track {
                    track_id: track.id as u64,
                    source,
                    quality,
                },
            })
        })
        .collect::<Result<Vec<_>, _>>()
}

pub async fn get_create_download_tasks_for_album_ids(
    db: &dyn Database,
    album_ids: &[u64],
    download_path: &Path,
    source: Option<DownloadApiSource>,
    quality: Option<TrackAudioQuality>,
    download_album_cover: bool,
    download_artist_cover: bool,
) -> Result<Vec<CreateDownloadTask>, GetCreateDownloadTasksError> {
    let mut tasks = vec![];

    for album_id in album_ids {
        let tracks = get_album_tracks(db, *album_id)
            .await?
            .into_iter()
            .filter(|track| {
                if let Some(source) = source {
                    let track_source = source.into();
                    track.source == track_source
                } else {
                    track.source != TrackApiSource::Local
                }
            })
            .collect::<Vec<_>>();

        if tracks.is_empty() {
            continue;
        }

        tasks.extend(get_create_download_tasks_for_tracks(
            &tracks,
            download_path,
            source,
            quality,
        )?);

        if download_album_cover || download_artist_cover {
            let album_path = if let Some(track) = tracks.first() {
                download_path
                    .join(&sanitize_filename(&track.artist))
                    .join(&sanitize_filename(&track.album))
            } else {
                let album = get_album(db, Some(*album_id), None, None)
                    .await?
                    .ok_or(GetCreateDownloadTasksError::NotFound)?;

                download_path
                    .join(&sanitize_filename(&album.artist))
                    .join(&sanitize_filename(&album.title))
            };

            if download_album_cover && tracks.first().unwrap().artwork.is_some() {
                tasks.push(CreateDownloadTask {
                    file_path: album_path.join("cover.jpg").to_str().unwrap().to_string(),
                    item: DownloadItem::AlbumCover(*album_id),
                });
            }
            if download_artist_cover {
                let artist = get_artist(db, "id", tracks.first().unwrap().artist_id as u64)
                    .await?
                    .ok_or(GetCreateDownloadTasksError::NotFound)?;

                if artist.cover.is_some() {
                    tasks.push(CreateDownloadTask {
                        file_path: album_path
                            .parent()
                            .unwrap()
                            .join("artist.jpg")
                            .to_str()
                            .unwrap()
                            .to_string(),
                        item: DownloadItem::ArtistCover(*album_id),
                    });
                }
            }
        }
    }

    Ok(tasks)
}

#[derive(Debug, Error)]
pub enum CreateDownloadTasksError {
    #[error(transparent)]
    Db(#[from] DbError),
}

pub async fn create_download_tasks(
    db: &dyn Database,
    tasks: Vec<CreateDownloadTask>,
) -> Result<Vec<DownloadTask>, CreateDownloadTasksError> {
    let mut results = vec![];

    for task in tasks {
        results.push(create_download_task(db, &task).await?);
    }

    Ok(results)
}

fn get_filename_for_track(track: &LibraryTrack) -> String {
    let extension = "flac";

    format!(
        "{}_{}.{extension}",
        track.number,
        sanitize_filename(&track.title)
    )
}

#[derive(Debug, Error)]
pub enum DownloadTrackError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    TrackSource(#[from] TrackSourceError),
    #[error(transparent)]
    GetTrackBytes(#[from] GetTrackBytesError),
    #[error(transparent)]
    IO(#[from] tokio::io::Error),
    #[error(transparent)]
    GetContentLength(#[from] GetContentLengthError),
    #[error(transparent)]
    SaveBytesStreamToFile(#[from] SaveBytesStreamToFileError),
    #[error(transparent)]
    TagTrackFile(#[from] TagTrackFileError),
    #[error("Invalid source")]
    InvalidSource,
    #[error("Not found")]
    NotFound,
}

#[allow(clippy::too_many_arguments)]
pub async fn download_track_id(
    db: &dyn Database,
    path: &str,
    track_id: u64,
    quality: TrackAudioQuality,
    source: DownloadApiSource,
    on_progress: Arc<std::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadTrackError> {
    log::debug!("Starting download for track_id={track_id} quality={quality:?} source={source:?} path={path}");

    let track = get_track(db, track_id)
        .await?
        .ok_or(DownloadTrackError::NotFound)?;

    download_track(
        db,
        path,
        &track,
        quality,
        source,
        None,
        on_progress,
        speed,
        timeout_duration,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
async fn download_track(
    db: &dyn Database,
    path: &str,
    track: &LibraryTrack,
    quality: TrackAudioQuality,
    source: DownloadApiSource,
    start: Option<u64>,
    on_progress: Arc<std::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadTrackError> {
    match download_track_inner(
        db,
        path,
        track,
        quality,
        source,
        start,
        on_progress.clone(),
        speed.clone(),
        timeout_duration,
    )
    .await
    {
        Ok(_) => Ok(()),
        Err(err) => Err(match err {
            DownloadTrackInnerError::Db(err) => DownloadTrackError::Db(err),
            DownloadTrackInnerError::TrackSource(err) => DownloadTrackError::TrackSource(err),
            DownloadTrackInnerError::GetTrackBytes(err) => DownloadTrackError::GetTrackBytes(err),
            DownloadTrackInnerError::IO(err) => DownloadTrackError::IO(err),
            DownloadTrackInnerError::GetContentLength(err) => {
                DownloadTrackError::GetContentLength(err)
            }
            DownloadTrackInnerError::SaveBytesStreamToFile(err) => {
                DownloadTrackError::SaveBytesStreamToFile(err)
            }
            DownloadTrackInnerError::TagTrackFile(err) => DownloadTrackError::TagTrackFile(err),
            DownloadTrackInnerError::InvalidSource => DownloadTrackError::InvalidSource,
            DownloadTrackInnerError::NotFound => DownloadTrackError::NotFound,
            DownloadTrackInnerError::Timeout(start) => {
                log::warn!("Track download timed out. Trying again at start {start:?}");
                return download_track(
                    db,
                    path,
                    track,
                    quality,
                    source,
                    start,
                    on_progress,
                    speed,
                    timeout_duration,
                )
                .await;
            }
        }),
    }
}

#[derive(Debug, Error)]
pub enum DownloadTrackInnerError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    TrackSource(#[from] TrackSourceError),
    #[error(transparent)]
    GetTrackBytes(#[from] GetTrackBytesError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    GetContentLength(#[from] GetContentLengthError),
    #[error(transparent)]
    SaveBytesStreamToFile(#[from] SaveBytesStreamToFileError),
    #[error(transparent)]
    TagTrackFile(#[from] TagTrackFileError),
    #[error("Invalid source")]
    InvalidSource,
    #[error("Not found")]
    NotFound,
    #[error("Timeout")]
    Timeout(Option<u64>),
}

#[allow(clippy::too_many_arguments)]
async fn download_track_inner(
    db: &dyn Database,
    path: &str,
    track: &LibraryTrack,
    quality: TrackAudioQuality,
    source: DownloadApiSource,
    start: Option<u64>,
    on_progress: Arc<std::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadTrackInnerError> {
    log::debug!(
        "Starting download for track={track:?} quality={quality:?} source={source:?} path={path} start={start:?}"
    );

    let req = get_track_source(track.id, Some(track), db, Some(quality), ApiSource::Library);

    let result = if let Some(timeout_duration) = timeout_duration {
        select! {
            result = req => result,
            _ = tokio::time::sleep(timeout_duration) => {
                return Err(DownloadTrackInnerError::Timeout(start));
            }
        }
    } else {
        req.await
    };

    let source = match result {
        Ok(source) => source,
        Err(err) => {
            let is_timeout = err.source().is_some_and(|source| {
                if let Some(error) = source.downcast_ref::<hyper::Error>() {
                    error.is_timeout()
                        || error.is_closed()
                        || error.is_canceled()
                        || error.is_incomplete_message()
                } else {
                    source.to_string() == "operation timed out"
                }
            });

            if is_timeout {
                return Err(DownloadTrackInnerError::Timeout(start));
            }

            return Err(DownloadTrackInnerError::TrackSource(err));
        }
    };

    let size = match &source {
        moosicbox_files::files::track::TrackSource::LocalFilePath { path, .. } => {
            if let Ok(file) = tokio::fs::File::open(path).await {
                if let Ok(metadata) = file.metadata().await {
                    Some(metadata.len())
                } else {
                    None
                }
            } else {
                None
            }
        }
        moosicbox_files::files::track::TrackSource::Tidal { url, .. }
        | moosicbox_files::files::track::TrackSource::Qobuz { url, .. } => {
            get_content_length(url, start, None).await?
        }
    };

    log::debug!("Got track size: {size:?}");

    (on_progress.lock().unwrap())(GenericProgressEvent::Size { bytes: size });

    let mut bytes = get_track_bytes(
        db,
        track.id as u64,
        source,
        AudioFormat::Source,
        false,
        start,
        None,
    )
    .await?;

    if let Some(size) = size {
        bytes.size.replace(size);
    }

    let track_path = PathBuf::from_str(path).unwrap();

    tokio::fs::create_dir_all(&track_path.parent().unwrap()).await?;

    if Path::is_file(&track_path) {
        log::debug!("Track already downloaded");
        return Ok(());
    }

    log::debug!("Downloading track to track_path={track_path:?}");

    {
        let mut reader = bytes.stream;

        if let Some(timeout_duration) = timeout_duration {
            reader = reader.with_timeout(timeout_duration);
        }

        speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

        let result = save_bytes_stream_to_file_with_speed_listener(
            reader.map(|x| match x {
                Ok(Ok(x)) => Ok(x),
                Ok(Err(err)) | Err(err) => Err(err),
            }),
            &track_path,
            start,
            Box::new({
                let speed = speed.clone();
                let speed_progress = on_progress.clone();
                move |x| {
                    (speed_progress.lock().unwrap())(GenericProgressEvent::Speed {
                        bytes_per_second: x,
                    });
                    speed.store(x, std::sync::atomic::Ordering::SeqCst)
                }
            }),
            Some(Box::new(move |read, total| {
                (on_progress.lock().unwrap())(GenericProgressEvent::BytesRead { read, total });
            })),
        )
        .await;

        speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

        if let Err(err) = result {
            if let SaveBytesStreamToFileError::Read {
                bytes_read,
                ref source,
            } = err
            {
                if source.kind() == tokio::io::ErrorKind::TimedOut {
                    return Err(DownloadTrackInnerError::Timeout(Some(bytes_read)));
                }
            }

            return Err(DownloadTrackInnerError::SaveBytesStreamToFile(err));
        }
    }

    log::debug!("Finished downloading track to track_path={track_path:?}");

    tag_track_file(&track_path, track).await?;

    log::debug!("Completed track download for track_path={track_path:?}");

    Ok(())
}

#[derive(Debug, Error)]
pub enum TagTrackFileError {
    #[error(transparent)]
    Tag(#[from] audiotags::Error),
}

pub async fn tag_track_file(
    track_path: &Path,
    track: &LibraryTrack,
) -> Result<(), TagTrackFileError> {
    log::debug!("Adding tags to track_path={track_path:?}");

    let mut tag = Tag::new().read_from_path(track_path)?;

    tag.set_title(&track.title);
    tag.set_track_number(track.number as u16);
    tag.set_album_title(&track.album);
    tag.set_artist(&track.artist);
    tag.set_album_artist(&track.artist);

    if let Some(date) = &track.date_released {
        if let Ok(timestamp) = Timestamp::from_str(date) {
            tag.set_date(timestamp);
        }
    }

    tag.write_to_path(track_path.to_str().unwrap())?;

    Ok(())
}

#[derive(Debug, Error)]
pub enum DownloadAlbumError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    DownloadTrack(#[from] DownloadTrackError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    SaveBytesStreamToFile(#[from] SaveBytesStreamToFileError),
    #[error(transparent)]
    ArtistCover(#[from] ArtistCoverError),
    #[error(transparent)]
    AlbumCover(#[from] AlbumCoverError),
    #[error("Invalid source")]
    InvalidSource,
    #[error("Not found")]
    NotFound,
}

#[allow(clippy::too_many_arguments)]
pub async fn download_album_id(
    db: &dyn Database,
    path: &str,
    album_id: u64,
    try_download_album_cover: bool,
    try_download_artist_cover: bool,
    quality: TrackAudioQuality,
    source: DownloadApiSource,
    on_progress: Arc<std::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadAlbumError> {
    log::debug!("Starting download for album_id={album_id} quality={quality:?} source={source:?} path={path}");

    let track_source = source.into();
    let tracks = get_album_tracks(db, album_id)
        .await?
        .into_iter()
        .filter(|track| track.source == track_source)
        .collect::<Vec<_>>();

    for track in tracks.iter() {
        download_track(
            db,
            path,
            track,
            quality,
            source,
            None,
            on_progress.clone(),
            speed.clone(),
            timeout_duration,
        )
        .await?
    }

    log::debug!("Completed album download for {} tracks", tracks.len());

    if try_download_album_cover {
        download_album_cover(db, path, album_id, on_progress.clone(), speed.clone()).await?;
    }

    if try_download_artist_cover {
        download_artist_cover(db, path, album_id, on_progress, speed).await?;
    }

    Ok(())
}

pub async fn download_album_cover(
    db: &dyn Database,
    path: &str,
    album_id: u64,
    on_progress: Arc<std::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
) -> Result<(), DownloadAlbumError> {
    log::debug!("Downloading album cover path={path}");

    speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

    let cover_path = PathBuf::from_str(path).unwrap();

    if Path::is_file(&cover_path) {
        log::debug!("Album cover already downloaded");
        return Ok(());
    }

    let bytes = match get_library_album_cover_bytes(album_id as i32, db, true).await {
        Ok(bytes) => bytes,
        Err(err) => match err {
            AlbumCoverError::NotFound(_) => {
                log::debug!("No album cover found");
                return Ok(());
            }
            _ => {
                return Err(DownloadAlbumError::AlbumCover(err));
            }
        },
    };

    log::debug!("Got album cover size: {:?}", bytes.size);

    (on_progress.lock().unwrap())(GenericProgressEvent::Size { bytes: bytes.size });

    log::debug!("Saving album cover to {cover_path:?}");

    let result = save_bytes_stream_to_file_with_speed_listener(
        bytes.stream.map(|x| match x {
            Ok(Ok(x)) => Ok(x),
            Ok(Err(err)) | Err(err) => Err(err),
        }),
        &cover_path,
        None,
        Box::new({
            let speed = speed.clone();
            move |x| speed.store(x, std::sync::atomic::Ordering::SeqCst)
        }),
        None,
    )
    .await;

    speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

    result?;

    log::debug!("Completed album cover download");

    Ok(())
}

pub async fn download_artist_cover(
    db: &dyn Database,
    path: &str,
    album_id: u64,
    on_progress: Arc<std::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
) -> Result<(), DownloadAlbumError> {
    log::debug!("Downloading artist cover path={path}");

    let cover_path = PathBuf::from_str(path).unwrap();

    if Path::is_file(&cover_path) {
        log::debug!("Artist cover already downloaded");
        return Ok(());
    }

    let artist = get_artist_by_album_id(db, album_id)
        .await?
        .ok_or(DownloadAlbumError::NotFound)?;

    let bytes = match get_library_artist_cover_bytes(artist.id, db, true).await {
        Ok(bytes) => bytes,
        Err(err) => match err {
            ArtistCoverError::NotFound(_) => {
                log::debug!("No artist cover found");
                return Ok(());
            }
            _ => {
                return Err(DownloadAlbumError::ArtistCover(err));
            }
        },
    };

    log::debug!("Got artist cover size: {:?}", bytes.size);

    (on_progress.lock().unwrap())(GenericProgressEvent::Size { bytes: bytes.size });

    log::debug!("Saving artist cover to {cover_path:?}");

    let result = save_bytes_stream_to_file_with_speed_listener(
        bytes.stream.map(|x| match x {
            Ok(Ok(x)) => Ok(x),
            Ok(Err(err)) | Err(err) => Err(err),
        }),
        &cover_path,
        None,
        Box::new({
            let speed = speed.clone();
            move |x| speed.store(x, std::sync::atomic::Ordering::SeqCst)
        }),
        None,
    )
    .await;

    speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

    result?;

    log::debug!("Completed artist cover download");

    Ok(())
}

#[async_trait]
pub trait Downloader {
    fn speed(&self) -> Option<f64> {
        None
    }

    async fn download_track_id(
        &self,
        path: &str,
        track_id: u64,
        quality: TrackAudioQuality,
        source: DownloadApiSource,
        on_progress: ProgressListener,
        timeout_duration: Option<Duration>,
    ) -> Result<(), DownloadTrackError>;

    async fn download_album_cover(
        &self,
        path: &str,
        album_id: u64,
        on_progress: ProgressListener,
    ) -> Result<(), DownloadAlbumError>;

    async fn download_artist_cover(
        &self,
        path: &str,
        album_id: u64,
        on_progress: ProgressListener,
    ) -> Result<(), DownloadAlbumError>;
}

pub struct MoosicboxDownloader {
    speed: Arc<AtomicF64>,
    db: Arc<Box<dyn Database>>,
}

impl MoosicboxDownloader {
    pub fn new(db: Arc<Box<dyn Database>>) -> Self {
        Self {
            speed: Arc::new(AtomicF64::new(0.0)),
            db,
        }
    }
}

#[async_trait]
impl Downloader for MoosicboxDownloader {
    fn speed(&self) -> Option<f64> {
        Some(self.speed.load(std::sync::atomic::Ordering::SeqCst))
    }

    async fn download_track_id(
        &self,
        path: &str,
        track_id: u64,
        quality: TrackAudioQuality,
        source: DownloadApiSource,
        on_progress: ProgressListener,
        timeout_duration: Option<Duration>,
    ) -> Result<(), DownloadTrackError> {
        download_track_id(
            &**self.db,
            path,
            track_id,
            quality,
            source,
            Arc::new(std::sync::Mutex::new(on_progress)),
            self.speed.clone(),
            timeout_duration,
        )
        .await
    }

    async fn download_album_cover(
        &self,
        path: &str,
        album_id: u64,
        on_progress: ProgressListener,
    ) -> Result<(), DownloadAlbumError> {
        download_album_cover(
            &**self.db,
            path,
            album_id,
            Arc::new(std::sync::Mutex::new(on_progress)),
            self.speed.clone(),
        )
        .await
    }

    async fn download_artist_cover(
        &self,
        path: &str,
        album_id: u64,
        on_progress: ProgressListener,
    ) -> Result<(), DownloadAlbumError> {
        download_artist_cover(
            &**self.db,
            path,
            album_id,
            Arc::new(std::sync::Mutex::new(on_progress)),
            self.speed.clone(),
        )
        .await
    }
}
