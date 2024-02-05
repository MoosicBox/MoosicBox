#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{path::PathBuf, str::FromStr};

use audiotags::Tag;
use futures::StreamExt;
use id3::Timestamp;
use moosicbox_core::{
    app::Db,
    sqlite::{
        db::{get_album_tracks, get_track},
        models::{LibraryTrack, TrackApiSource},
    },
    types::AudioFormat,
};
use moosicbox_files::{
    files::track::{
        get_track_bytes, get_track_source, GetTrackBytesError, TrackAudioQuality, TrackSourceError,
    },
    sanitize_filename,
};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;
use tokio::io::AsyncWriteExt;

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
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Tag(#[from] audiotags::Error),
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
) -> Result<(), DownloadTrackError> {
    log::debug!("Starting download for track_id={track_id} quality={quality:?} source={source:?} path={path}");

    let track = get_track(&db.library.lock().as_ref().unwrap().inner, track_id as i32)?
        .ok_or(DownloadTrackError::NotFound)?;

    download_track(db, path, &track, quality, source).await
}

pub async fn download_track(
    db: &Db,
    path: &str,
    track: &LibraryTrack,
    quality: Option<TrackAudioQuality>,
    source: Option<DownloadApiSource>,
) -> Result<(), DownloadTrackError> {
    log::debug!(
        "Starting download for track={track:?} quality={quality:?} source={source:?} path={path}"
    );

    let source = if let Some(source) = source {
        source
    } else if track.qobuz_id.is_some() {
        log::debug!("Falling back to Qobuz DownloadApiSource");
        DownloadApiSource::Qobuz
    } else if track.tidal_id.is_some() {
        log::debug!("Falling back to Tidal DownloadApiSource");
        DownloadApiSource::Tidal
    } else {
        return Err(DownloadTrackError::InvalidSource);
    };

    let source = get_track_source(track.id, db, quality, Some(source.into())).await?;
    let mut bytes =
        get_track_bytes(db, track.id as u64, source, AudioFormat::Source, false).await?;

    let path_buf = PathBuf::from_str(path)
        .unwrap()
        .join(&sanitize_filename(&track.artist))
        .join(&sanitize_filename(&track.album));

    std::fs::create_dir_all(&path_buf)?;

    let track_path = &path_buf
        .join(&get_filename_for_track(&track))
        .to_str()
        .unwrap()
        .to_string();

    let mut reader = bytes.stream.as_mut();

    log::debug!("Downloading track to track_path={track_path:?}");

    {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(track_path)
            .await?;

        let mut total = 0_usize;

        while let Some(Ok(data)) = reader.next().await {
            total += data.len();
            log::debug!(
                "Writing bytes to '{track_path}': {} ({total} total)",
                data.len()
            );
            file.write(&data).await?;
        }
    }

    log::debug!("Finished downloading track to track_path={track_path:?}");
    log::debug!("Adding tags to track_path={track_path:?}");

    //let mut tag: Box<dyn AudioTag> = Box::new(FlacTag::new());
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

    log::debug!("Completed track download for track_path={track_path:?}");

    Ok(())
}

#[derive(Debug, Error)]
pub enum DownloadAlbumError {
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error(transparent)]
    DownloadTrack(#[from] DownloadTrackError),
    #[error("Not found")]
    NotFound,
}

pub async fn download_album_id(
    db: &Db,
    path: &str,
    album_id: u64,
    quality: Option<TrackAudioQuality>,
    source: Option<DownloadApiSource>,
) -> Result<(), DownloadAlbumError> {
    log::debug!("Starting download for album_id={album_id} quality={quality:?} source={source:?} path={path}");

    let tracks = get_album_tracks(&db.library.lock().as_ref().unwrap().inner, album_id as i32)?;

    for track in tracks.iter() {
        download_track(db, path, track, quality, source).await?
    }

    log::debug!("Completed album download for {} tracks", tracks.len());

    Ok(())
}
