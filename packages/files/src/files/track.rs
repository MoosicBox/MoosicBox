use std::env;

use log::{debug, error, trace};
use moosicbox_core::{
    app::Db,
    sqlite::db::{get_track, DbError},
};
use regex::{Captures, Regex};
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

    trace!("Got track {track:?}");

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
