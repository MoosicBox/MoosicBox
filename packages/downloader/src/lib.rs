#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::redundant_pub_crate)]

use std::{
    error::Error,
    num::ParseIntError,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, LazyLock},
    time::Duration,
};

use crate::{db::models::DownloadApiSource, queue::GenericProgressEvent};
use async_recursion::async_recursion;
use async_trait::async_trait;
use atomic_float::AtomicF64;
use db::{
    create_download_task, get_download_location,
    models::{CreateDownloadTask, DownloadItem, DownloadTask},
};
use futures::StreamExt;
use gimbal_database::profiles::LibraryDatabase;
use id3::Timestamp;
use moosicbox_audiotags::Tag;
use moosicbox_config::get_config_dir_path;
use moosicbox_files::{
    GetContentLengthError, SaveBytesStreamToFileError,
    files::{
        album::{AlbumCoverError, get_album_cover_bytes},
        artist::{ArtistCoverError, get_artist_cover_bytes},
        track::{GetTrackBytesError, TrackSourceError, get_track_bytes},
    },
    get_content_length, sanitize_filename, save_bytes_stream_to_file_with_speed_listener,
};
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_music_api::{
    AlbumError, ArtistError, MusicApi, MusicApis, MusicApisError, SourceToMusicApi as _,
    TrackError, TracksError,
    models::{ImageCoverSize, TrackAudioQuality, TrackSource},
};
use moosicbox_music_models::{
    Album, Artist, AudioFormat, Track, TrackApiSource,
    id::{Id, IdType, ParseIdsError, parse_id_ranges},
};
use queue::ProgressListener;
use regex::{Captures, Regex};
use thiserror::Error;
use tokio::select;

#[cfg(feature = "api")]
pub mod api;

pub(crate) mod db;
pub mod queue;

#[derive(Debug, Error)]
pub enum GetDownloadPathError {
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error("Failed to get config directory")]
    FailedToGetConfigDirectory,
    #[error("Not found")]
    NotFound,
}

/// # Panics
///
/// * If the path cannot be created from the download location string
///
/// # Errors
///
/// * If there is a database error
/// * If there is no config dir set
/// * If the download path is not found
pub async fn get_download_path(
    db: &LibraryDatabase,
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
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error(transparent)]
    Artist(#[from] ArtistError),
    #[error(transparent)]
    Album(#[from] AlbumError),
    #[error(transparent)]
    Tracks(#[from] TracksError),
    #[error(transparent)]
    ParseInt(#[from] ParseIntError),
    #[error(transparent)]
    ParseIds(#[from] ParseIdsError),
    #[error("Invalid source")]
    InvalidSource,
    #[error("Not found")]
    NotFound,
}

/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If IDs fail to parse
/// * If given an invalid `ApiSource`
#[allow(clippy::too_many_arguments)]
pub async fn get_create_download_tasks(
    api: &dyn MusicApi,
    download_path: &Path,
    track_id: Option<String>,
    track_ids: Option<String>,
    album_id: Option<String>,
    album_ids: Option<String>,
    download_album_cover: bool,
    download_artist_cover: bool,
    quality: Option<TrackAudioQuality>,
    source: Option<DownloadApiSource>,
) -> Result<Vec<CreateDownloadTask>, GetCreateDownloadTasksError> {
    let mut tasks = vec![];

    if let Some(album_id) = album_id {
        tasks.extend(
            #[allow(unreachable_code)]
            get_create_download_tasks_for_album_ids(
                api,
                &[Id::try_from_str(&album_id, api.source(), IdType::Album)?],
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
        #[allow(unreachable_code)]
        let album_ids = parse_id_ranges(album_ids, api.source(), IdType::Album)?;

        tasks.extend(
            get_create_download_tasks_for_album_ids(
                api,
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

    if let Some(track_id) = track_id {
        tasks.extend(
            #[allow(unreachable_code)]
            get_create_download_tasks_for_track_ids(
                api,
                &[Id::try_from_str(&track_id, api.source(), IdType::Track)?],
                download_path,
                source,
                quality,
            )
            .await?,
        );
    }

    if let Some(track_ids) = &track_ids {
        let track_ids = parse_id_ranges(track_ids, api.source(), IdType::Track)?;

        tasks.extend(
            get_create_download_tasks_for_track_ids(
                api,
                &track_ids,
                download_path,
                source,
                quality,
            )
            .await?,
        );
    }

    Ok(tasks)
}

/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If IDs fail to parse
/// * If given an invalid `ApiSource`
pub async fn get_create_download_tasks_for_track_ids(
    api: &dyn MusicApi,
    track_ids: &[Id],
    download_path: &Path,
    source: Option<DownloadApiSource>,
    quality: Option<TrackAudioQuality>,
) -> Result<Vec<CreateDownloadTask>, GetCreateDownloadTasksError> {
    let tracks = api.tracks(Some(track_ids), None, None, None, None).await?;
    log::debug!(
        "get_create_download_tasks_for_track_ids: track_ids={track_ids:?} tracks={:?}",
        &tracks.items()
    );

    get_create_download_tasks_for_tracks(api, &tracks, download_path, source, quality).await
}

/// # Panics
///
/// * If the track `Path` fails to be converted to a `str`
///
/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If IDs fail to parse
/// * If given an invalid `ApiSource`
pub async fn get_create_download_tasks_for_tracks(
    api: &dyn MusicApi,
    tracks: &[Track],
    download_path: &Path,
    source: Option<DownloadApiSource>,
    quality: Option<TrackAudioQuality>,
) -> Result<Vec<CreateDownloadTask>, GetCreateDownloadTasksError> {
    let mut tasks = vec![];

    for track in tracks {
        static REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/mnt/(\w+)").unwrap());

        #[allow(clippy::manual_let_else)]
        let source = {
            if let Some(source) = source {
                source
            } else {
                match track.track_source {
                    TrackApiSource::Local => {
                        return Err(GetCreateDownloadTasksError::InvalidSource);
                    }
                    #[cfg(feature = "tidal")]
                    TrackApiSource::Tidal => DownloadApiSource::Tidal,
                    #[cfg(feature = "qobuz")]
                    TrackApiSource::Qobuz => DownloadApiSource::Qobuz,
                    #[cfg(feature = "yt")]
                    TrackApiSource::Yt => DownloadApiSource::Yt,
                }
            }
        };

        let quality = quality.unwrap_or(TrackAudioQuality::FlacHighestRes);

        let album = api.album(&track.album_id).await?;

        let path = download_path
            .join(sanitize_filename(&track.artist))
            .join(sanitize_filename(&track.album))
            .join(get_filename_for_track(track))
            .to_str()
            .unwrap()
            .to_string();

        let path = if std::env::consts::OS == "windows" {
            REGEX
                .replace(&path, |caps: &Captures| {
                    format!("{}:", caps[1].to_uppercase())
                })
                .replace('/', "\\")
        } else {
            path
        };

        tasks.push(CreateDownloadTask {
            file_path: path,
            item: DownloadItem::Track {
                track_id: track.id.clone(),
                source,
                quality,
                artist_id: track.artist_id.clone(),
                artist: track.artist.clone(),
                album_id: track.album_id.clone(),
                album: track.album.clone(),
                title: track.title.clone(),
                contains_cover: album.is_some_and(|x| x.artwork.is_some()),
            },
        });
    }

    Ok(tasks)
}

/// # Panics
///
/// * If the album `Path` fails to be converted to a `str`
///
/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If IDs fail to parse
/// * If given an invalid `ApiSource`
#[allow(clippy::too_many_arguments)]
pub async fn get_create_download_tasks_for_album_ids(
    api: &dyn MusicApi,
    album_ids: &[Id],
    download_path: &Path,
    source: Option<DownloadApiSource>,
    quality: Option<TrackAudioQuality>,
    download_album_cover: bool,
    download_artist_cover: bool,
) -> Result<Vec<CreateDownloadTask>, GetCreateDownloadTasksError> {
    let mut tasks = vec![];

    for album_id in album_ids {
        let album = api
            .album(album_id)
            .await?
            .ok_or(GetCreateDownloadTasksError::NotFound)?;

        if download_album_cover || download_artist_cover {
            static REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/mnt/(\w+)").unwrap());

            let album_path = download_path
                .join(sanitize_filename(&album.artist))
                .join(sanitize_filename(&album.title));

            let path = album_path.join("cover.jpg").to_str().unwrap().to_string();

            let path = if std::env::consts::OS == "windows" {
                REGEX
                    .replace(&path, |caps: &Captures| {
                        format!("{}:", caps[1].to_uppercase())
                    })
                    .replace('/', "\\")
            } else {
                path
            };

            if download_artist_cover {
                static REGEX: LazyLock<Regex> =
                    LazyLock::new(|| Regex::new(r"/mnt/(\w+)").unwrap());

                let artist = api
                    .artist(&album.artist_id)
                    .await?
                    .ok_or(GetCreateDownloadTasksError::NotFound)?;

                let path = album_path
                    .parent()
                    .unwrap()
                    .join("artist.jpg")
                    .to_str()
                    .unwrap()
                    .to_string();

                let path = if std::env::consts::OS == "windows" {
                    REGEX
                        .replace(&path, |caps: &Captures| {
                            format!("{}:", caps[1].to_uppercase())
                        })
                        .replace('/', "\\")
                } else {
                    path
                };

                if artist.cover.is_some() {
                    #[allow(unreachable_code)]
                    tasks.push(CreateDownloadTask {
                        file_path: path,
                        item: DownloadItem::ArtistCover {
                            album_id: album_id.to_owned(),
                            source: api.source().into(),
                            artist_id: artist.id,
                            title: artist.title,
                            contains_cover: artist.cover.is_some(),
                        },
                    });
                }
            }

            if download_album_cover && album.artwork.is_some() {
                #[allow(unreachable_code)]
                tasks.push(CreateDownloadTask {
                    file_path: path,
                    item: DownloadItem::AlbumCover {
                        album_id: album_id.to_owned(),
                        source: api.source().into(),
                        artist_id: album.artist_id,
                        artist: album.artist,
                        title: album.title,
                        contains_cover: album.artwork.is_some(),
                    },
                });
            }
        }

        let tracks = api
            .album_tracks(album_id, None, None, None, None)
            .await?
            .with_rest_of_items_in_batches()
            .await?
            .into_iter()
            .filter(|track| {
                source.map_or_else(
                    || track.track_source != TrackApiSource::Local,
                    |source| {
                        let track_source = source.into();
                        track.track_source == track_source
                    },
                )
            })
            .collect::<Vec<_>>();

        if tracks.is_empty() {
            continue;
        }

        tasks.extend(
            get_create_download_tasks_for_tracks(api, &tracks, download_path, source, quality)
                .await?,
        );
    }

    Ok(tasks)
}

#[derive(Debug, Error)]
pub enum CreateDownloadTasksError {
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// # Errors
///
/// * If the download tasks fail to be created in the database
pub async fn create_download_tasks(
    db: &LibraryDatabase,
    tasks: Vec<CreateDownloadTask>,
) -> Result<Vec<DownloadTask>, CreateDownloadTasksError> {
    let mut results = vec![];

    for task in tasks {
        results.push(create_download_task(db, &task).await?);
    }

    Ok(results)
}

fn get_filename_for_track(track: &Track) -> String {
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
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error(transparent)]
    MusicApis(#[from] MusicApisError),
    #[error(transparent)]
    Track(#[from] TrackError),
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

/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If failed to fetch the track source
/// * If failed to add tags to the downloaded audio file
/// * If an IO error occurs
/// * If there is an error saving the bytes stream to the file
/// * If failed to get the content length of the audio data to download
/// * If given an invalid `ApiSource`
/// * If an item is not found
#[allow(clippy::too_many_arguments)]
pub async fn download_track_id(
    api: &dyn MusicApi,
    path: &str,
    track_id: &Id,
    quality: TrackAudioQuality,
    source: DownloadApiSource,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
    timeout_duration: Option<Duration>,
) -> Result<Track, DownloadTrackError> {
    log::debug!(
        "Starting download for track_id={track_id} quality={quality:?} source={source:?} path={path}"
    );

    let track = api
        .track(track_id)
        .await?
        .ok_or(DownloadTrackError::NotFound)?;

    download_track(
        api,
        path,
        &track,
        quality,
        source,
        None,
        on_progress,
        speed,
        timeout_duration,
    )
    .await?;

    Ok(track)
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
async fn download_track(
    api: &dyn MusicApi,
    path: &str,
    track: &Track,
    quality: TrackAudioQuality,
    source: DownloadApiSource,
    start: Option<u64>,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadTrackError> {
    match download_track_inner(
        api,
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
        Ok(()) => Ok(()),
        Err(e) => Err(match e {
            DownloadTrackInnerError::DatabaseFetch(e) => DownloadTrackError::DatabaseFetch(e),
            DownloadTrackInnerError::Track(e) => DownloadTrackError::Track(e),
            DownloadTrackInnerError::TrackSource(e) => DownloadTrackError::TrackSource(e),
            DownloadTrackInnerError::GetTrackBytes(e) => DownloadTrackError::GetTrackBytes(e),
            DownloadTrackInnerError::IO(e) => DownloadTrackError::IO(e),
            DownloadTrackInnerError::GetContentLength(e) => DownloadTrackError::GetContentLength(e),
            DownloadTrackInnerError::SaveBytesStreamToFile(e) => {
                DownloadTrackError::SaveBytesStreamToFile(e)
            }
            DownloadTrackInnerError::TagTrackFile(e) => DownloadTrackError::TagTrackFile(e),
            DownloadTrackInnerError::InvalidSource => DownloadTrackError::InvalidSource,
            DownloadTrackInnerError::NotFound => DownloadTrackError::NotFound,
            DownloadTrackInnerError::Timeout(start) => {
                log::warn!("Track download timed out. Trying again at start {start:?}");
                return download_track(
                    api,
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
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error(transparent)]
    Track(#[from] TrackError),
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

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
async fn download_track_inner(
    api: &dyn MusicApi,
    path: &str,
    track: &Track,
    quality: TrackAudioQuality,
    source: DownloadApiSource,
    mut start: Option<u64>,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadTrackInnerError> {
    log::debug!(
        "Starting download for track={track:?} quality={quality:?} source={source:?} path={path} start={start:?}"
    );

    let req = api.track_source(track.into(), quality);

    let result = if let Some(timeout_duration) = timeout_duration {
        select! {
            result = req => result,
            () = tokio::time::sleep(timeout_duration) => {
                return Err(DownloadTrackInnerError::Timeout(start));
            }
        }
    } else {
        req.await
    };

    let source = match result {
        Ok(Some(source)) => source,
        Ok(None) => {
            return Err(DownloadTrackInnerError::InvalidSource);
        }
        Err(e) => {
            let is_timeout = e.source().is_some_and(|source| {
                source.downcast_ref::<hyper::Error>().map_or_else(
                    || source.to_string() == "operation timed out",
                    |error| {
                        error.is_timeout()
                            || error.is_closed()
                            || error.is_canceled()
                            || error.is_incomplete_message()
                    },
                )
            });

            if is_timeout {
                return Err(DownloadTrackInnerError::Timeout(start));
            }

            return Err(e.into());
        }
    };

    let size = match &source {
        TrackSource::LocalFilePath { path, .. } => {
            if let Ok(file) = tokio::fs::File::open(path).await {
                (file.metadata().await).map_or(None, |metadata| Some(metadata.len()))
            } else {
                None
            }
        }
        TrackSource::RemoteUrl { url, .. } => get_content_length(url, start, None).await?,
    };

    log::debug!("Got track size: {size:?}");

    (on_progress.lock().await)(GenericProgressEvent::Size { bytes: size }).await;

    let mut bytes = get_track_bytes(
        api,
        &track.id,
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
        if start.is_none() {
            if let Ok(metadata) = std::fs::File::open(&track_path).and_then(|x| x.metadata()) {
                let len = metadata.len();
                start = Some(len);
                log::debug!("Resuming track download from {len} bytes");
            } else {
                return Ok(());
            }
        }
    }

    log::debug!("Downloading track to track_path={track_path:?} start={start:?}");

    {
        let mut reader = bytes.stream;

        if let Some(timeout_duration) = timeout_duration {
            reader = reader.with_timeout(timeout_duration);
        }

        speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

        let result = save_bytes_stream_to_file_with_speed_listener(
            reader.map(|x| match x {
                Ok(Ok(x)) => Ok(x),
                Ok(Err(e)) | Err(e) => Err(e),
            }),
            &track_path,
            start,
            Box::new({
                let speed = speed.clone();
                let speed_progress = on_progress.clone();
                move |x| {
                    let speed = speed.clone();
                    let speed_progress = speed_progress.clone();
                    Box::pin(async move {
                        (speed_progress.lock().await)(GenericProgressEvent::Speed {
                            bytes_per_second: x,
                        })
                        .await;
                        speed.store(x, std::sync::atomic::Ordering::SeqCst);
                    })
                }
            }),
            Some(Box::new(move |read, total| {
                let on_progress = on_progress.clone();
                Box::pin(async move {
                    (on_progress.lock().await)(GenericProgressEvent::BytesRead { read, total })
                        .await;
                })
            })),
        )
        .await;

        speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

        if let Err(e) = result {
            if let SaveBytesStreamToFileError::Read {
                bytes_read,
                ref source,
            } = e
            {
                if source.kind() == tokio::io::ErrorKind::TimedOut {
                    return Err(DownloadTrackInnerError::Timeout(Some(bytes_read)));
                }
            }

            return Err(DownloadTrackInnerError::SaveBytesStreamToFile(e));
        }
    }

    log::debug!("Finished downloading track to track_path={track_path:?}");

    tag_track_file(&track_path, track)?;

    log::debug!("Completed track download for track_path={track_path:?}");

    Ok(())
}

#[derive(Debug, Error)]
pub enum TagTrackFileError {
    #[error(transparent)]
    Tag(#[from] moosicbox_audiotags::Error),
}

/// # Panics
///
/// * If the track number fails to be converted to a `u16`
/// * If the track `Path` fails to be converted to a `str`
///
/// # Errors
///
/// * If `moosicbox_audiotags` fails to tag the audio file
pub fn tag_track_file(track_path: &Path, track: &Track) -> Result<(), TagTrackFileError> {
    log::debug!("Adding tags to track_path={track_path:?}");

    let mut tag = Tag::new().read_from_path(track_path)?;

    tag.set_title(&track.title);
    tag.set_track_number(u16::try_from(track.number).unwrap());
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
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error(transparent)]
    MusicApis(#[from] MusicApisError),
    #[error(transparent)]
    DownloadTrack(#[from] DownloadTrackError),
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    SaveBytesStreamToFile(#[from] SaveBytesStreamToFileError),
    #[error(transparent)]
    ArtistCover(#[from] ArtistCoverError),
    #[error(transparent)]
    Artist(#[from] ArtistError),
    #[error(transparent)]
    Album(#[from] AlbumError),
    #[error(transparent)]
    Tracks(#[from] TracksError),
    #[error(transparent)]
    AlbumCover(#[from] AlbumCoverError),
    #[error("Invalid source")]
    InvalidSource,
    #[error("Not found")]
    NotFound,
}

/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If failed to fetch the track source
/// * If failed to add tags to the downloaded audio file
/// * If an IO error occurs
/// * If there is an error saving the bytes stream to the file
/// * If failed to get the content length of the audio data to download
/// * If given an invalid `ApiSource`
/// * If an item is not found
#[allow(clippy::too_many_arguments)]
pub async fn download_album_id(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    path: &str,
    album_id: &Id,
    try_download_album_cover: bool,
    try_download_artist_cover: bool,
    quality: TrackAudioQuality,
    source: DownloadApiSource,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadAlbumError> {
    log::debug!(
        "Starting download for album_id={album_id} quality={quality:?} source={source:?} path={path}"
    );

    let track_source = source.into();
    let tracks = api
        .album_tracks(album_id, None, None, None, None)
        .await?
        .with_rest_of_items_in_batches()
        .await?
        .into_iter()
        .filter(|track| track.track_source == track_source)
        .collect::<Vec<_>>();

    for track in &tracks {
        download_track(
            api,
            path,
            track,
            quality,
            source,
            None,
            on_progress.clone(),
            speed.clone(),
            timeout_duration,
        )
        .await?;
    }

    log::debug!("Completed album download for {} tracks", tracks.len());

    if try_download_album_cover {
        download_album_cover(api, db, path, album_id, on_progress.clone(), speed.clone()).await?;
    }

    if try_download_artist_cover {
        download_artist_cover(api, db, path, album_id, on_progress, speed).await?;
    }

    Ok(())
}

/// # Panics
///
/// * If the track `Path` fails to be converted to a `str`
///
/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If failed to fetch the track source
/// * If failed to add tags to the downloaded audio file
/// * If an IO error occurs
/// * If there is an error saving the bytes stream to the file
/// * If failed to get the content length of the audio data to download
/// * If given an invalid `ApiSource`
/// * If an item is not found
pub async fn download_album_cover(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    path: &str,
    album_id: &Id,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
) -> Result<Album, DownloadAlbumError> {
    log::debug!("Downloading album cover path={path}");

    speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

    let cover_path = PathBuf::from_str(path).unwrap();

    let album = api
        .album(album_id)
        .await?
        .ok_or(DownloadAlbumError::NotFound)?;

    if Path::is_file(&cover_path) {
        log::debug!("Album cover already downloaded");
        return Ok(album);
    }

    let bytes = match get_album_cover_bytes(api, db, &album, ImageCoverSize::Max, true).await {
        Ok(bytes) => bytes,
        Err(e) => match e {
            AlbumCoverError::NotFound(_) => {
                log::debug!("No album cover found");
                return Ok(album);
            }
            _ => {
                return Err(DownloadAlbumError::AlbumCover(e));
            }
        },
    };

    log::debug!("Got album cover size: {:?}", bytes.size);

    (on_progress.lock().await)(GenericProgressEvent::Size { bytes: bytes.size }).await;

    log::debug!("Saving album cover to {cover_path:?}");

    let result = save_bytes_stream_to_file_with_speed_listener(
        bytes.stream.map(|x| match x {
            Ok(Ok(x)) => Ok(x),
            Ok(Err(e)) | Err(e) => Err(e),
        }),
        &cover_path,
        None,
        Box::new({
            let speed = speed.clone();
            move |x| {
                let speed = speed.clone();
                Box::pin(async move { speed.store(x, std::sync::atomic::Ordering::SeqCst) })
            }
        }),
        None,
    )
    .await;

    speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

    result?;

    log::debug!("Completed album cover download");

    Ok(album)
}

/// # Panics
///
/// * If the track `Path` fails to be converted to a `str`
///
/// # Errors
///
/// * If there is a database error
/// * If there are errors fetching track/album/artist info
/// * If failed to fetch the track source
/// * If failed to add tags to the downloaded audio file
/// * If an IO error occurs
/// * If there is an error saving the bytes stream to the file
/// * If failed to get the content length of the audio data to download
/// * If given an invalid `ApiSource`
/// * If an item is not found
pub async fn download_artist_cover(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    path: &str,
    album_id: &Id,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
) -> Result<Artist, DownloadAlbumError> {
    log::debug!("Downloading artist cover path={path}");

    let cover_path = PathBuf::from_str(path).unwrap();

    let artist = api
        .album_artist(album_id)
        .await?
        .ok_or(DownloadAlbumError::NotFound)?;

    if Path::is_file(&cover_path) {
        log::debug!("Artist cover already downloaded");
        return Ok(artist);
    }

    let bytes = match get_artist_cover_bytes(api, db, &artist, ImageCoverSize::Max, true).await {
        Ok(bytes) => bytes,
        Err(e) => match e {
            ArtistCoverError::NotFound(..) => {
                log::debug!("No artist cover found");
                return Ok(artist);
            }
            _ => {
                return Err(DownloadAlbumError::ArtistCover(e));
            }
        },
    };

    log::debug!("Got artist cover size: {:?}", bytes.size);

    (on_progress.lock().await)(GenericProgressEvent::Size { bytes: bytes.size }).await;

    log::debug!("Saving artist cover to {cover_path:?}");

    let result = save_bytes_stream_to_file_with_speed_listener(
        bytes.stream.map(|x| match x {
            Ok(Ok(x)) => Ok(x),
            Ok(Err(e)) | Err(e) => Err(e),
        }),
        &cover_path,
        None,
        Box::new({
            let speed = speed.clone();
            move |x| {
                let speed = speed.clone();
                Box::pin(async move { speed.store(x, std::sync::atomic::Ordering::SeqCst) })
            }
        }),
        None,
    )
    .await;

    speed.store(0.0, std::sync::atomic::Ordering::SeqCst);

    result?;

    log::debug!("Completed artist cover download");

    Ok(artist)
}

#[async_trait]
pub trait Downloader {
    fn speed(&self) -> Option<f64> {
        None
    }

    async fn download_track_id(
        &self,
        path: &str,
        track_id: &Id,
        quality: TrackAudioQuality,
        source: DownloadApiSource,
        on_progress: ProgressListener,
        timeout_duration: Option<Duration>,
    ) -> Result<Track, DownloadTrackError>;

    async fn download_album_cover(
        &self,
        path: &str,
        album_id: &Id,
        source: DownloadApiSource,
        on_progress: ProgressListener,
    ) -> Result<Album, DownloadAlbumError>;

    async fn download_artist_cover(
        &self,
        path: &str,
        album_id: &Id,
        source: DownloadApiSource,
        on_progress: ProgressListener,
    ) -> Result<Artist, DownloadAlbumError>;
}

pub struct MoosicboxDownloader {
    speed: Arc<AtomicF64>,
    db: LibraryDatabase,
    music_apis: MusicApis,
}

impl MoosicboxDownloader {
    #[must_use]
    pub fn new(db: LibraryDatabase, music_apis: MusicApis) -> Self {
        Self {
            speed: Arc::new(AtomicF64::new(0.0)),
            db,
            music_apis,
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
        track_id: &Id,
        quality: TrackAudioQuality,
        source: DownloadApiSource,
        on_progress: ProgressListener,
        timeout_duration: Option<Duration>,
    ) -> Result<Track, DownloadTrackError> {
        download_track_id(
            &**self.music_apis.get(source.into())?,
            path,
            track_id,
            quality,
            source,
            Arc::new(tokio::sync::Mutex::new(on_progress)),
            self.speed.clone(),
            timeout_duration,
        )
        .await
    }

    async fn download_album_cover(
        &self,
        path: &str,
        album_id: &Id,
        source: DownloadApiSource,
        on_progress: ProgressListener,
    ) -> Result<Album, DownloadAlbumError> {
        download_album_cover(
            &**self.music_apis.get(source.into())?,
            &self.db,
            path,
            album_id,
            Arc::new(tokio::sync::Mutex::new(on_progress)),
            self.speed.clone(),
        )
        .await
    }

    async fn download_artist_cover(
        &self,
        path: &str,
        album_id: &Id,
        source: DownloadApiSource,
        on_progress: ProgressListener,
    ) -> Result<Artist, DownloadAlbumError> {
        download_artist_cover(
            &**self.music_apis.get(source.into())?,
            &self.db,
            path,
            album_id,
            Arc::new(tokio::sync::Mutex::new(on_progress)),
            self.speed.clone(),
        )
        .await
    }
}
