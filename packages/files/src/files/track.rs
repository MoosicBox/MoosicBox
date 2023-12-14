use std::{env, fs::File};

use log::{debug, error, trace};
use moosicbox_core::{
    app::{Db, DbConnection},
    sqlite::{
        db::{get_track, get_track_size, get_tracks, set_track_size, DbError},
        models::Track,
    },
    types::{AudioFormat, PlaybackQuality},
};
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Clone)]
pub enum TrackSource {
    LocalFilePath(String),
}

#[derive(Debug, Error)]
pub enum TrackSourceError {
    #[error("Track not found: {0}")]
    NotFound(i32),
    #[error("Invalid source")]
    InvalidSource,
    #[error(transparent)]
    Db(#[from] DbError),
}

pub async fn get_track_source(track_id: i32, db: Db) -> Result<TrackSource, TrackSourceError> {
    debug!("Getting track audio file {track_id}");

    let track = {
        let library = db.library.lock().unwrap();
        get_track(&library.inner, track_id)?
    };

    debug!("Got track {track:?}");

    if track.is_none() {
        return Err(TrackSourceError::NotFound(track_id));
    }

    let track = track.unwrap();

    match track.file {
        Some(file) => match env::consts::OS {
            "windows" => Ok(TrackSource::LocalFilePath(
                Regex::new(r"/mnt/(\w+)")
                    .unwrap()
                    .replace(&file, |caps: &Captures| {
                        format!("{}:", caps[1].to_uppercase())
                    })
                    .replace('/', "\\"),
            )),
            _ => Ok(TrackSource::LocalFilePath(file)),
        },
        None => Err(TrackSourceError::InvalidSource),
    }
}

#[derive(Debug, Error)]
pub enum TrackInfoError {
    #[error("Track not found: {0}")]
    NotFound(i32),
    #[error(transparent)]
    Db(#[from] DbError),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackInfo {
    pub id: i32,
    pub number: i32,
    pub title: String,
    pub duration: f64,
    pub album: String,
    pub album_id: i32,
    pub date_released: Option<String>,
    pub artist: String,
    pub artist_id: i32,
    pub blur: bool,
}

impl From<Track> for TrackInfo {
    fn from(value: Track) -> Self {
        TrackInfo {
            id: value.id,
            number: value.number,
            title: value.title,
            duration: value.duration,
            album: value.album,
            album_id: value.album_id,
            date_released: value.date_released,
            artist: value.artist,
            artist_id: value.artist_id,
            blur: value.blur,
        }
    }
}

pub async fn get_tracks_info(
    track_ids: Vec<i32>,
    db: Db,
) -> Result<Vec<TrackInfo>, TrackInfoError> {
    debug!("Getting tracks info {track_ids:?}");

    let tracks = {
        let library = db.library.lock().unwrap();
        get_tracks(&library.inner, &track_ids)?
    };

    trace!("Got tracks {tracks:?}");

    Ok(tracks.into_iter().map(|t| t.into()).collect())
}

pub async fn get_track_info(track_id: i32, db: Db) -> Result<TrackInfo, TrackInfoError> {
    debug!("Getting track info {track_id}");

    let track = {
        let library = db.library.lock().unwrap();
        get_track(&library.inner, track_id)?
    };

    trace!("Got track {track:?}");

    if track.is_none() {
        return Err(TrackInfoError::NotFound(track_id));
    }

    Ok(track.unwrap().into())
}

pub fn get_or_init_track_size(
    track_id: i32,
    source: &TrackSource,
    quality: PlaybackQuality,
    connection: &DbConnection,
) -> Result<u64, TrackInfoError> {
    debug!("Getting track size {track_id}");

    if let Some(size) = get_track_size(&connection.inner, track_id, &quality)? {
        return Ok(size);
    }

    match source {
        TrackSource::LocalFilePath(ref path) => match quality.format {
            #[cfg(feature = "aac")]
            AudioFormat::Aac => {
                let size = moosicbox_symphonia_player::output::encoder::aac::encoder::encode_aac(
                    path.to_string(),
                    std::io::empty(),
                ) as u64;
                set_track_size(&connection.inner, track_id, &quality, size)?;
                Ok(size)
            }
            #[cfg(feature = "mp3")]
            AudioFormat::Mp3 => {
                let size = moosicbox_symphonia_player::output::encoder::mp3::encoder::encode_mp3(
                    path.to_string(),
                    std::io::empty(),
                ) as u64;
                set_track_size(&connection.inner, track_id, &quality, size)?;
                Ok(size)
            }
            #[cfg(feature = "opus")]
            AudioFormat::Opus => {
                let size = moosicbox_symphonia_player::output::encoder::opus::encoder::encode_opus(
                    path.to_string(),
                    std::io::empty(),
                ) as u64;
                set_track_size(&connection.inner, track_id, &quality, size)?;
                Ok(size)
            }
            AudioFormat::Source => {
                let size = { File::open(path).unwrap().metadata().unwrap().len() };
                set_track_size(&connection.inner, track_id, &quality, size)?;
                Ok(size)
            }
        },
    }
}
