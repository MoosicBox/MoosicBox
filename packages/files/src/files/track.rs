use std::env;

use log::{debug, error, trace};
use moosicbox_core::{
    app::Db,
    sqlite::{
        db::{get_track, DbError},
        models::Track,
    },
};
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use thiserror::Error;

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
        get_track(&library, track_id)?
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
    pub bytes: u64,
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
            bytes: value.bytes,
            album: value.album,
            album_id: value.album_id,
            date_released: value.date_released,
            artist: value.artist,
            artist_id: value.artist_id,
            blur: value.blur,
        }
    }
}

pub async fn get_track_info(track_id: i32, db: Db) -> Result<TrackInfo, TrackInfoError> {
    debug!("Getting track info {track_id}");

    let track = {
        let library = db.library.lock().unwrap();
        get_track(&library, track_id)?
    };

    trace!("Got track {track:?}");

    if track.is_none() {
        return Err(TrackInfoError::NotFound(track_id));
    }

    Ok(track.unwrap().into())
}
