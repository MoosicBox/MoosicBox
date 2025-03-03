pub mod albums;
pub mod artists;

use albums::propagate_api_sources_from_library_album;
use moosicbox_database::profiles::LibraryDatabase;
use moosicbox_date_utils::chrono;
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_library::{
    cache::{CacheItemType, CacheRequest, get_or_set_to_cache},
    db,
    models::LibraryAlbum,
};
use moosicbox_music_api::{ArtistError, MusicApi};
use moosicbox_music_models::{Album, ApiSource, Artist, id::Id};
use std::{
    sync::{Arc, PoisonError},
    time::{Duration, SystemTime},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GetArtistError {
    #[error(transparent)]
    Artist(#[from] ArtistError),
    #[error("Invalid request")]
    InvalidRequest,
}

/// # Errors
///
/// * If the `MusicApi` fails to get the artist
#[allow(clippy::too_many_arguments)]
pub async fn get_artist(
    api: &dyn MusicApi,
    artist_id: Option<&Id>,
    album_id: Option<&Id>,
) -> Result<Option<Artist>, GetArtistError> {
    if let Some(artist_id) = artist_id {
        Ok(api.artist(artist_id).await?)
    } else if let Some(album_id) = album_id {
        Ok(api.album_artist(album_id).await?)
    } else {
        Err(GetArtistError::InvalidRequest)
    }
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
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error("Invalid request")]
    InvalidRequest,
    #[error(transparent)]
    ChronoParse(#[from] chrono::ParseError),
}

impl<T> From<PoisonError<T>> for GetAlbumError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::PoisonError
    }
}

/// # Errors
///
/// * If the `LibraryMusicApi` fails to get the album from the `ApiSource`
pub async fn get_album_from_source(
    db: &LibraryDatabase,
    album_id: &Id,
    source: ApiSource,
) -> Result<Option<Album>, GetAlbumError> {
    let mut album = match source {
        ApiSource::Library => {
            let albums = get_albums(db).await?;
            albums
                .iter()
                .find(|album| &Into::<Id>::into(album.id) == album_id)
                .cloned()
                .map(TryInto::try_into)
        }
        #[cfg(feature = "tidal")]
        ApiSource::Tidal => moosicbox_tidal::album(db, album_id, None, None, None, None)
            .await
            .ok()
            .map(TryInto::try_into),
        #[cfg(feature = "qobuz")]
        ApiSource::Qobuz => moosicbox_qobuz::album(db, album_id, None, None)
            .await
            .ok()
            .map(TryInto::try_into),
        #[cfg(feature = "yt")]
        ApiSource::Yt => moosicbox_yt::album(db, album_id, None, None, None, None)
            .await
            .ok()
            .map(TryInto::try_into),
    }
    .transpose()?;

    if let Some(album) = &mut album {
        let library_albums = get_albums(db).await?;

        propagate_api_sources_from_library_album(source, album, &library_albums);
    }

    Ok(album)
}

/// # Errors
///
/// * If the `LibraryMusicApi` fails to get the `LibraryAlbum` from the `ApiSource`
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
                    .album_sources
                    .iter()
                    .any(|x| x.source == ApiSource::Tidal && &x.id == album_id)
            })
            .cloned(),
        #[cfg(feature = "qobuz")]
        ApiSource::Qobuz => albums
            .iter()
            .find(|album| {
                album
                    .album_sources
                    .iter()
                    .any(|x| x.source == ApiSource::Qobuz && &x.id == album_id)
            })
            .cloned(),
        #[cfg(feature = "yt")]
        ApiSource::Yt => albums
            .iter()
            .find(|album| {
                album
                    .album_sources
                    .iter()
                    .any(|x| x.source == ApiSource::Yt && &x.id == album_id)
            })
            .cloned(),
    })
}

#[derive(Debug, Error)]
pub enum GetAlbumsError {
    #[error("Poison error")]
    Poison,
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

impl<T> From<PoisonError<T>> for GetAlbumsError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

/// # Panics
///
/// * If fails to fetch the `LibraryAlbum`s from the cache
///
/// # Errors
///
/// * If fails to get the `LibraryAlbum`s from the cache or database
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
    DatabaseFetch(#[from] DatabaseFetchError),
}

impl<T> From<PoisonError<T>> for GetArtistAlbumsError {
    fn from(_err: PoisonError<T>) -> Self {
        Self::Poison
    }
}

/// # Panics
///
/// * If fails to fetch the artist's `LibraryAlbum`s from the cache
///
/// # Errors
///
/// * If fails to get the artist's `LibraryAlbum`s from the cache or database
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
