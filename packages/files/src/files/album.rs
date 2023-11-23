use std::env;

use moosicbox_core::{
    app::Db,
    sqlite::db::{get_album, DbError},
};
use regex::{Captures, Regex};
use thiserror::Error;

pub enum AlbumCoverSource {
    LocalFilePath(String),
}

#[derive(Debug, Error)]
pub enum AlbumCoverError {
    #[error("Album cover not found for album: {0}")]
    NotFound(i32),
    #[error("Invalid source")]
    InvalidSource,
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("Failed to read file with path: {0} ({1})")]
    File(String, String),
}

pub async fn get_album_cover(album_id: i32, db: Db) -> Result<AlbumCoverSource, AlbumCoverError> {
    let album = {
        let library = db.library.lock().unwrap();
        get_album(&library, album_id)?
    };

    if album.is_none() {
        return Err(AlbumCoverError::NotFound(album_id));
    }

    let album = album.unwrap();

    if album.artwork.is_none() {
        return Err(AlbumCoverError::NotFound(album_id));
    }
    if album.directory.is_none() {
        return Err(AlbumCoverError::InvalidSource);
    }

    let directory = album.directory.unwrap();

    match album.artwork {
        Some(cover) => {
            let file = match env::consts::OS {
                "windows" => Regex::new(r"/mnt/(\w+)")
                    .unwrap()
                    .replace(&cover, |caps: &Captures| {
                        format!("{}:", caps[1].to_uppercase())
                    })
                    .replace('/', "\\"),
                _ => cover.to_string(),
            };

            Ok(AlbumCoverSource::LocalFilePath(
                std::path::PathBuf::from(directory)
                    .join(file)
                    .to_str()
                    .unwrap()
                    .to_string(),
            ))
        }
        None => Err(AlbumCoverError::NotFound(album_id)),
    }
}
