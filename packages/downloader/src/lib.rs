#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    error::Error,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use crate::db::models::DownloadApiSource;
use async_recursion::async_recursion;
use async_trait::async_trait;
use audiotags::Tag;
use db::{
    create_download_task, get_download_location,
    models::{CreateDownloadTask, DownloadItem, DownloadTask},
};
use futures::StreamExt;
use id3::Timestamp;
use moosicbox_config::get_config_dir_path;
use moosicbox_core::{
    app::Db,
    integer_range::{parse_integer_ranges, ParseIntegersError},
    sqlite::{
        db::{get_album, get_album_tracks, get_artist_by_album_id, get_track, get_tracks, DbError},
        models::{LibraryTrack, TrackApiSource},
    },
    types::AudioFormat,
};
use moosicbox_files::{
    files::{
        album::{get_library_album_cover_bytes, AlbumCoverError},
        artist::{get_library_artist_cover_bytes, ArtistCoverError},
        track::{
            get_track_bytes, get_track_source, GetTrackBytesError, TrackAudioQuality,
            TrackSourceError,
        },
    },
    sanitize_filename, save_bytes_stream_to_file,
};
use thiserror::Error;
use tokio::{
    io::{AsyncSeekExt, AsyncWriteExt, BufWriter},
    select,
};

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

pub fn get_download_path(
    db: &Db,
    location_id: Option<u64>,
) -> Result<PathBuf, GetDownloadPathError> {
    Ok(if let Some(location_id) = location_id {
        PathBuf::from_str(
            &get_download_location(&db.library.lock().as_ref().unwrap().inner, location_id)?
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
    ParseIntegers(#[from] ParseIntegersError),
    #[error("Not found")]
    NotFound,
}

pub fn get_create_download_tasks(
    db: &Db,
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
        tasks.extend(get_create_download_tasks_for_track_ids(
            db,
            &[track_id],
            download_path,
            source,
            quality,
        )?);
    }

    if let Some(track_ids) = &track_ids {
        let track_ids = parse_integer_ranges(track_ids)?;

        tasks.extend(get_create_download_tasks_for_track_ids(
            db,
            &track_ids,
            download_path,
            source,
            quality,
        )?);
    }

    if let Some(album_id) = album_id {
        tasks.extend(get_create_download_tasks_for_album_ids(
            db,
            &[album_id],
            download_path,
            source,
            quality,
            download_album_cover,
            download_artist_cover,
        )?);
    }

    if let Some(album_ids) = &album_ids {
        let album_ids = parse_integer_ranges(album_ids)?;

        tasks.extend(get_create_download_tasks_for_album_ids(
            db,
            &album_ids,
            download_path,
            source,
            quality,
            download_album_cover,
            download_artist_cover,
        )?);
    }

    Ok(tasks)
}

pub fn get_create_download_tasks_for_track_ids(
    db: &Db,
    track_ids: &[u64],
    download_path: &Path,
    source: Option<DownloadApiSource>,
    quality: Option<TrackAudioQuality>,
) -> Result<Vec<CreateDownloadTask>, GetCreateDownloadTasksError> {
    let tracks = get_tracks(
        &db.library.lock().as_ref().unwrap().inner,
        Some(&track_ids.iter().map(|id| *id as i32).collect::<Vec<_>>()),
    )?;

    Ok(tracks
        .into_iter()
        .map(|track| CreateDownloadTask {
            file_path: download_path
                .join(&sanitize_filename(&track.artist))
                .join(&sanitize_filename(&track.album))
                .join(&get_filename_for_track(&track))
                .to_str()
                .unwrap()
                .to_string(),
            item: DownloadItem::Track(track.id as u64),
            source,
            quality,
        })
        .collect::<Vec<_>>())
}

pub fn get_create_download_tasks_for_album_ids(
    db: &Db,
    album_ids: &[u64],
    download_path: &Path,
    source: Option<DownloadApiSource>,
    quality: Option<TrackAudioQuality>,
    download_album_cover: bool,
    download_artist_cover: bool,
) -> Result<Vec<CreateDownloadTask>, GetCreateDownloadTasksError> {
    let mut tasks = vec![];

    for album_id in album_ids {
        let tracks =
            get_album_tracks(&db.library.lock().as_ref().unwrap().inner, *album_id as i32)?
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

        tasks.extend(
            tracks
                .iter()
                .map(|track| CreateDownloadTask {
                    file_path: download_path
                        .join(&sanitize_filename(&track.artist))
                        .join(&sanitize_filename(&track.album))
                        .join(&get_filename_for_track(&track))
                        .to_str()
                        .unwrap()
                        .to_string(),
                    item: DownloadItem::Track(track.id as u64),
                    source,
                    quality,
                })
                .collect::<Vec<_>>(),
        );

        if download_album_cover || download_artist_cover {
            let album_path = tracks
                .first()
                .map(|track| {
                    Ok::<_, GetCreateDownloadTasksError>(
                        download_path
                            .join(&sanitize_filename(&track.artist))
                            .join(&sanitize_filename(&track.album)),
                    )
                })
                .unwrap_or_else(|| {
                    let album =
                        get_album(&db.library.lock().as_ref().unwrap().inner, *album_id as i32)?
                            .ok_or(GetCreateDownloadTasksError::NotFound)?;

                    Ok(download_path
                        .join(&sanitize_filename(&album.artist))
                        .join(&sanitize_filename(&album.title)))
                })?;

            if download_album_cover {
                tasks.push(CreateDownloadTask {
                    file_path: album_path.join("cover.jpg").to_str().unwrap().to_string(),
                    item: DownloadItem::AlbumCover(*album_id),
                    source,
                    quality,
                });
            }
            if download_artist_cover {
                tasks.push(CreateDownloadTask {
                    file_path: album_path
                        .parent()
                        .unwrap()
                        .join("artist.jpg")
                        .to_str()
                        .unwrap()
                        .to_string(),
                    item: DownloadItem::ArtistCover(*album_id),
                    source,
                    quality,
                });
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

pub fn create_download_tasks(
    db: &Db,
    tasks: Vec<CreateDownloadTask>,
) -> Result<Vec<DownloadTask>, CreateDownloadTasksError> {
    let db = &db.library.lock().unwrap().inner;

    Ok(tasks
        .into_iter()
        .map(|task| create_download_task(db, &task))
        .collect::<Result<Vec<_>, _>>()?)
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
    TagTrackFile(#[from] TagTrackFileError),
    #[error("Invalid source")]
    InvalidSource,
    #[error("Not found")]
    NotFound,
}

pub async fn download_track_id(
    db: &Db,
    path: &str,
    track_id: u64,
    quality: Option<TrackAudioQuality>,
    source: Option<DownloadApiSource>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadTrackError> {
    log::debug!("Starting download for track_id={track_id} quality={quality:?} source={source:?} path={path}");

    let track = get_track(&db.library.lock().as_ref().unwrap().inner, track_id as i32)?
        .ok_or(DownloadTrackError::NotFound)?;

    download_track(db, path, &track, quality, source, None, timeout_duration).await
}

#[async_recursion]
async fn download_track(
    db: &Db,
    path: &str,
    track: &LibraryTrack,
    quality: Option<TrackAudioQuality>,
    source: Option<DownloadApiSource>,
    start: Option<u64>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadTrackError> {
    match download_track_inner(db, path, track, quality, source, start, timeout_duration).await {
        Ok(_) => Ok(()),
        Err(err) => Err(match err {
            DownloadTrackInnerError::Db(err) => DownloadTrackError::Db(err),
            DownloadTrackInnerError::TrackSource(err) => DownloadTrackError::TrackSource(err),
            DownloadTrackInnerError::GetTrackBytes(err) => DownloadTrackError::GetTrackBytes(err),
            DownloadTrackInnerError::IO(err) => DownloadTrackError::IO(err),
            DownloadTrackInnerError::TagTrackFile(err) => DownloadTrackError::TagTrackFile(err),
            DownloadTrackInnerError::InvalidSource => DownloadTrackError::InvalidSource,
            DownloadTrackInnerError::NotFound => DownloadTrackError::NotFound,
            DownloadTrackInnerError::Timeout(start) => {
                log::warn!("Track download timed out. Trying again at start {start:?}");
                return download_track(db, path, track, quality, source, start, timeout_duration)
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
    TagTrackFile(#[from] TagTrackFileError),
    #[error("Invalid source")]
    InvalidSource,
    #[error("Not found")]
    NotFound,
    #[error("Timeout")]
    Timeout(Option<u64>),
}

async fn download_track_inner(
    db: &Db,
    path: &str,
    track: &LibraryTrack,
    quality: Option<TrackAudioQuality>,
    source: Option<DownloadApiSource>,
    start: Option<u64>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadTrackInnerError> {
    log::debug!(
        "Starting download for track={track:?} quality={quality:?} source={source:?} path={path} start={start:?}"
    );

    let download_source = if let Some(source) = source {
        source
    } else if track.qobuz_id.is_some() {
        log::debug!("Falling back to Qobuz DownloadApiSource");
        DownloadApiSource::Qobuz
    } else if track.tidal_id.is_some() {
        log::debug!("Falling back to Tidal DownloadApiSource");
        DownloadApiSource::Tidal
    } else {
        return Err(DownloadTrackInnerError::InvalidSource);
    };

    let quality = quality.or(Some(TrackAudioQuality::FlacHighestRes));

    let req = get_track_source(track.id, db, quality, Some(download_source.into()));

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
                        || error.is_connect()
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

    let bytes = get_track_bytes(
        db,
        track.id as u64,
        source,
        AudioFormat::Source,
        false,
        start,
        None,
    )
    .await?;

    let track_path = PathBuf::from_str(path).unwrap();

    tokio::fs::create_dir_all(&track_path.parent().unwrap()).await?;

    if Path::exists(&track_path) {
        log::debug!("Track already downloaded");
        return Ok(());
    }

    let track_path = &track_path.to_str().unwrap().to_string();

    log::debug!("Downloading track to track_path={track_path:?}");

    {
        let mut reader = bytes.stream;

        if let Some(timeout_duration) = timeout_duration {
            reader = reader.with_timeout(timeout_duration);
        }

        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(!start.is_some_and(|start| start > 0))
            .open(track_path)
            .await?;

        let mut writer = BufWriter::new(file);

        if let Some(start) = start {
            writer.seek(std::io::SeekFrom::Start(start)).await?;
        }

        let mut total = start.unwrap_or(0) as usize;

        while let Some(data) = reader.next().await {
            match data {
                Ok(data) => {
                    total += data.len();
                    log::debug!(
                        "Writing bytes to '{track_path}': {} ({total} total)",
                        data.len()
                    );
                    writer.write(&data).await?;
                }
                Err(err) => {
                    if err.kind() == std::io::ErrorKind::TimedOut {
                        return Err(DownloadTrackInnerError::Timeout(Some(total as u64)));
                    }

                    return Err(DownloadTrackInnerError::IO(err));
                }
            }
        }

        writer.flush().await?;
    }

    log::debug!("Finished downloading track to track_path={track_path:?}");

    tag_track_file(track_path, track).await?;

    log::debug!("Completed track download for track_path={track_path:?}");

    Ok(())
}

#[derive(Debug, Error)]
pub enum TagTrackFileError {
    #[error(transparent)]
    Tag(#[from] audiotags::Error),
}

pub async fn tag_track_file(
    track_path: &str,
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

    tag.write_to_path(track_path)?;

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
    ArtistCover(#[from] ArtistCoverError),
    #[error(transparent)]
    AlbumCover(#[from] AlbumCoverError),
    #[error("Invalid source")]
    InvalidSource,
    #[error("Not found")]
    NotFound,
}

pub async fn download_album_id(
    db: &Db,
    path: &str,
    album_id: u64,
    try_download_album_cover: bool,
    try_download_artist_cover: bool,
    quality: Option<TrackAudioQuality>,
    source: Option<DownloadApiSource>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadAlbumError> {
    log::debug!("Starting download for album_id={album_id} quality={quality:?} source={source:?} path={path}");

    let tracks = get_album_tracks(&db.library.lock().as_ref().unwrap().inner, album_id as i32)?;

    let tracks = if let Some(source) = source {
        let track_source = source.into();

        tracks
            .into_iter()
            .filter(|track| track.source == track_source)
            .collect()
    } else {
        tracks
    };

    for track in tracks.iter() {
        download_track(db, path, track, quality, source, None, timeout_duration).await?
    }

    log::debug!("Completed album download for {} tracks", tracks.len());

    if try_download_album_cover {
        download_album_cover(db, path, album_id).await?;
    }

    if try_download_artist_cover {
        download_artist_cover(db, path, album_id).await?;
    }

    Ok(())
}

pub async fn download_album_cover(
    db: &Db,
    path: &str,
    album_id: u64,
) -> Result<(), DownloadAlbumError> {
    log::debug!("Downloading album cover");

    let path_buf = PathBuf::from_str(path).unwrap();

    tokio::fs::create_dir_all(&path_buf.parent().unwrap()).await?;

    let cover_path = path_buf.join("cover.jpg");

    if Path::exists(&cover_path) {
        log::debug!("Album cover already downloaded");
        return Ok(());
    }

    let bytes = match get_library_album_cover_bytes(album_id as i32, db).await {
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

    log::debug!("Saving album cover to {cover_path:?}");

    save_bytes_stream_to_file(bytes, &cover_path).await?;

    log::debug!("Completed album cover download");

    Ok(())
}

pub async fn download_artist_cover(
    db: &Db,
    path: &str,
    album_id: u64,
) -> Result<(), DownloadAlbumError> {
    log::debug!("Downloading artist cover");

    let path_buf = PathBuf::from_str(path).unwrap();

    tokio::fs::create_dir_all(&path_buf.parent().unwrap()).await?;

    let cover_path = path_buf.join("artist.jpg");

    if Path::exists(&cover_path) {
        log::debug!("Artist cover already downloaded");
        return Ok(());
    }

    let artist =
        get_artist_by_album_id(&db.library.lock().as_ref().unwrap().inner, album_id as i32)?
            .ok_or(DownloadAlbumError::NotFound)?;

    let bytes = match get_library_artist_cover_bytes(artist.id, db).await {
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

    log::debug!("Saving artist cover to {cover_path:?}");

    save_bytes_stream_to_file(bytes, &cover_path).await?;

    log::debug!("Completed artist cover download");

    Ok(())
}

#[async_trait]
pub trait Downloader /*: Clone + Send + Sync*/ {
    async fn download_track_id(
        &self,
        db: &Db,
        path: &str,
        track_id: u64,
        quality: Option<TrackAudioQuality>,
        source: Option<DownloadApiSource>,
        timeout_duration: Option<Duration>,
    ) -> Result<(), DownloadTrackError>;

    async fn download_album_cover(
        &self,
        db: &Db,
        path: &str,
        album_id: u64,
    ) -> Result<(), DownloadAlbumError>;

    async fn download_artist_cover(
        &self,
        db: &Db,
        path: &str,
        album_id: u64,
    ) -> Result<(), DownloadAlbumError>;
}

pub struct MoosicboxDownloader {}

#[async_trait]
impl Downloader for MoosicboxDownloader {
    async fn download_track_id(
        &self,
        db: &Db,
        path: &str,
        track_id: u64,
        quality: Option<TrackAudioQuality>,
        source: Option<DownloadApiSource>,
        timeout_duration: Option<Duration>,
    ) -> Result<(), DownloadTrackError> {
        download_track_id(db, path, track_id, quality, source, timeout_duration).await
    }

    async fn download_album_cover(
        &self,
        db: &Db,
        path: &str,
        album_id: u64,
    ) -> Result<(), DownloadAlbumError> {
        download_album_cover(db, path, album_id).await
    }

    async fn download_artist_cover(
        &self,
        db: &Db,
        path: &str,
        album_id: u64,
    ) -> Result<(), DownloadAlbumError> {
        download_artist_cover(db, path, album_id).await
    }
}
