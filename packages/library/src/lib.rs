//! Music library management functionality for `MoosicBox`.
//!
//! This crate provides core functionality for managing music libraries including:
//!
//! * Accessing and querying artists, albums, and tracks
//! * Managing favorite items (artists, albums, tracks)
//! * Searching library content
//! * Filtering and sorting library items
//! * Caching library data for performance
//! * Database operations for library metadata
//!
//! The library supports both local music collections and integration with external
//! music API sources.
//!
//! # Main Entry Points
//!
//! * [`favorite_artists`], [`favorite_albums`], [`favorite_tracks`] - Retrieve favorite items
//! * [`artist`], [`album`], [`track`] - Get individual items by ID
//! * [`artist_albums`], [`album_tracks`] - Get related items
//! * [`search`] - Search library content
//! * [`reindex_global_search_index`] - Rebuild search index
//!
//! # Examples
//!
//! ```rust,no_run
//! # use moosicbox_library::{favorite_albums, album_tracks};
//! # use moosicbox_music_api_models::AlbumsRequest;
//! # use moosicbox_music_models::id::Id;
//! # use switchy_database::profiles::LibraryDatabase;
//! # async fn example(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
//! // Get favorite albums
//! let albums = favorite_albums(db, &AlbumsRequest::default()).await?;
//!
//! // Get tracks for a specific album
//! let tracks = album_tracks(db, &Id::Number(123), None, None).await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{cmp::Ordering, sync::Arc};

use models::{LibraryAlbum, LibraryArtist, LibraryTrack};

use async_recursion::async_recursion;
use moosicbox_json_utils::database::DatabaseFetchError;
use moosicbox_library_models::LibraryAlbumType;
use moosicbox_menu_models::AlbumVersion;
use moosicbox_music_api_models::{
    AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder, ArtistOrderDirection, TrackOrder,
    TrackOrderDirection, TrackSource, search::api::ApiSearchResultsResponse,
};
use moosicbox_music_models::{Album, AlbumSort, ApiSource, Artist, AudioFormat, Track, id::Id};
use moosicbox_paging::{Page, PagingRequest, PagingResponse, PagingResult};
use moosicbox_search::{
    PopulateIndexError, RecreateIndexError, SearchIndexError, data::AsDataValues as _,
    populate_global_search_index,
};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use switchy_database::profiles::LibraryDatabase;
use thiserror::Error;
use tokio::sync::Mutex;

#[cfg(feature = "api")]
/// HTTP API endpoints for library operations.
pub mod api;

/// Caching functionality for library data.
pub mod cache;
/// Database operations for library metadata.
pub mod db;

/// Library data models re-exported from `moosicbox_library_models`.
pub mod models {
    pub use moosicbox_library_models::*;
}

/// Sort order for artist listings.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryArtistOrder {
    /// Sort by date added.
    Date,
}

/// Sort direction for artist listings.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryArtistOrderDirection {
    /// Ascending order.
    Asc,
    /// Descending order.
    Desc,
}

/// Errors that can occur when retrieving favorite artists.
#[derive(Debug, Error)]
pub enum LibraryFavoriteArtistsError {
    /// No user ID is available for the request.
    #[error("No user ID available")]
    NoUserIdAvailable,
    /// The request failed with an error message.
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Retrieves a paginated list of favorite artists from the library.
///
/// # Errors
///
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn favorite_artists(
    db: &LibraryDatabase,
    offset: Option<u32>,
    limit: Option<u32>,
    #[allow(clippy::used_underscore_binding)] _order: Option<LibraryArtistOrder>,
    #[allow(clippy::used_underscore_binding)] _order_direction: Option<LibraryArtistOrderDirection>,
) -> PagingResult<LibraryArtist, LibraryFavoriteArtistsError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let items = db::get_artists(db).await?;
    log::trace!("Received favorite artists response: {items:?}");

    #[allow(clippy::cast_possible_truncation)]
    let total = items.len() as u32;

    let db = db.to_owned();

    Ok(PagingResponse {
        page: Page::WithTotal {
            items,
            offset,
            limit,
            total,
        },
        fetch: Arc::new(Mutex::new(Box::new(move |offset, limit| {
            let db = db.clone();

            Box::pin(async move {
                favorite_artists(&db, Some(offset), Some(limit), _order, _order_direction).await
            })
        }))),
    })
}

/// Errors that can occur when adding a favorite artist.
#[derive(Debug, Error)]
pub enum LibraryAddFavoriteArtistError {
    /// No user ID is available for the request.
    #[error("No user ID available")]
    NoUserIdAvailable,
    /// The request failed with an error message.
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Adds an artist to the user's favorites.
///
/// # Errors
///
/// * If no user id is available for the request
/// * If the request failed
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
pub const fn add_favorite_artist(
    _db: &LibraryDatabase,
    _artist_id: &Id,
) -> Result<(), LibraryAddFavoriteArtistError> {
    Ok(())
}

/// Errors that can occur when removing a favorite artist.
#[derive(Debug, Error)]
pub enum LibraryRemoveFavoriteArtistError {
    /// No user ID is available for the request.
    #[error("No user ID available")]
    NoUserIdAvailable,
    /// The request failed with an error message.
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Removes an artist from the user's favorites.
///
/// # Errors
///
/// * If no user id is available for the request
/// * If the request failed
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
pub const fn remove_favorite_artist(
    _db: &LibraryDatabase,
    _artist_id: &Id,
) -> Result<(), LibraryRemoveFavoriteArtistError> {
    Ok(())
}

/// Sort order for album listings.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryAlbumOrder {
    /// Sort by date added.
    Date,
}

/// Sort direction for album listings.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryAlbumOrderDirection {
    /// Ascending order.
    Asc,
    /// Descending order.
    Desc,
}

/// Filters albums based on the provided request criteria.
pub fn filter_albums<'a>(
    albums: &'a [LibraryAlbum],
    request: &'a AlbumsRequest,
) -> impl Iterator<Item = &'a LibraryAlbum> {
    let albums = albums.iter().filter(|album| {
        request.filters.as_ref().is_none_or(|x| {
            x.artist_id
                .as_ref()
                .is_none_or(|id| &Id::Number(album.artist_id) == id)
        })
    });

    let albums = albums.filter(|album| {
        request.filters.as_ref().is_none_or(|x| {
            x.artist_api_id.as_ref().is_none_or(|id| {
                album
                    .artist_sources
                    .iter()
                    .filter(|x| x.source == id.source)
                    .map(|x| &x.id)
                    .any(|x| x == &id.id)
            })
        })
    });

    albums
        .filter(|album| {
            request.sources.as_ref().is_none_or(|s| {
                s.iter().any(|source| {
                    album
                        .versions
                        .iter()
                        .any(|v| v.source == source.clone().into())
                })
            })
        })
        .filter(|album| {
            request.filters.as_ref().is_none_or(|x| {
                x.album_type
                    .map(Into::into)
                    .is_none_or(|t| album.album_type == t)
            })
        })
        .filter(|album| {
            request.filters.as_ref().is_none_or(|x| {
                x.name
                    .as_ref()
                    .is_none_or(|s| album.title.to_lowercase().contains(s))
            })
        })
        .filter(|album| {
            request.filters.as_ref().is_none_or(|x| {
                x.artist
                    .as_ref()
                    .is_none_or(|s| album.artist.to_lowercase().contains(s))
            })
        })
        .filter(|album| {
            request.filters.as_ref().is_none_or(|x| {
                x.search.as_ref().is_none_or(|s| {
                    album.title.to_lowercase().contains(s)
                        || album.artist.to_lowercase().contains(s)
                })
            })
        })
}

/// Sorts albums based on the provided request criteria.
#[must_use]
pub fn sort_albums<'a>(
    mut albums: Vec<&'a LibraryAlbum>,
    request: &'a AlbumsRequest,
) -> Vec<&'a LibraryAlbum> {
    match request.sort {
        Some(AlbumSort::ArtistAsc) => albums.sort_by(|a, b| a.artist.cmp(&b.artist)),
        Some(AlbumSort::NameAsc) => albums.sort_by(|a, b| a.title.cmp(&b.title)),
        Some(AlbumSort::ArtistDesc) => albums.sort_by(|a, b| b.artist.cmp(&a.artist)),
        Some(AlbumSort::NameDesc) => albums.sort_by(|a, b| b.title.cmp(&a.title)),
        _ => (),
    }
    match request.sort {
        Some(AlbumSort::ArtistAsc) => {
            albums.sort_by(|a, b| a.artist.to_lowercase().cmp(&b.artist.to_lowercase()));
        }
        Some(AlbumSort::NameAsc) => {
            albums.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
        }
        Some(AlbumSort::ArtistDesc) => {
            albums.sort_by(|a, b| b.artist.to_lowercase().cmp(&a.artist.to_lowercase()));
        }
        Some(AlbumSort::NameDesc) => {
            albums.sort_by(|a, b| b.title.to_lowercase().cmp(&a.title.to_lowercase()));
        }
        Some(AlbumSort::ReleaseDateAsc) => albums.sort_by(|a, b| {
            if a.date_released.is_none() {
                return Ordering::Greater;
            }
            if b.date_released.is_none() {
                return Ordering::Less;
            }

            a.date_released.cmp(&b.date_released)
        }),
        Some(AlbumSort::ReleaseDateDesc) => albums.sort_by(|a, b| {
            if a.date_released.is_none() {
                return Ordering::Greater;
            }
            if b.date_released.is_none() {
                return Ordering::Less;
            }

            b.date_released.cmp(&a.date_released)
        }),
        Some(AlbumSort::DateAddedAsc) => albums.sort_by(|a, b| a.date_added.cmp(&b.date_added)),
        Some(AlbumSort::DateAddedDesc) => albums.sort_by(|b, a| a.date_added.cmp(&b.date_added)),
        None => (),
    }

    albums
}

/// Errors that can occur when retrieving favorite albums.
#[derive(Debug, Error)]
pub enum LibraryFavoriteAlbumsError {
    /// No user ID is available for the request.
    #[error("No user ID available")]
    NoUserIdAvailable,
    /// The request failed with an error message.
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Retrieves a paginated list of favorite albums from the library with filtering and sorting.
///
/// # Errors
///
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn favorite_albums(
    db: &LibraryDatabase,
    request: &AlbumsRequest,
) -> PagingResult<LibraryAlbum, LibraryFavoriteAlbumsError> {
    let albums = db::get_albums(db).await?; // TODO: should this be cached?
    let items = sort_albums(filter_albums(&albums, request).collect::<Vec<_>>(), request);

    #[allow(clippy::cast_possible_truncation)]
    let total = items.len() as u32;
    let offset = request.page.as_ref().map_or(0, |x| x.offset);
    let limit = request.page.as_ref().map_or(total, |x| x.limit);

    let items = if offset != 0 || limit != total {
        items
            .into_iter()
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect::<Vec<_>>()
    } else {
        items.into_iter().cloned().collect::<Vec<_>>()
    };

    log::trace!("Received favorite albums response: {items:?}");

    let db = db.to_owned();
    let request = request.clone();

    Ok(PagingResponse {
        page: Page::WithTotal {
            items,
            offset,
            limit,
            total,
        },
        fetch: Arc::new(Mutex::new(Box::new(move |offset, limit| {
            let db = db.clone();
            let mut request = request.clone();

            request.page = Some(PagingRequest { offset, limit });

            Box::pin(async move { favorite_albums(&db, &request).await })
        }))),
    })
}

/// Errors that can occur when adding a favorite album.
#[derive(Debug, Error)]
pub enum LibraryAddFavoriteAlbumError {
    /// No user ID is available for the request.
    #[error("No user ID available")]
    NoUserIdAvailable,
    /// The request failed with an error message.
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Adds an album to the user's favorites.
///
/// # Errors
///
/// * If no user id is available for the request
/// * If the request failed
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
pub const fn add_favorite_album(
    _db: &LibraryDatabase,
    _album_id: &Id,
) -> Result<(), LibraryAddFavoriteAlbumError> {
    Ok(())
}

/// Errors that can occur when removing a favorite album.
#[derive(Debug, Error)]
pub enum LibraryRemoveFavoriteAlbumError {
    /// No user ID is available for the request.
    #[error("No user ID available")]
    NoUserIdAvailable,
    /// The request failed with an error message.
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Removes an album from the user's favorites.
///
/// # Errors
///
/// * If no user id is available for the request
/// * If the request failed
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
pub const fn remove_favorite_album(
    _db: &LibraryDatabase,
    _album_id: &Id,
) -> Result<(), LibraryRemoveFavoriteAlbumError> {
    Ok(())
}

/// Sort order for track listings.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryTrackOrder {
    /// Sort by date added.
    Date,
}

/// Sort direction for track listings.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryTrackOrderDirection {
    /// Ascending order.
    Asc,
    /// Descending order.
    Desc,
}

/// Errors that can occur when retrieving favorite tracks.
#[derive(Debug, Error)]
pub enum LibraryFavoriteTracksError {
    /// No user ID is available for the request.
    #[error("No user ID available")]
    NoUserIdAvailable,
    /// The request failed with an error message.
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Retrieves a paginated list of favorite tracks from the library.
///
/// # Errors
///
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn favorite_tracks(
    db: &LibraryDatabase,
    track_ids: Option<&[Id]>,
    offset: Option<u32>,
    limit: Option<u32>,
    #[allow(clippy::used_underscore_binding)] _order: Option<LibraryTrackOrder>,
    #[allow(clippy::used_underscore_binding)] _order_direction: Option<LibraryTrackOrderDirection>,
) -> PagingResult<LibraryTrack, LibraryFavoriteTracksError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let items = db::get_tracks(db, track_ids).await?;
    log::trace!("Received favorite tracks response: {items:?}");

    #[allow(clippy::cast_possible_truncation)]
    let total = items.len() as u32;

    Ok(PagingResponse {
        page: Page::WithTotal {
            items,
            offset,
            limit,
            total,
        },
        fetch: Arc::new(Mutex::new(Box::new({
            let db = db.to_owned();
            let track_ids = track_ids.map(<[Id]>::to_vec);

            move |offset, limit| {
                let db = db.clone();
                let track_ids = track_ids.clone();

                Box::pin(async move {
                    favorite_tracks(
                        &db,
                        track_ids.as_deref(),
                        Some(offset),
                        Some(limit),
                        _order,
                        _order_direction,
                    )
                    .await
                })
            }
        }))),
    })
}

/// Errors that can occur when adding a favorite track.
#[derive(Debug, Error)]
pub enum LibraryAddFavoriteTrackError {
    /// No user ID is available for the request.
    #[error("No user ID available")]
    NoUserIdAvailable,
    /// The request failed with an error message.
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Adds a track to the user's favorites.
///
/// # Errors
///
/// * If no user id is available for the request
/// * If the request failed
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
pub const fn add_favorite_track(
    _db: &LibraryDatabase,
    _track_id: &Id,
) -> Result<(), LibraryAddFavoriteTrackError> {
    Ok(())
}

/// Errors that can occur when removing a favorite track.
#[derive(Debug, Error)]
pub enum LibraryRemoveFavoriteTrackError {
    /// No user ID is available for the request.
    #[error("No user ID available")]
    NoUserIdAvailable,
    /// The request failed with an error message.
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Removes a track from the user's favorites.
///
/// # Errors
///
/// * If no user id is available for the request
/// * If the request failed
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
pub const fn remove_favorite_track(
    _db: &LibraryDatabase,
    _track_id: &Id,
) -> Result<(), LibraryRemoveFavoriteTrackError> {
    Ok(())
}

/// Errors that can occur when retrieving albums for an artist.
#[derive(Debug, Error)]
pub enum LibraryArtistAlbumsError {
    /// The request failed with an error message.
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Retrieves a paginated list of albums for a specific artist.
///
/// # Errors
///
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn artist_albums(
    db: &LibraryDatabase,
    artist_id: &Id,
    offset: Option<u32>,
    limit: Option<u32>,
    album_type: Option<LibraryAlbumType>,
) -> PagingResult<LibraryAlbum, LibraryArtistAlbumsError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let items = db::get_artist_albums(db, artist_id).await?;
    let items = if let Some(album_type) = album_type {
        items
            .into_iter()
            .filter(|x| x.album_type == album_type)
            .collect()
    } else {
        items
    };
    log::trace!("Received artist albums response: {items:?}");

    #[allow(clippy::cast_possible_truncation)]
    let total = items.len() as u32;

    let db = db.to_owned();
    let artist_id = artist_id.clone();

    Ok(PagingResponse {
        page: Page::WithTotal {
            items,
            offset,
            limit,
            total,
        },
        fetch: Arc::new(Mutex::new(Box::new(move |offset, limit| {
            let db = db.clone();
            let artist_id = artist_id.clone();

            Box::pin(async move {
                artist_albums(&db, &artist_id, Some(offset), Some(limit), album_type).await
            })
        }))),
    })
}

/// Errors that can occur when retrieving tracks for an album.
#[derive(Debug, Error)]
pub enum LibraryAlbumTracksError {
    /// The request failed with an error message.
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Retrieves a paginated list of tracks for a specific album.
///
/// # Errors
///
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn album_tracks(
    db: &LibraryDatabase,
    album_id: &Id,
    offset: Option<u32>,
    limit: Option<u32>,
) -> PagingResult<LibraryTrack, LibraryAlbumTracksError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let items = db::get_album_tracks(db, album_id).await?;
    log::trace!("Received album tracks response: {items:?}");

    #[allow(clippy::cast_possible_truncation)]
    let total = items.len() as u32;

    let db = db.to_owned();
    let album_id = album_id.clone();

    Ok(PagingResponse {
        page: Page::WithTotal {
            items,
            offset,
            limit,
            total,
        },
        fetch: Arc::new(Mutex::new(Box::new(move |offset, limit| {
            let db = db.clone();
            let album_id = album_id.clone();

            Box::pin(async move { album_tracks(&db, &album_id, Some(offset), Some(limit)).await })
        }))),
    })
}

/// Errors that can occur when retrieving an album.
#[derive(Debug, Error)]
pub enum LibraryAlbumError {
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Retrieves album information by ID from a specific API source.
///
/// # Errors
///
/// * If there was a database error
pub async fn album_from_source(
    db: &LibraryDatabase,
    album_id: &Id,
    source: &ApiSource,
) -> Result<Option<LibraryAlbum>, LibraryAlbumError> {
    Ok(db::get_album(db, source, album_id).await?)
}

/// Retrieves album information by ID from the library.
///
/// # Errors
///
/// * If there was a database error
pub async fn album(
    db: &LibraryDatabase,
    album_id: &Id,
) -> Result<Option<LibraryAlbum>, LibraryAlbumError> {
    Ok(db::get_album(db, ApiSource::library_ref(), album_id).await?)
}

/// Sorts album versions by audio quality metrics in descending order of sample rate, bit depth, and source.
pub fn sort_album_versions(versions: &mut [AlbumVersion]) {
    versions.sort_by(|a, b| {
        b.sample_rate
            .unwrap_or_default()
            .cmp(&a.sample_rate.unwrap_or_default())
    });
    versions.sort_by(|a, b| {
        b.bit_depth
            .unwrap_or_default()
            .cmp(&a.bit_depth.unwrap_or_default())
    });
    versions.sort_by(|a, b| a.source.cmp(&b.source));
}

/// Retrieves all available versions of an album with different audio qualities.
///
/// # Errors
///
/// * If there was a database error
pub async fn album_versions(
    db: &LibraryDatabase,
    album_id: &Id,
) -> Result<Vec<AlbumVersion>, LibraryAlbumTracksError> {
    log::trace!("album_versions: album_id={album_id}");

    let tracks = album_tracks(db, album_id, None, None)
        .await?
        .with_rest_of_items_in_batches()
        .await?;
    log::trace!("Got {} album id={album_id} tracks", tracks.len());

    let mut versions = vec![];

    for track in tracks {
        if versions.is_empty() {
            log::trace!("No versions exist yet. Creating first version");
            versions.push(AlbumVersion {
                tracks: vec![track.clone().into()],
                format: track.format,
                bit_depth: track.bit_depth,
                sample_rate: track.sample_rate,
                channels: track.channels,
                source: track.source,
            });
            continue;
        }

        if let Some(existing_version) = versions.iter_mut().find(|v| {
            v.sample_rate == track.sample_rate
                && v.bit_depth == track.bit_depth
                && v.tracks[0].directory() == track.directory()
                && v.source == track.source
        }) {
            log::trace!("Adding track to existing version");
            existing_version.tracks.push(track.into());
        } else {
            log::trace!("Adding track to new version");
            versions.push(AlbumVersion {
                tracks: vec![track.clone().into()],
                format: track.format,
                bit_depth: track.bit_depth,
                sample_rate: track.sample_rate,
                channels: track.channels,
                source: track.source,
            });
        }
    }

    sort_album_versions(&mut versions);

    Ok(versions)
}

/// Errors that can occur when retrieving an artist.
#[derive(Debug, Error)]
pub enum LibraryArtistError {
    /// The requested artist was not found.
    #[error("Not found")]
    NotFound,
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Retrieves artist information by ID from the library.
///
/// # Errors
///
/// * If the artist was not found
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
pub async fn artist(
    db: &LibraryDatabase,
    artist_id: &Id,
) -> Result<LibraryArtist, LibraryArtistError> {
    db::get_artist(db, ApiSource::library_ref(), artist_id)
        .await?
        .ok_or(LibraryArtistError::NotFound)
}

/// Errors that can occur when retrieving a track.
#[derive(Debug, Error)]
pub enum LibraryTrackError {
    /// The requested track was not found.
    #[error("Not found")]
    NotFound,
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Retrieves track information by ID from the library.
///
/// # Errors
///
/// * If the track was not found
/// * If there was a database error
pub async fn track(
    db: &LibraryDatabase,
    track_id: &Id,
) -> Result<Option<LibraryTrack>, LibraryTrackError> {
    Ok(db::get_track(db, track_id).await?)
}

/// Types of content that can be searched in the library.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, EnumString, AsRefStr)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum SearchType {
    /// Search for artists.
    Artists,
    /// Search for albums.
    Albums,
    /// Search for tracks.
    Tracks,
    /// Search for videos.
    Videos,
    /// Search for playlists.
    Playlists,
    /// Search for user profiles.
    UserProfiles,
}

impl From<SearchType> for LibrarySearchType {
    fn from(value: SearchType) -> Self {
        match value {
            SearchType::Artists => Self::Artists,
            SearchType::Albums => Self::Albums,
            SearchType::Tracks => Self::Tracks,
            SearchType::Videos => Self::Videos,
            SearchType::Playlists => Self::Playlists,
            SearchType::UserProfiles => Self::UserProfiles,
        }
    }
}

/// Internal representation of search types for library queries.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, EnumString, AsRefStr)]
#[serde(rename_all = "UPPERCASE")]
#[strum(serialize_all = "UPPERCASE")]
pub enum LibrarySearchType {
    /// Search for artists.
    Artists,
    /// Search for albums.
    Albums,
    /// Search for tracks.
    Tracks,
    /// Search for videos.
    Videos,
    /// Search for playlists.
    Playlists,
    /// Search for user profiles.
    UserProfiles,
}

/// Errors that can occur during library search operations.
#[derive(Debug, Error)]
pub enum SearchError {
    /// Search index operation failed.
    #[error(transparent)]
    SearchIndex(#[from] SearchIndexError),
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
}

/// Searches the library for content matching the query string.
///
/// # Errors
///
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
pub fn search(
    query: &str,
    offset: Option<u32>,
    limit: Option<u32>,
    _types: Option<&[LibrarySearchType]>,
) -> Result<ApiSearchResultsResponse, SearchError> {
    let results = moosicbox_search::global_search(query, offset, limit)?;
    log::trace!("Received search response: results={results:?}");

    Ok(results)
}

/// Audio quality levels for library tracks.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryAudioQuality {
    /// High quality compressed audio.
    High,
    /// Lossless audio quality.
    Lossless,
    /// High-resolution lossless audio quality.
    HiResLossless,
}

/// Errors that can occur when retrieving a track file URL.
#[derive(Debug, Error)]
pub enum LibraryTrackFileUrlError {
    /// The track has no associated file.
    #[error("Track has no file")]
    NoFile,
    /// Track retrieval error.
    #[error(transparent)]
    LibraryTrack(#[from] LibraryTrackError),
}

/// Retrieves the file URL for a track at the specified audio quality.
///
/// # Errors
///
/// * If the track has no associated file
/// * If the track was not found
/// * If there was a database error
pub async fn track_file_url(
    db: &LibraryDatabase,
    _audio_quality: LibraryAudioQuality,
    track_id: &Id,
) -> Result<String, LibraryTrackFileUrlError> {
    let track = track(db, track_id)
        .await?
        .ok_or(LibraryTrackFileUrlError::NoFile)?;
    log::trace!("Received track file url response: {track:?}");

    track.file.ok_or(LibraryTrackFileUrlError::NoFile)
}

impl From<ArtistOrder> for LibraryArtistOrder {
    fn from(value: ArtistOrder) -> Self {
        match value {
            ArtistOrder::DateAdded => Self::Date,
        }
    }
}

impl From<ArtistOrderDirection> for LibraryArtistOrderDirection {
    fn from(value: ArtistOrderDirection) -> Self {
        match value {
            ArtistOrderDirection::Ascending => Self::Asc,
            ArtistOrderDirection::Descending => Self::Desc,
        }
    }
}

impl From<AlbumOrder> for LibraryAlbumOrder {
    fn from(value: AlbumOrder) -> Self {
        match value {
            AlbumOrder::DateAdded => Self::Date,
        }
    }
}

impl From<AlbumOrderDirection> for LibraryAlbumOrderDirection {
    fn from(value: AlbumOrderDirection) -> Self {
        match value {
            AlbumOrderDirection::Ascending => Self::Asc,
            AlbumOrderDirection::Descending => Self::Desc,
        }
    }
}

impl From<TrackOrder> for LibraryTrackOrder {
    fn from(value: TrackOrder) -> Self {
        match value {
            TrackOrder::DateAdded => Self::Date,
        }
    }
}

impl From<TrackOrderDirection> for LibraryTrackOrderDirection {
    fn from(value: TrackOrderDirection) -> Self {
        match value {
            TrackOrderDirection::Ascending => Self::Asc,
            TrackOrderDirection::Descending => Self::Desc,
        }
    }
}

/// Errors that can occur when converting album types.
#[derive(Debug, Error)]
pub enum TryFromAlbumTypeError {
    /// The album type is not supported.
    #[error("Unsupported AlbumType")]
    UnsupportedAlbumType,
}

/// Errors that can occur when determining track size.
#[derive(Debug, Error)]
pub enum TrackSizeError {
    /// The audio format is not supported.
    #[error("Unsupported audio format: {0:?}")]
    UnsupportedFormat(AudioFormat),
    /// The track source is not supported.
    #[error("Unsupported track source: {0:?}")]
    UnsupportedSource(TrackSource),
}

/// Errors that can occur when reindexing the search database.
#[derive(Debug, Error)]
pub enum ReindexError {
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Failed to recreate the search index.
    #[error(transparent)]
    RecreateIndex(#[from] RecreateIndexError),
    /// Failed to populate the search index.
    #[error(transparent)]
    PopulateIndex(#[from] PopulateIndexError),
    /// Failed to get albums.
    #[error("Failed to get albums: {0:?}")]
    GetAlbums(Box<dyn std::error::Error>),
}

/// Rebuilds the global search index with all library content.
///
/// # Panics
///
/// * If time went backwards
///
/// # Errors
///
/// * If there was a database error
/// * If failed to recreate the index
/// * If failed to populate the index
pub async fn reindex_global_search_index(db: &LibraryDatabase) -> Result<(), ReindexError> {
    let reindex_start = switchy_time::now();

    moosicbox_search::data::recreate_global_search_index().await?;

    let artists = db::get_artists(db)
        .await?
        .into_iter()
        .map(Into::into)
        .map(|artist: Artist| artist.as_data_values())
        .collect::<Vec<_>>();

    populate_global_search_index(&artists, false).await?;

    let albums = db::get_albums(db)
        .await?
        .into_iter()
        .map(TryInto::try_into)
        .map(|album: Result<Album, _>| album.map(|x| x.as_data_values()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ReindexError::GetAlbums(Box::new(e)))?;

    populate_global_search_index(&albums, false).await?;

    let tracks = db::get_tracks(db, None)
        .await?
        .into_iter()
        .map(Into::into)
        .map(|track: Track| track.as_data_values())
        .collect::<Vec<_>>();

    populate_global_search_index(&tracks, false).await?;

    let reindex_end = switchy_time::now();
    log::info!(
        "Finished search reindex update for scan in {}ms",
        reindex_end
            .duration_since(reindex_start)
            .unwrap()
            .as_millis()
    );

    Ok(())
}

#[cfg(test)]
mod test {
    use moosicbox_music_api_models::AlbumFilters;
    use moosicbox_music_models::AlbumSource;

    use super::*;

    #[test_log::test]
    fn filter_albums_empty_albums_returns_empty_albums() {
        let albums = vec![];
        let result = filter_albums(
            &albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: None,
                page: Some(PagingRequest {
                    offset: 0,
                    limit: 10,
                }),
            },
        )
        .cloned()
        .collect::<Vec<_>>();
        assert_eq!(result, vec![]);
    }

    #[test_log::test]
    fn filter_albums_filters_albums_of_sources_that_dont_match() {
        use moosicbox_music_models::{AlbumVersionQuality, TrackApiSource};

        let local = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: String::new(),
            artwork: None,
            versions: vec![AlbumVersionQuality {
                source: TrackApiSource::Local,
                ..Default::default()
            }],
            ..Default::default()
        };
        let tidal = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: String::new(),
            artwork: None,
            versions: vec![AlbumVersionQuality {
                source: TrackApiSource::Api(ApiSource::register("Tidal", "Tidal")),
                ..Default::default()
            }],
            ..Default::default()
        };
        let qobuz = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: String::new(),
            artwork: None,
            versions: vec![AlbumVersionQuality {
                source: TrackApiSource::Api(ApiSource::register("Qobuz", "Qobuz")),
                ..Default::default()
            }],
            ..Default::default()
        };
        let albums = vec![local.clone(), tidal, qobuz];
        let result = filter_albums(
            &albums,
            &AlbumsRequest {
                sources: Some(vec![AlbumSource::Local]),
                sort: None,
                filters: None,
                page: Some(PagingRequest {
                    offset: 0,
                    limit: 10,
                }),
            },
        )
        .cloned()
        .collect::<Vec<_>>();
        assert_eq!(result, vec![local]);
    }

    #[test_log::test]
    fn filter_albums_filters_albums_of_name_that_dont_match() {
        let bob = LibraryAlbum {
            id: 0,
            title: "bob".to_string(),
            artist: String::new(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "sally".to_string(),
            artist: String::new(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "test".to_string(),
            artist: String::new(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            &albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: Some(AlbumFilters {
                    name: Some("test".to_string()),
                    ..Default::default()
                }),
                page: Some(PagingRequest {
                    offset: 0,
                    limit: 10,
                }),
            },
        )
        .cloned()
        .collect::<Vec<_>>();
        assert_eq!(result, vec![test]);
    }

    #[test_log::test]
    fn filter_albums_filters_albums_of_name_that_dont_match_and_searches_multiple_words() {
        let bob = LibraryAlbum {
            id: 0,
            title: "bob".to_string(),
            artist: String::new(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "sally".to_string(),
            artist: String::new(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "one test two".to_string(),
            artist: String::new(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            &albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: Some(AlbumFilters {
                    name: Some("test".to_string()),
                    ..Default::default()
                }),
                page: Some(PagingRequest {
                    offset: 0,
                    limit: 10,
                }),
            },
        )
        .cloned()
        .collect::<Vec<_>>();
        assert_eq!(result, vec![test]);
    }

    #[test_log::test]
    fn filter_albums_filters_albums_of_artist_that_dont_match() {
        let bob = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: "test".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            &albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: Some(AlbumFilters {
                    artist: Some("test".to_string()),
                    ..Default::default()
                }),
                page: Some(PagingRequest {
                    offset: 0,
                    limit: 10,
                }),
            },
        )
        .cloned()
        .collect::<Vec<_>>();
        assert_eq!(result, vec![test]);
    }

    #[test_log::test]
    fn filter_albums_filters_albums_of_artist_that_dont_match_and_searches_multiple_words() {
        let bob = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: "one test two".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            &albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: Some(AlbumFilters {
                    artist: Some("test".to_string()),
                    ..Default::default()
                }),
                page: Some(PagingRequest {
                    offset: 0,
                    limit: 10,
                }),
            },
        )
        .cloned()
        .collect::<Vec<_>>();
        assert_eq!(result, vec![test]);
    }

    #[test_log::test]
    fn filter_albums_filters_albums_of_search_that_dont_match_artist() {
        let bob = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: "test".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            &albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: Some(AlbumFilters {
                    search: Some("test".to_string()),
                    ..Default::default()
                }),
                page: Some(PagingRequest {
                    offset: 0,
                    limit: 10,
                }),
            },
        )
        .cloned()
        .collect::<Vec<_>>();
        assert_eq!(result, vec![test]);
    }

    #[test_log::test]
    fn filter_albums_filters_albums_of_search_that_dont_match_artist_and_searches_multiple_words() {
        let bob = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: String::new(),
            artist: "one test two".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            &albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: Some(AlbumFilters {
                    search: Some("test".to_string()),
                    ..Default::default()
                }),
                page: Some(PagingRequest {
                    offset: 0,
                    limit: 10,
                }),
            },
        )
        .cloned()
        .collect::<Vec<_>>();
        assert_eq!(result, vec![test]);
    }

    #[test_log::test]
    fn filter_albums_filters_albums_of_search_that_dont_match_name() {
        let bob = LibraryAlbum {
            id: 0,
            title: "bob".to_string(),
            artist: String::new(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "sally".to_string(),
            artist: String::new(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "test".to_string(),
            artist: String::new(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            &albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: Some(AlbumFilters {
                    search: Some("test".to_string()),
                    ..Default::default()
                }),
                page: Some(PagingRequest {
                    offset: 0,
                    limit: 10,
                }),
            },
        )
        .cloned()
        .collect::<Vec<_>>();
        assert_eq!(result, vec![test]);
    }

    #[test_log::test]
    fn filter_albums_filters_albums_of_search_that_dont_match_name_and_searches_multiple_words() {
        let bob = LibraryAlbum {
            id: 0,
            title: "bob".to_string(),
            artist: String::new(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "sally".to_string(),
            artist: String::new(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "one test two".to_string(),
            artist: String::new(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            &albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: Some(AlbumFilters {
                    search: Some("test".to_string()),
                    ..Default::default()
                }),
                page: Some(PagingRequest {
                    offset: 0,
                    limit: 10,
                }),
            },
        )
        .cloned()
        .collect::<Vec<_>>();
        assert_eq!(result, vec![test]);
    }

    #[test_log::test]
    fn filter_albums_filters_albums_of_search_that_dont_match_and_searches_across_properties() {
        let bob = LibraryAlbum {
            id: 0,
            title: "bob".to_string(),
            artist: "test".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "sally".to_string(),
            artist: String::new(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "one test two".to_string(),
            artist: String::new(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob.clone(), sally, test.clone()];
        let result = filter_albums(
            &albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: Some(AlbumFilters {
                    search: Some("test".to_string()),
                    ..Default::default()
                }),
                page: Some(PagingRequest {
                    offset: 0,
                    limit: 10,
                }),
            },
        )
        .cloned()
        .collect::<Vec<_>>();
        assert_eq!(result, vec![bob, test]);
    }

    #[test_log::test]
    fn sort_albums_by_artist_asc() {
        let zebra = LibraryAlbum {
            id: 1,
            title: String::new(),
            artist: "Zebra".to_string(),
            ..Default::default()
        };
        let alpha = LibraryAlbum {
            id: 2,
            title: String::new(),
            artist: "Alpha".to_string(),
            ..Default::default()
        };
        let beta = LibraryAlbum {
            id: 3,
            title: String::new(),
            artist: "Beta".to_string(),
            ..Default::default()
        };

        let albums = vec![&zebra, &alpha, &beta];
        let request = AlbumsRequest {
            sort: Some(AlbumSort::ArtistAsc),
            ..Default::default()
        };
        let result = sort_albums(albums, &request);

        assert_eq!(result, vec![&alpha, &beta, &zebra]);
    }

    #[test_log::test]
    fn sort_albums_by_artist_desc() {
        let zebra = LibraryAlbum {
            id: 1,
            title: String::new(),
            artist: "Zebra".to_string(),
            ..Default::default()
        };
        let alpha = LibraryAlbum {
            id: 2,
            title: String::new(),
            artist: "Alpha".to_string(),
            ..Default::default()
        };
        let beta = LibraryAlbum {
            id: 3,
            title: String::new(),
            artist: "Beta".to_string(),
            ..Default::default()
        };

        let albums = vec![&alpha, &beta, &zebra];
        let request = AlbumsRequest {
            sort: Some(AlbumSort::ArtistDesc),
            ..Default::default()
        };
        let result = sort_albums(albums, &request);

        assert_eq!(result, vec![&zebra, &beta, &alpha]);
    }

    #[test_log::test]
    fn sort_albums_by_name_asc() {
        let zoo = LibraryAlbum {
            id: 1,
            title: "Zoo".to_string(),
            artist: String::new(),
            ..Default::default()
        };
        let apple = LibraryAlbum {
            id: 2,
            title: "Apple".to_string(),
            artist: String::new(),
            ..Default::default()
        };
        let banana = LibraryAlbum {
            id: 3,
            title: "Banana".to_string(),
            artist: String::new(),
            ..Default::default()
        };

        let albums = vec![&zoo, &apple, &banana];
        let request = AlbumsRequest {
            sort: Some(AlbumSort::NameAsc),
            ..Default::default()
        };
        let result = sort_albums(albums, &request);

        assert_eq!(result, vec![&apple, &banana, &zoo]);
    }

    #[test_log::test]
    fn sort_albums_by_name_desc() {
        let zoo = LibraryAlbum {
            id: 1,
            title: "Zoo".to_string(),
            artist: String::new(),
            ..Default::default()
        };
        let apple = LibraryAlbum {
            id: 2,
            title: "Apple".to_string(),
            artist: String::new(),
            ..Default::default()
        };
        let banana = LibraryAlbum {
            id: 3,
            title: "Banana".to_string(),
            artist: String::new(),
            ..Default::default()
        };

        let albums = vec![&apple, &banana, &zoo];
        let request = AlbumsRequest {
            sort: Some(AlbumSort::NameDesc),
            ..Default::default()
        };
        let result = sort_albums(albums, &request);

        assert_eq!(result, vec![&zoo, &banana, &apple]);
    }

    #[test_log::test]
    fn sort_albums_by_release_date_asc_with_none_values() {
        let no_date = LibraryAlbum {
            id: 1,
            title: "No Date".to_string(),
            artist: String::new(),
            date_released: None,
            ..Default::default()
        };
        let old = LibraryAlbum {
            id: 2,
            title: "Old".to_string(),
            artist: String::new(),
            date_released: Some("2020-01-01".to_string()),
            ..Default::default()
        };
        let new = LibraryAlbum {
            id: 3,
            title: "New".to_string(),
            artist: String::new(),
            date_released: Some("2024-01-01".to_string()),
            ..Default::default()
        };

        let albums = vec![&no_date, &new, &old];
        let request = AlbumsRequest {
            sort: Some(AlbumSort::ReleaseDateAsc),
            ..Default::default()
        };
        let result = sort_albums(albums, &request);

        // None values should be sorted to the end (Greater)
        assert_eq!(result, vec![&old, &new, &no_date]);
    }

    #[test_log::test]
    fn sort_albums_by_release_date_desc_with_none_values() {
        let no_date = LibraryAlbum {
            id: 1,
            title: "No Date".to_string(),
            artist: String::new(),
            date_released: None,
            ..Default::default()
        };
        let old = LibraryAlbum {
            id: 2,
            title: "Old".to_string(),
            artist: String::new(),
            date_released: Some("2020-01-01".to_string()),
            ..Default::default()
        };
        let new = LibraryAlbum {
            id: 3,
            title: "New".to_string(),
            artist: String::new(),
            date_released: Some("2024-01-01".to_string()),
            ..Default::default()
        };

        let albums = vec![&old, &new, &no_date];
        let request = AlbumsRequest {
            sort: Some(AlbumSort::ReleaseDateDesc),
            ..Default::default()
        };
        let result = sort_albums(albums, &request);

        // None values should be sorted to the end (Greater)
        assert_eq!(result, vec![&new, &old, &no_date]);
    }

    #[test_log::test]
    fn sort_albums_by_date_added_asc() {
        let old = LibraryAlbum {
            id: 1,
            title: "Old".to_string(),
            artist: String::new(),
            date_added: Some("2020-01-01T00:00:00Z".to_string()),
            ..Default::default()
        };
        let new = LibraryAlbum {
            id: 2,
            title: "New".to_string(),
            artist: String::new(),
            date_added: Some("2024-01-01T00:00:00Z".to_string()),
            ..Default::default()
        };

        let albums = vec![&new, &old];
        let request = AlbumsRequest {
            sort: Some(AlbumSort::DateAddedAsc),
            ..Default::default()
        };
        let result = sort_albums(albums, &request);

        assert_eq!(result, vec![&old, &new]);
    }

    #[test_log::test]
    fn sort_albums_by_date_added_desc() {
        let old = LibraryAlbum {
            id: 1,
            title: "Old".to_string(),
            artist: String::new(),
            date_added: Some("2020-01-01T00:00:00Z".to_string()),
            ..Default::default()
        };
        let new = LibraryAlbum {
            id: 2,
            title: "New".to_string(),
            artist: String::new(),
            date_added: Some("2024-01-01T00:00:00Z".to_string()),
            ..Default::default()
        };

        let albums = vec![&old, &new];
        let request = AlbumsRequest {
            sort: Some(AlbumSort::DateAddedDesc),
            ..Default::default()
        };
        let result = sort_albums(albums, &request);

        assert_eq!(result, vec![&new, &old]);
    }

    #[test_log::test]
    fn sort_albums_case_insensitive_artist() {
        let upper = LibraryAlbum {
            id: 1,
            title: String::new(),
            artist: "ZEBRA".to_string(),
            ..Default::default()
        };
        let lower = LibraryAlbum {
            id: 2,
            title: String::new(),
            artist: "alpha".to_string(),
            ..Default::default()
        };
        let mixed = LibraryAlbum {
            id: 3,
            title: String::new(),
            artist: "Beta".to_string(),
            ..Default::default()
        };

        let albums = vec![&upper, &lower, &mixed];
        let request = AlbumsRequest {
            sort: Some(AlbumSort::ArtistAsc),
            ..Default::default()
        };
        let result = sort_albums(albums, &request);

        // Should be case-insensitive: alpha, Beta, ZEBRA
        assert_eq!(result, vec![&lower, &mixed, &upper]);
    }

    #[test_log::test]
    fn sort_albums_case_insensitive_name() {
        let upper = LibraryAlbum {
            id: 1,
            title: "ZOO".to_string(),
            artist: String::new(),
            ..Default::default()
        };
        let lower = LibraryAlbum {
            id: 2,
            title: "apple".to_string(),
            artist: String::new(),
            ..Default::default()
        };
        let mixed = LibraryAlbum {
            id: 3,
            title: "Banana".to_string(),
            artist: String::new(),
            ..Default::default()
        };

        let albums = vec![&upper, &lower, &mixed];
        let request = AlbumsRequest {
            sort: Some(AlbumSort::NameAsc),
            ..Default::default()
        };
        let result = sort_albums(albums, &request);

        // Should be case-insensitive: apple, Banana, ZOO
        assert_eq!(result, vec![&lower, &mixed, &upper]);
    }

    #[test_log::test]
    fn sort_album_versions_by_sample_rate() {
        use moosicbox_menu_models::AlbumVersion;
        use moosicbox_music_models::TrackApiSource;

        let mut versions = vec![
            AlbumVersion {
                tracks: vec![],
                format: None,
                sample_rate: Some(44100),
                bit_depth: Some(16),
                channels: None,
                source: TrackApiSource::Local,
            },
            AlbumVersion {
                tracks: vec![],
                format: None,
                sample_rate: Some(192000),
                bit_depth: Some(24),
                channels: None,
                source: TrackApiSource::Local,
            },
            AlbumVersion {
                tracks: vec![],
                format: None,
                sample_rate: Some(96000),
                bit_depth: Some(24),
                channels: None,
                source: TrackApiSource::Local,
            },
        ];

        sort_album_versions(&mut versions);

        // Should be sorted by sample rate descending (highest first)
        assert_eq!(versions[0].sample_rate, Some(192000));
        assert_eq!(versions[1].sample_rate, Some(96000));
        assert_eq!(versions[2].sample_rate, Some(44100));
    }

    #[test_log::test]
    fn sort_album_versions_by_bit_depth() {
        use moosicbox_menu_models::AlbumVersion;
        use moosicbox_music_models::TrackApiSource;

        let mut versions = vec![
            AlbumVersion {
                tracks: vec![],
                format: None,
                sample_rate: Some(44100),
                bit_depth: Some(16),
                channels: None,
                source: TrackApiSource::Local,
            },
            AlbumVersion {
                tracks: vec![],
                format: None,
                sample_rate: Some(44100),
                bit_depth: Some(24),
                channels: None,
                source: TrackApiSource::Local,
            },
            AlbumVersion {
                tracks: vec![],
                format: None,
                sample_rate: Some(44100),
                bit_depth: Some(32),
                channels: None,
                source: TrackApiSource::Local,
            },
        ];

        sort_album_versions(&mut versions);

        // Should be sorted by bit depth descending (highest first) when sample rate is equal
        assert_eq!(versions[0].bit_depth, Some(32));
        assert_eq!(versions[1].bit_depth, Some(24));
        assert_eq!(versions[2].bit_depth, Some(16));
    }

    #[test_log::test]
    fn sort_album_versions_by_source() {
        use moosicbox_menu_models::AlbumVersion;
        use moosicbox_music_models::TrackApiSource;

        let tidal_source = TrackApiSource::Api(ApiSource::register("Tidal", "Tidal"));
        let qobuz_source = TrackApiSource::Api(ApiSource::register("Qobuz", "Qobuz"));

        let mut versions = vec![
            AlbumVersion {
                tracks: vec![],
                format: None,
                sample_rate: Some(44100),
                bit_depth: Some(16),
                channels: None,
                source: tidal_source,
            },
            AlbumVersion {
                tracks: vec![],
                format: None,
                sample_rate: Some(44100),
                bit_depth: Some(16),
                channels: None,
                source: TrackApiSource::Local,
            },
            AlbumVersion {
                tracks: vec![],
                format: None,
                sample_rate: Some(44100),
                bit_depth: Some(16),
                channels: None,
                source: qobuz_source,
            },
        ];

        sort_album_versions(&mut versions);

        // Should be sorted by source when sample rate and bit depth are equal
        // Local < Api sources, and Api sources are sorted by their names
        assert_eq!(versions[0].source, TrackApiSource::Local);
    }

    #[test_log::test]
    fn sort_album_versions_with_none_values() {
        use moosicbox_menu_models::AlbumVersion;
        use moosicbox_music_models::TrackApiSource;

        let mut versions = vec![
            AlbumVersion {
                tracks: vec![],
                format: None,
                sample_rate: None,
                bit_depth: None,
                channels: None,
                source: TrackApiSource::Local,
            },
            AlbumVersion {
                tracks: vec![],
                format: None,
                sample_rate: Some(96000),
                bit_depth: Some(24),
                channels: None,
                source: TrackApiSource::Local,
            },
            AlbumVersion {
                tracks: vec![],
                format: None,
                sample_rate: Some(44100),
                bit_depth: None,
                channels: None,
                source: TrackApiSource::Local,
            },
        ];

        sort_album_versions(&mut versions);

        // None values should be treated as 0 and sorted to the end
        assert_eq!(versions[0].sample_rate, Some(96000));
        assert_eq!(versions[1].sample_rate, Some(44100));
        assert_eq!(versions[2].sample_rate, None);
    }

    #[test_log::test]
    fn from_artist_order_to_library_artist_order() {
        use moosicbox_music_api_models::ArtistOrder;

        let result = LibraryArtistOrder::from(ArtistOrder::DateAdded);
        assert_eq!(result, LibraryArtistOrder::Date);
    }

    #[test_log::test]
    fn from_artist_order_direction_to_library_artist_order_direction() {
        use moosicbox_music_api_models::ArtistOrderDirection;

        let asc = LibraryArtistOrderDirection::from(ArtistOrderDirection::Ascending);
        assert_eq!(asc, LibraryArtistOrderDirection::Asc);

        let desc = LibraryArtistOrderDirection::from(ArtistOrderDirection::Descending);
        assert_eq!(desc, LibraryArtistOrderDirection::Desc);
    }

    #[test_log::test]
    fn from_album_order_to_library_album_order() {
        use moosicbox_music_api_models::AlbumOrder;

        let result = LibraryAlbumOrder::from(AlbumOrder::DateAdded);
        assert_eq!(result, LibraryAlbumOrder::Date);
    }

    #[test_log::test]
    fn from_album_order_direction_to_library_album_order_direction() {
        use moosicbox_music_api_models::AlbumOrderDirection;

        let asc = LibraryAlbumOrderDirection::from(AlbumOrderDirection::Ascending);
        assert_eq!(asc, LibraryAlbumOrderDirection::Asc);

        let desc = LibraryAlbumOrderDirection::from(AlbumOrderDirection::Descending);
        assert_eq!(desc, LibraryAlbumOrderDirection::Desc);
    }

    #[test_log::test]
    fn from_track_order_to_library_track_order() {
        use moosicbox_music_api_models::TrackOrder;

        let result = LibraryTrackOrder::from(TrackOrder::DateAdded);
        assert_eq!(result, LibraryTrackOrder::Date);
    }

    #[test_log::test]
    fn from_track_order_direction_to_library_track_order_direction() {
        use moosicbox_music_api_models::TrackOrderDirection;

        let asc = LibraryTrackOrderDirection::from(TrackOrderDirection::Ascending);
        assert_eq!(asc, LibraryTrackOrderDirection::Asc);

        let desc = LibraryTrackOrderDirection::from(TrackOrderDirection::Descending);
        assert_eq!(desc, LibraryTrackOrderDirection::Desc);
    }
}
