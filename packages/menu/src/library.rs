pub mod albums;
pub mod artists;

use albums::propagate_api_sources_from_library_album;
use moosicbox_date_utils::chrono;
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_library::{
    cache::{CacheItemType, CacheRequest, get_or_set_to_cache},
    db,
    models::LibraryAlbum,
};
use moosicbox_music_api::{MusicApi, SourceToMusicApi as _};
use moosicbox_music_models::{Album, ApiSource, Artist, id::Id};
use std::{
    sync::{Arc, PoisonError},
    time::Duration,
};
use switchy_database::profiles::LibraryDatabase;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GetArtistError {
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
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
    #[error(transparent)]
    MusicApi(#[from] moosicbox_music_api::Error),
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
    profile: &str,
    album_id: &Id,
    source: &ApiSource,
) -> Result<Option<Album>, GetAlbumError> {
    let mut album = if source.is_library() {
        let albums = get_albums(db).await?;
        albums
            .iter()
            .find(|album| &Into::<Id>::into(album.id) == album_id)
            .cloned()
            .map(TryInto::try_into)
            .transpose()?
    } else {
        let music_api = moosicbox_music_api::profiles::PROFILES
            .get(profile)
            .ok_or_else(|| GetAlbumError::UnknownSource {
                album_source: source.to_string(),
            })?
            .get(source)
            .ok_or_else(|| GetAlbumError::UnknownSource {
                album_source: source.to_string(),
            })?;

        music_api.album(album_id).await?
    };

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
    source: &ApiSource,
) -> Result<Option<LibraryAlbum>, GetAlbumError> {
    let albums = get_albums(db).await?;

    Ok(if source.is_library() {
        albums
            .iter()
            .find(|album| &Into::<Id>::into(album.id) == album_id)
            .cloned()
    } else {
        albums
            .iter()
            .find(|album| {
                album
                    .album_sources
                    .iter()
                    .any(|x| &x.source == source && &x.id == album_id)
            })
            .cloned()
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

    let start = switchy_time::now();
    let albums = get_or_set_to_cache(request, || async {
        Ok::<CacheItemType, GetAlbumsError>(CacheItemType::Albums(Arc::new(
            db::get_albums(db).await?,
        )))
    })
    .await?
    .into_albums()
    .unwrap();
    let elapsed = switchy_time::now()
        .duration_since(start)
        .unwrap()
        .as_millis();
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
