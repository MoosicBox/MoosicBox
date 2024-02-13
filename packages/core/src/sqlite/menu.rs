use crate::{
    app::AppState,
    cache::{get_or_set_to_cache, CacheItemType, CacheRequest},
};
use actix_web::error::{ErrorInternalServerError, ErrorNotFound};
use moosicbox_database::Database;
use std::{
    sync::{Arc, PoisonError},
    time::{Duration, SystemTime},
};
use thiserror::Error;

use super::{
    db::{self, DbError},
    models::{LibraryAlbum, LibraryArtist},
};

#[derive(Debug, Error)]
pub enum GetArtistError {
    #[error("Artist not found with ID {0}")]
    ArtistNotFound(u64),
    #[error("Artist not found with album ID {0}")]
    AlbumArtistNotFound(u64),
    #[error("Unknown source: {artist_source:?}")]
    UnknownSource { artist_source: String },
    #[error("Poison error")]
    PoisonError,
    #[error(transparent)]
    SqliteError(#[from] rusqlite::Error),
    #[error(transparent)]
    DbError(#[from] db::DbError),
    #[error("Invalid request")]
    InvalidRequest,
}

impl<T> From<PoisonError<T>> for GetArtistError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonError
    }
}

pub async fn get_artist(
    artist_id: Option<u64>,
    tidal_artist_id: Option<u64>,
    qobuz_artist_id: Option<u64>,
    album_id: Option<u64>,
    tidal_album_id: Option<u64>,
    qobuz_album_id: Option<u64>,
    data: &AppState,
) -> Result<Arc<LibraryArtist>, GetArtistError> {
    let request = CacheRequest {
        key: &format!("artist|{artist_id:?}|{tidal_artist_id:?}|{qobuz_artist_id:?}|{album_id:?}|{tidal_album_id:?}|{qobuz_album_id:?}"),
        expiration: Duration::from_secs(5 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        if let Some(artist_id) = artist_id {
            match db::get_artist(&data.database, "id", artist_id as i32).await {
                Ok(artist) => {
                    if artist.is_none() {
                        return Err(GetArtistError::ArtistNotFound(artist_id));
                    }

                    let artist = artist.unwrap();

                    Ok(CacheItemType::Artist(Arc::new(artist)))
                }
                Err(err) => Err(GetArtistError::DbError(err)),
            }
        } else if let Some(tidal_artist_id) = tidal_artist_id {
            match db::get_artist(&data.database, "tidal_id", tidal_artist_id as i32).await {
                Ok(artist) => {
                    if artist.is_none() {
                        return Err(GetArtistError::ArtistNotFound(tidal_artist_id));
                    }

                    let artist = artist.unwrap();

                    Ok(CacheItemType::Artist(Arc::new(artist)))
                }
                Err(err) => Err(GetArtistError::DbError(err)),
            }
        } else if let Some(qobuz_artist_id) = qobuz_artist_id {
            match db::get_artist(&data.database, "qobuz_id", qobuz_artist_id as i32).await {
                Ok(artist) => {
                    if artist.is_none() {
                        return Err(GetArtistError::ArtistNotFound(qobuz_artist_id));
                    }

                    let artist = artist.unwrap();

                    Ok(CacheItemType::Artist(Arc::new(artist)))
                }
                Err(err) => Err(GetArtistError::DbError(err)),
            }
        } else if let Some(album_id) = album_id {
            match db::get_album_artist(&data.database, album_id as i32).await {
                Ok(artist) => {
                    if artist.is_none() {
                        return Err(GetArtistError::AlbumArtistNotFound(album_id));
                    }

                    let artist = artist.unwrap();

                    Ok(CacheItemType::Artist(Arc::new(artist)))
                }
                Err(err) => Err(GetArtistError::DbError(err)),
            }
        } else if let Some(tidal_album_id) = tidal_album_id {
            match db::get_tidal_album_artist(&data.database, tidal_album_id as i32).await {
                Ok(artist) => {
                    if artist.is_none() {
                        return Err(GetArtistError::AlbumArtistNotFound(tidal_album_id));
                    }

                    let artist = artist.unwrap();

                    Ok(CacheItemType::Artist(Arc::new(artist)))
                }
                Err(err) => Err(GetArtistError::DbError(err)),
            }
        } else if let Some(qobuz_album_id) = qobuz_album_id {
            match db::get_qobuz_album_artist(&data.database, qobuz_album_id as i32).await {
                Ok(artist) => {
                    if artist.is_none() {
                        return Err(GetArtistError::AlbumArtistNotFound(qobuz_album_id));
                    }

                    let artist = artist.unwrap();

                    Ok(CacheItemType::Artist(Arc::new(artist)))
                }
                Err(err) => Err(GetArtistError::DbError(err)),
            }
        } else {
            Err(GetArtistError::InvalidRequest)
        }
    })
    .await?
    .into_artist()
    .unwrap())
}

#[derive(Debug, Error)]
pub enum GetAlbumError {
    #[error("Album not found with ID {0}")]
    AlbumNotFound(String),
    #[error("Too many albums found with ID {album_id:?}")]
    TooManyAlbumsFound { album_id: i32 },
    #[error("Unknown source: {album_source:?}")]
    UnknownSource { album_source: String },
    #[error("Poison error")]
    PoisonError,
    #[error(transparent)]
    GetAlbums(#[from] GetAlbumsError),
    #[error(transparent)]
    SqliteError(#[from] rusqlite::Error),
    #[error(transparent)]
    DbError(#[from] db::DbError),
    #[error("Invalid request")]
    InvalidRequest,
}

impl<T> From<PoisonError<T>> for GetAlbumError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonError
    }
}

impl From<GetAlbumError> for actix_web::Error {
    fn from(err: GetAlbumError) -> Self {
        log::error!("{err:?}");
        if let GetAlbumError::AlbumNotFound(_) = err {
            return ErrorNotFound("Album not found");
        }

        ErrorInternalServerError(err.to_string())
    }
}

pub async fn get_album(
    db: &Box<dyn Database>,
    album_id: Option<u64>,
    tidal_album_id: Option<u64>,
    qobuz_album_id: Option<String>,
) -> Result<Option<LibraryAlbum>, GetAlbumError> {
    /*let request = CacheRequest {
        key: format!("album|{album_id:?}|{tidal_album_id:?}|{qobuz_album_id:?}"),
        expiration: Duration::from_secs(5 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        if let Some(album_id) = album_id {
            match db::get_album(&db, "id", album_id as i32).await {
                Ok(album) => {
                    if album.is_none() {
                        return Err(GetAlbumError::AlbumNotFound(album_id.to_string()));
                    }

                    let album = album.unwrap();

                    Ok(CacheItemType::Album(album))
                }
                Err(err) => Err(GetAlbumError::DbError(err)),
            }
        } else if let Some(tidal_album_id) = tidal_album_id {
            match db::get_album(&db, "tidal_id", tidal_album_id as i32).await {
                Ok(album) => {
                    if album.is_none() {
                        return Err(GetAlbumError::AlbumNotFound(tidal_album_id.to_string()));
                    }

                    let album = album.unwrap();

                    Ok(CacheItemType::Album(album))
                }
                Err(err) => Err(GetAlbumError::DbError(err)),
            }
        } else if let Some(qobuz_album_id) = qobuz_album_id.clone() {
            match db::get_album(&db, "qobuz_id", qobuz_album_id.clone()).await {
                Ok(album) => {
                    if album.is_none() {
                        return Err(GetAlbumError::AlbumNotFound(qobuz_album_id));
                    }

                    let album = album.unwrap();

                    Ok(CacheItemType::Album(album))
                }
                Err(err) => Err(GetAlbumError::DbError(err)),
            }
        } else {
            Err(GetAlbumError::InvalidRequest)
        }
    })
    .await?
    .into_album()
    .unwrap())*/
    let albums = get_albums(db).await?;

    Ok(if let Some(album_id) = album_id {
        let album = albums.iter().find(|album| album.id as u64 == album_id);

        if album.is_none() {
            return Err(GetAlbumError::AlbumNotFound(album_id.to_string()));
        }

        let album = album.unwrap().clone();

        Some(album)
    } else if let Some(tidal_album_id) = tidal_album_id {
        let album = albums
            .iter()
            .find(|album| album.tidal_id.is_some_and(|id| id == tidal_album_id));

        if album.is_none() {
            return Err(GetAlbumError::AlbumNotFound(tidal_album_id.to_string()));
        }

        let album = album.unwrap().clone();

        Some(album)
    } else if let Some(qobuz_album_id) = qobuz_album_id {
        let album = albums.iter().find(|album| {
            album
                .qobuz_id
                .as_ref()
                .is_some_and(|id| id == &qobuz_album_id)
        });

        if album.is_none() {
            return Err(GetAlbumError::AlbumNotFound(qobuz_album_id));
        }

        let album = album.unwrap().clone();

        Some(album)
    } else {
        None
    })
}

#[derive(Debug, Error)]
pub enum GetAlbumsError {
    #[error("Poison error")]
    Poison,
    #[error(transparent)]
    Json(#[from] awc::error::JsonPayloadError),
    #[error(transparent)]
    Db(#[from] DbError),
}

impl<T> From<PoisonError<T>> for GetAlbumsError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

impl From<GetAlbumsError> for actix_web::Error {
    fn from(err: GetAlbumsError) -> Self {
        log::error!("{err:?}");
        ErrorInternalServerError(err.to_string())
    }
}

pub async fn get_albums(db: &Box<dyn Database>) -> Result<Arc<Vec<LibraryAlbum>>, GetAlbumsError> {
    let request = CacheRequest {
        key: "sqlite|local_albums",
        expiration: Duration::from_secs(5 * 60),
    };

    let start = SystemTime::now();
    let albums = get_or_set_to_cache(request, || async {
        Ok::<CacheItemType, GetAlbumsError>(CacheItemType::Albums(Arc::new(
            super::db::get_albums(db).await?,
        )))
    })
    .await?
    .into_albums()
    .unwrap();
    let elapsed = SystemTime::now().duration_since(start).unwrap().as_millis();
    log::debug!("Took {elapsed}ms to get albums");

    Ok(albums)
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
}

impl<T> From<PoisonError<T>> for GetArtistAlbumsError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

pub async fn get_artist_albums(
    artist_id: i32,
    data: &AppState,
) -> Result<Arc<Vec<LibraryAlbum>>, GetArtistAlbumsError> {
    let request = CacheRequest {
        key: &format!("sqlite|local_artist_albums|{artist_id}"),
        expiration: Duration::from_secs(5 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        Ok::<CacheItemType, GetArtistAlbumsError>(CacheItemType::ArtistAlbums(Arc::new(
            db::get_artist_albums(&data.database, artist_id).await?,
        )))
    })
    .await?
    .into_artist_albums()
    .unwrap())
}
