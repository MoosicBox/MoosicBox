use crate::{
    app::AppState,
    cache::{get_or_set_to_cache, CacheItemType, CacheRequest},
};
use std::{sync::PoisonError, time::Duration};
use thiserror::Error;

use super::{
    db::{self, DbError},
    models::{Album, Artist},
};

#[derive(Debug, Error)]
pub enum GetArtistError {
    #[error("Album not found with ID {artist_id:?}")]
    ArtistNotFound { artist_id: i32 },
    #[error("Unknown source: {artist_source:?}")]
    UnknownSource { artist_source: String },
    #[error("Poison error")]
    PoisonError,
    #[error(transparent)]
    SqliteError(#[from] rusqlite::Error),
    #[error(transparent)]
    DbError(#[from] db::DbError),
    #[error("No DB set")]
    NoDb,
}

impl<T> From<PoisonError<T>> for GetArtistError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonError
    }
}

pub async fn get_artist(artist_id: i32, data: &AppState) -> Result<Artist, GetArtistError> {
    let request = CacheRequest {
        key: format!("artist|{artist_id}"),
        expiration: Duration::from_secs(5 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        let library = data
            .db
            .as_ref()
            .ok_or(GetArtistError::NoDb)?
            .library
            .lock()?;

        match db::get_artist(&library.inner, artist_id) {
            Ok(artist) => {
                if artist.is_none() {
                    return Err(GetArtistError::ArtistNotFound { artist_id });
                }

                let artist = artist.unwrap();

                Ok(CacheItemType::Artist(artist))
            }
            Err(err) => Err(GetArtistError::DbError(err)),
        }
    })
    .await?
    .into_artist()
    .unwrap())
}

#[derive(Debug, Error)]
pub enum GetAlbumError {
    #[error("Album not found with ID {album_id:?}")]
    AlbumNotFound { album_id: i32 },
    #[error("Too many albums found with ID {album_id:?}")]
    TooManyAlbumsFound { album_id: i32 },
    #[error("Unknown source: {album_source:?}")]
    UnknownSource { album_source: String },
    #[error("Poison error")]
    PoisonError,
    #[error(transparent)]
    SqliteError(#[from] rusqlite::Error),
    #[error(transparent)]
    DbError(#[from] db::DbError),
    #[error("No DB set")]
    NoDb,
}

impl<T> From<PoisonError<T>> for GetAlbumError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonError
    }
}

pub async fn get_album(album_id: i32, data: &AppState) -> Result<Album, GetAlbumError> {
    let request = CacheRequest {
        key: format!("album|{album_id}"),
        expiration: Duration::from_secs(5 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        let library = data
            .db
            .as_ref()
            .ok_or(GetAlbumError::NoDb)?
            .library
            .lock()?;

        match db::get_album(&library.inner, album_id) {
            Ok(album) => {
                if album.is_none() {
                    return Err(GetAlbumError::AlbumNotFound { album_id });
                }

                let album = album.unwrap();

                Ok(CacheItemType::Album(album))
            }
            Err(err) => Err(GetAlbumError::DbError(err)),
        }
    })
    .await?
    .into_album()
    .unwrap())
}

#[derive(Debug, Error)]
pub enum GetAlbumsError {
    #[error("Poison error")]
    Poison,
    #[error(transparent)]
    Json(#[from] awc::error::JsonPayloadError),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("No DB set")]
    NoDb,
}

impl<T> From<PoisonError<T>> for GetAlbumsError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

pub async fn get_albums(data: &AppState) -> Result<Vec<Album>, GetAlbumsError> {
    let request = CacheRequest {
        key: "sqlite|local_albums".to_string(),
        expiration: Duration::from_secs(5 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        let library = data
            .db
            .as_ref()
            .ok_or(GetAlbumsError::NoDb)?
            .library
            .lock()?;

        Ok::<CacheItemType, GetAlbumsError>(CacheItemType::Albums(super::db::get_albums(
            &library.inner,
        )?))
    })
    .await?
    .into_albums()
    .unwrap())
}

#[derive(Debug, Error)]
pub enum GetArtistAlbumsError {
    #[error("Poison error")]
    Poison,
    #[error(transparent)]
    Json(#[from] awc::error::JsonPayloadError),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Db(#[from] DbError),
    #[error("No DB set")]
    NoDb,
}

impl<T> From<PoisonError<T>> for GetArtistAlbumsError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

pub async fn get_artist_albums(
    artist_id: i32,
    data: &AppState,
) -> Result<Vec<Album>, GetArtistAlbumsError> {
    let request = CacheRequest {
        key: format!("sqlite|local_artist_albums|{artist_id}"),
        expiration: Duration::from_secs(5 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        let library = data
            .db
            .as_ref()
            .ok_or(GetArtistAlbumsError::NoDb)?
            .library
            .lock()?;

        Ok::<CacheItemType, GetArtistAlbumsError>(CacheItemType::ArtistAlbums(
            db::get_artist_albums(&library.inner, artist_id)?,
        ))
    })
    .await?
    .into_artist_albums()
    .unwrap())
}
