pub mod albums;
pub mod artists;

use moosicbox_core::sqlite::{
    db::DbError,
    models::{Album, ApiSource, Id},
};
use moosicbox_database::profiles::LibraryDatabase;
use moosicbox_library::{
    cache::{get_or_set_to_cache, CacheItemType, CacheRequest},
    db,
    models::{LibraryAlbum, LibraryArtist},
};
use std::{
    sync::{Arc, PoisonError},
    time::{Duration, SystemTime},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GetArtistError {
    #[error("Artist not found with ID {0}")]
    ArtistNotFound(u64),
    #[error("Artist not found with album ID {0}")]
    AlbumArtistNotFound(String),
    #[error("Unknown source: {artist_source:?}")]
    UnknownSource { artist_source: String },
    #[error("Poison error")]
    PoisonError,
    #[error(transparent)]
    DbError(#[from] DbError),
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
    qobuz_album_id: Option<String>,
    db: &LibraryDatabase,
) -> Result<Arc<LibraryArtist>, GetArtistError> {
    let request = CacheRequest {
        key: &format!("artist|{artist_id:?}|{tidal_artist_id:?}|{qobuz_artist_id:?}|{album_id:?}|{tidal_album_id:?}|{qobuz_album_id:?}"),
        expiration: Duration::from_secs(5 * 60),
    };

    Ok(get_or_set_to_cache(request, move || {
        let qobuz_album_id = qobuz_album_id.clone();
        async move {
            if let Some(artist_id) = artist_id {
                match db::get_artist(db, "id", &artist_id.into()).await {
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
                match db::get_artist(db, "tidal_id", &tidal_artist_id.into()).await {
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
                match db::get_artist(db, "qobuz_id", &qobuz_artist_id.into()).await {
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
                match db::get_album_artist(db, album_id).await {
                    Ok(artist) => {
                        if artist.is_none() {
                            return Err(GetArtistError::AlbumArtistNotFound(album_id.to_string()));
                        }

                        let artist = artist.unwrap();

                        Ok(CacheItemType::Artist(Arc::new(artist)))
                    }
                    Err(err) => Err(GetArtistError::DbError(err)),
                }
            } else if let Some(tidal_album_id) = tidal_album_id {
                match db::get_tidal_album_artist(db, tidal_album_id).await {
                    Ok(artist) => {
                        if artist.is_none() {
                            return Err(GetArtistError::AlbumArtistNotFound(
                                tidal_album_id.to_string(),
                            ));
                        }

                        let artist = artist.unwrap();

                        Ok(CacheItemType::Artist(Arc::new(artist)))
                    }
                    Err(err) => Err(GetArtistError::DbError(err)),
                }
            } else if let Some(qobuz_album_id) = qobuz_album_id {
                match db::get_qobuz_album_artist(db, &qobuz_album_id).await {
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
        }
    })
    .await?
    .into_artist()
    .unwrap())
}

#[derive(Debug, Error)]
pub enum GetAlbumError {
    #[error("Too many albums found with ID {album_id:?}")]
    TooManyAlbumsFound { album_id: i32 },
    #[error("Unknown source: {album_source:?}")]
    UnknownSource { album_source: String },
    #[error("Poison error")]
    PoisonError,
    #[error(transparent)]
    GetAlbums(#[from] GetAlbumsError),
    #[error(transparent)]
    DbError(#[from] DbError),
    #[error("Invalid request")]
    InvalidRequest,
}

impl<T> From<PoisonError<T>> for GetAlbumError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonError
    }
}

pub async fn get_album_from_source(
    db: &LibraryDatabase,
    album_id: &Id,
    source: ApiSource,
) -> Result<Option<Album>, GetAlbumError> {
    Ok(match source {
        ApiSource::Library => {
            let albums = get_albums(db).await?;
            albums
                .iter()
                .find(|album| &Into::<Id>::into(album.id) == album_id)
                .cloned()
                .map(Into::into)
        }
        #[cfg(feature = "tidal")]
        ApiSource::Tidal => moosicbox_tidal::album(db, album_id, None, None, None, None)
            .await
            .ok()
            .map(Into::into),
        #[cfg(feature = "qobuz")]
        ApiSource::Qobuz => moosicbox_qobuz::album(db, album_id, None, None)
            .await
            .ok()
            .map(Into::into),
        #[cfg(feature = "yt")]
        ApiSource::Yt => moosicbox_yt::album(db, album_id, None, None, None, None)
            .await
            .ok()
            .map(Into::into),
    })
}

pub async fn get_library_album(
    db: &LibraryDatabase,
    album_id: &Id,
    source: ApiSource,
) -> Result<Option<LibraryAlbum>, GetAlbumError> {
    let albums = get_albums(db).await?;

    Ok(match source {
        ApiSource::Library => albums
            .iter()
            .find(|album| &Into::<Id>::into(album.id) == album_id)
            .cloned(),
        #[cfg(feature = "tidal")]
        ApiSource::Tidal => albums
            .iter()
            .find(|album| {
                album
                    .tidal_id
                    .is_some_and(|id| &Into::<Id>::into(id) == album_id)
            })
            .cloned(),
        #[cfg(feature = "qobuz")]
        ApiSource::Qobuz => albums
            .iter()
            .find(|album| {
                album
                    .qobuz_id
                    .as_ref()
                    .is_some_and(|id| &Into::<Id>::into(id) == album_id)
            })
            .cloned(),
        #[cfg(feature = "yt")]
        ApiSource::Yt => albums
            .iter()
            .find(|album| {
                album
                    .yt_id
                    .as_ref()
                    .is_some_and(|id| &Into::<Id>::into(id) == album_id)
            })
            .cloned(),
    })
}

#[derive(Debug, Error)]
pub enum GetAlbumsError {
    #[error("Poison error")]
    Poison,
    #[error(transparent)]
    Db(#[from] DbError),
}

impl<T> From<PoisonError<T>> for GetAlbumsError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

pub async fn get_albums(db: &LibraryDatabase) -> Result<Arc<Vec<LibraryAlbum>>, GetAlbumsError> {
    let request = CacheRequest {
        key: "sqlite|local_albums",
        expiration: Duration::from_secs(5 * 60),
    };

    let start = SystemTime::now();
    let albums = get_or_set_to_cache(request, || async {
        Ok::<CacheItemType, GetAlbumsError>(CacheItemType::Albums(Arc::new(
            db::get_albums(db).await?,
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
    Db(#[from] DbError),
}

impl<T> From<PoisonError<T>> for GetArtistAlbumsError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

pub async fn get_artist_albums(
    artist_id: &Id,
    db: &LibraryDatabase,
) -> Result<Arc<Vec<LibraryAlbum>>, GetArtistAlbumsError> {
    let request = CacheRequest {
        key: &format!("sqlite|local_artist_albums|{artist_id}"),
        expiration: Duration::from_secs(5 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        Ok::<CacheItemType, GetArtistAlbumsError>(CacheItemType::ArtistAlbums(Arc::new(
            db::get_artist_albums(db, artist_id).await?,
        )))
    })
    .await?
    .into_artist_albums()
    .unwrap())
}
