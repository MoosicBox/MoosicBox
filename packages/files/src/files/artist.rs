use std::env;

use moosicbox_core::{
    app::Db,
    sqlite::db::{get_artist, DbError},
};
use regex::{Captures, Regex};
use thiserror::Error;

pub enum ArtistCoverSource {
    LocalFilePath(String),
}

#[derive(Debug, Error)]
pub enum ArtistCoverError {
    #[error("Artist cover not found for album: {0}")]
    NotFound(i32),
    #[error("Invalid source")]
    InvalidSource,
    #[error(transparent)]
    Db(#[from] DbError),
}

pub async fn get_artist_cover(
    artist_id: i32,
    db: Db,
) -> Result<ArtistCoverSource, ArtistCoverError> {
    let artist = {
        let library = db.library.lock().unwrap();
        get_artist(&library.inner, artist_id)?
    };

    if artist.is_none() {
        return Err(ArtistCoverError::NotFound(artist_id));
    }

    let artist = artist.unwrap();

    if artist.cover.is_none() {
        return Err(ArtistCoverError::NotFound(artist_id));
    }

    match artist.cover {
        Some(cover) => match env::consts::OS {
            "windows" => Ok(ArtistCoverSource::LocalFilePath(
                Regex::new(r"/mnt/(\w+)")
                    .unwrap()
                    .replace(&cover, |caps: &Captures| {
                        format!("{}:", caps[1].to_uppercase())
                    })
                    .replace('/', "\\"),
            )),
            _ => Ok(ArtistCoverSource::LocalFilePath(cover.to_string())),
        },
        None => Err(ArtistCoverError::NotFound(artist_id)),
    }
}
