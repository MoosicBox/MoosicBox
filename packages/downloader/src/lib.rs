#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    error::Error,
    path::{Path, PathBuf},
    str::FromStr,
    time::Duration,
};

use async_recursion::async_recursion;
use audiotags::Tag;
use futures::StreamExt;
use id3::Timestamp;
use moosicbox_core::{
    app::Db,
    sqlite::{
        db::{get_album, get_album_tracks, get_artist_by_album_id, get_track},
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
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;
use tokio::{
    io::{AsyncSeekExt, AsyncWriteExt, BufWriter},
    select,
};

#[cfg(feature = "api")]
pub mod api;

pub mod db;

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum DownloadApiSource {
    Tidal,
    Qobuz,
}

impl From<DownloadApiSource> for TrackApiSource {
    fn from(value: DownloadApiSource) -> Self {
        match value {
            DownloadApiSource::Tidal => TrackApiSource::Tidal,
            DownloadApiSource::Qobuz => TrackApiSource::Qobuz,
        }
    }
}

impl From<TrackApiSource> for DownloadApiSource {
    fn from(value: TrackApiSource) -> Self {
        match value {
            TrackApiSource::Tidal => DownloadApiSource::Tidal,
            TrackApiSource::Qobuz => DownloadApiSource::Qobuz,
            _ => panic!("Invalid TrackApiSource"),
        }
    }
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
    Db(#[from] moosicbox_core::sqlite::db::DbError),
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
    Db(#[from] moosicbox_core::sqlite::db::DbError),
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

    let path_buf = PathBuf::from_str(path)
        .unwrap()
        .join(&sanitize_filename(&track.artist))
        .join(&sanitize_filename(&track.album));

    tokio::fs::create_dir_all(&path_buf).await?;

    let track_path = path_buf.join(&get_filename_for_track(&track));

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
    Db(#[from] moosicbox_core::sqlite::db::DbError),
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

async fn download_album_cover(
    db: &Db,
    path: &str,
    album_id: u64,
) -> Result<(), DownloadAlbumError> {
    log::debug!("Downloading album cover");

    let album = get_album(&db.library.lock().as_ref().unwrap().inner, album_id as i32)?
        .ok_or(DownloadAlbumError::NotFound)?;

    let path_buf = PathBuf::from_str(path)
        .unwrap()
        .join(&sanitize_filename(&album.artist))
        .join(&sanitize_filename(&album.title));

    tokio::fs::create_dir_all(&path_buf).await?;

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

async fn download_artist_cover(
    db: &Db,
    path: &str,
    album_id: u64,
) -> Result<(), DownloadAlbumError> {
    log::debug!("Downloading artist cover");

    let artist =
        get_artist_by_album_id(&db.library.lock().as_ref().unwrap().inner, album_id as i32)?
            .ok_or(DownloadAlbumError::NotFound)?;

    let path_buf = PathBuf::from_str(path)
        .unwrap()
        .join(&sanitize_filename(&artist.title));

    tokio::fs::create_dir_all(&path_buf).await?;

    let cover_path = path_buf.join("artist.jpg");

    if Path::exists(&cover_path) {
        log::debug!("Artist cover already downloaded");
        return Ok(());
    }

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
