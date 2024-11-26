#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    cmp::Ordering,
    fs::File,
    sync::{Arc, LazyLock},
};

use db::{get_artist_by_album_id, SetTrackSize};
use models::{LibraryAlbum, LibraryArtist, LibraryTrack};

use async_recursion::async_recursion;
use async_trait::async_trait;
use moosicbox_core::{
    sqlite::{
        db::DbError,
        models::{Album, AlbumSort, AlbumType, ApiSource, Artist, Id, Track},
    },
    types::{AudioFormat, PlaybackQuality},
};
use moosicbox_database::profiles::LibraryDatabase;
use moosicbox_files::get_content_length;
use moosicbox_library_models::{track_source_to_u8, LibraryAlbumType};
use moosicbox_menu_models::AlbumVersion;
use moosicbox_music_api::{
    models::{
        AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder, ArtistOrderDirection,
        ImageCoverSize, ImageCoverSource, TrackAudioQuality, TrackOrder, TrackOrderDirection,
        TrackSource,
    },
    AddAlbumError, AddArtistError, AddTrackError, AlbumError, AlbumsError, ArtistAlbumsError,
    ArtistError, ArtistsError, MusicApi, RemoveAlbumError, RemoveArtistError, RemoveTrackError,
    TrackError, TrackOrId, TracksError,
};
use moosicbox_paging::{Page, PagingRequest, PagingResponse, PagingResult};
use moosicbox_search::{
    data::AsDataValues as _, models::ApiGlobalSearchResult, populate_global_search_index,
    PopulateIndexError, RecreateIndexError,
};
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;
use tokio::sync::Mutex;

#[cfg(feature = "api")]
pub mod api;

pub mod cache;
pub mod db;
pub mod profiles;

pub mod models {
    pub use moosicbox_library_models::*;
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryArtistOrder {
    Date,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryArtistOrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Error)]
pub enum LibraryFavoriteArtistsError {
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Db(#[from] DbError),
}

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

#[derive(Debug, Error)]
pub enum LibraryAddFavoriteArtistError {
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Db(#[from] DbError),
}

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

#[derive(Debug, Error)]
pub enum LibraryRemoveFavoriteArtistError {
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Db(#[from] DbError),
}

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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryAlbumOrder {
    Date,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryAlbumOrderDirection {
    Asc,
    Desc,
}

pub fn filter_albums<'a>(
    albums: &'a [LibraryAlbum],
    request: &'a AlbumsRequest,
) -> impl Iterator<Item = &'a LibraryAlbum> {
    let albums = albums.iter().filter(|album| {
        !request.filters.as_ref().is_some_and(|x| {
            x.artist_id
                .as_ref()
                .is_some_and(|id| &Id::Number(album.artist_id) != id)
        })
    });

    #[cfg(feature = "tidal")]
    let albums = albums.filter(|#[allow(unused)] album| {
        !request.filters.as_ref().is_some_and(|x| {
            x.tidal_artist_id.as_ref().is_some_and(|id| {
                !album
                    .artist_sources
                    .iter()
                    .filter(|x| x.source == ApiSource::Tidal)
                    .map(|x| &x.id)
                    .any(|x| x == id)
            })
        })
    });

    #[cfg(feature = "qobuz")]
    let albums = albums.filter(|#[allow(unused)] album| {
        !request.filters.as_ref().is_some_and(|x| {
            x.qobuz_artist_id.as_ref().is_some_and(|id| {
                !album
                    .artist_sources
                    .iter()
                    .filter(|x| x.source == ApiSource::Qobuz)
                    .map(|x| &x.id)
                    .any(|x| x == id)
            })
        })
    });

    let albums = albums
        .filter(|album| {
            !request.sources.as_ref().is_some_and(|s| {
                !s.iter()
                    .any(|source| album.versions.iter().any(|v| v.source == (*source).into()))
            })
        })
        .filter(|album| {
            !request.filters.as_ref().is_some_and(|x| {
                x.album_type
                    .map(Into::into)
                    .is_some_and(|t| album.album_type == t)
            })
        })
        .filter(|album| {
            !request.filters.as_ref().is_some_and(|x| {
                x.name
                    .as_ref()
                    .is_some_and(|s| !album.title.to_lowercase().contains(s))
            })
        })
        .filter(|album| {
            !request.filters.as_ref().is_some_and(|x| {
                x.artist
                    .as_ref()
                    .is_some_and(|s| !&album.artist.to_lowercase().contains(s))
            })
        })
        .filter(|album| {
            !request.filters.as_ref().is_some_and(|x| {
                x.search.as_ref().is_some_and(|s| {
                    !(album.title.to_lowercase().contains(s)
                        || album.artist.to_lowercase().contains(s))
                })
            })
        });

    albums
}

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

#[derive(Debug, Error)]
pub enum LibraryFavoriteAlbumsError {
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Db(#[from] DbError),
}

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

#[derive(Debug, Error)]
pub enum LibraryAddFavoriteAlbumError {
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Db(#[from] DbError),
}

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

#[derive(Debug, Error)]
pub enum LibraryRemoveFavoriteAlbumError {
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Db(#[from] DbError),
}

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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryTrackOrder {
    Date,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryTrackOrderDirection {
    Asc,
    Desc,
}

#[derive(Debug, Error)]
pub enum LibraryFavoriteTracksError {
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Db(#[from] DbError),
}

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

#[derive(Debug, Error)]
pub enum LibraryAddFavoriteTrackError {
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Db(#[from] DbError),
}

/// # Errors
///
/// * If no user id i available for the request
/// * If the request failed
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
pub const fn add_favorite_track(
    _db: &LibraryDatabase,
    _track_id: &Id,
) -> Result<(), LibraryAddFavoriteTrackError> {
    Ok(())
}

#[derive(Debug, Error)]
pub enum LibraryRemoveFavoriteTrackError {
    #[error("No user ID available")]
    NoUserIdAvailable,
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Db(#[from] DbError),
}

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

#[derive(Debug, Error)]
pub enum LibraryArtistAlbumsError {
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Db(#[from] DbError),
}

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

#[derive(Debug, Error)]
pub enum LibraryAlbumTracksError {
    #[error("Request failed: {0:?}")]
    RequestFailed(String),
    #[error(transparent)]
    Db(#[from] DbError),
}

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

#[derive(Debug, Error)]
pub enum LibraryAlbumError {
    #[error(transparent)]
    Db(#[from] DbError),
}

/// # Errors
///
/// * If there was a database error
pub async fn album_from_source(
    db: &LibraryDatabase,
    album_id: &Id,
    source: ApiSource,
) -> Result<Option<LibraryAlbum>, LibraryAlbumError> {
    Ok(db::get_album(
        db,
        &format!("{}_id", source.to_string().to_lowercase()),
        album_id,
    )
    .await?)
}

/// # Errors
///
/// * If there was a database error
pub async fn album(
    db: &LibraryDatabase,
    album_id: &Id,
) -> Result<Option<LibraryAlbum>, LibraryAlbumError> {
    Ok(db::get_album(db, "id", album_id).await?)
}

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
    versions.sort_by(|a, b| track_source_to_u8(a.source).cmp(&track_source_to_u8(b.source)));
}

/// # Errors
///
/// * If there was a database error
pub async fn album_versions(
    db: &LibraryDatabase,
    album_id: &Id,
) -> Result<Vec<AlbumVersion>, LibraryAlbumTracksError> {
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
            continue;
        }
    }

    sort_album_versions(&mut versions);

    Ok(versions)
}

#[derive(Debug, Error)]
pub enum LibraryArtistError {
    #[error("Not found")]
    NotFound,
    #[error(transparent)]
    Db(#[from] DbError),
}

/// # Errors
///
/// * If the artist was not found
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
pub async fn artist(
    db: &LibraryDatabase,
    artist_id: &Id,
) -> Result<LibraryArtist, LibraryArtistError> {
    db::get_artist(db, "id", artist_id)
        .await?
        .ok_or(LibraryArtistError::NotFound)
}

#[derive(Debug, Error)]
pub enum LibraryTrackError {
    #[error("Not found")]
    NotFound,
    #[error(transparent)]
    Db(#[from] DbError),
}

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

#[derive(Debug, Copy, Clone, Serialize, Deserialize, EnumString, AsRefStr)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum SearchType {
    Artists,
    Albums,
    Tracks,
    Videos,
    Playlists,
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

#[derive(Debug, Copy, Clone, Serialize, Deserialize, EnumString, AsRefStr)]
#[serde(rename_all = "UPPERCASE")]
#[strum(serialize_all = "UPPERCASE")]
pub enum LibrarySearchType {
    Artists,
    Albums,
    Tracks,
    Videos,
    Playlists,
    UserProfiles,
}

#[derive(Debug, Error)]
pub enum LibrarySearchError {
    #[error(transparent)]
    Db(#[from] DbError),
}

/// # Errors
///
/// * If there was a database error
#[allow(clippy::too_many_arguments)]
pub fn search(
    _db: &LibraryDatabase,
    _query: &str,
    _offset: Option<usize>,
    _limit: Option<usize>,
    _types: &Option<Vec<LibrarySearchType>>,
) -> Result<Vec<ApiGlobalSearchResult>, LibrarySearchError> {
    let items = vec![];
    log::trace!("Received search response: {items:?}");

    Ok(items)
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryAudioQuality {
    High,
    Lossless,
    HiResLossless,
}

#[derive(Debug, Error)]
pub enum LibraryTrackFileUrlError {
    #[error("Track has no file")]
    NoFile,
    #[error(transparent)]
    LibraryTrack(#[from] LibraryTrackError),
}

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

#[derive(Debug, Error)]
pub enum TryFromAlbumTypeError {
    #[error("Unsupported AlbumType")]
    UnsupportedAlbumType,
}

impl From<LibraryFavoriteArtistsError> for ArtistsError {
    fn from(err: LibraryFavoriteArtistsError) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<LibraryArtistError> for ArtistError {
    fn from(err: LibraryArtistError) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<LibraryAddFavoriteArtistError> for AddArtistError {
    fn from(err: LibraryAddFavoriteArtistError) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<LibraryRemoveFavoriteArtistError> for RemoveArtistError {
    fn from(err: LibraryRemoveFavoriteArtistError) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<LibraryFavoriteAlbumsError> for AlbumsError {
    fn from(err: LibraryFavoriteAlbumsError) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<LibraryAlbumError> for AlbumError {
    fn from(err: LibraryAlbumError) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<LibraryArtistAlbumsError> for ArtistAlbumsError {
    fn from(err: LibraryArtistAlbumsError) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<TryFromAlbumTypeError> for ArtistAlbumsError {
    fn from(err: TryFromAlbumTypeError) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<LibraryAddFavoriteAlbumError> for AddAlbumError {
    fn from(err: LibraryAddFavoriteAlbumError) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<LibraryRemoveFavoriteAlbumError> for RemoveAlbumError {
    fn from(err: LibraryRemoveFavoriteAlbumError) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<LibraryFavoriteTracksError> for TracksError {
    fn from(err: LibraryFavoriteTracksError) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<LibraryAlbumTracksError> for TracksError {
    fn from(err: LibraryAlbumTracksError) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<LibraryTrackError> for TrackError {
    fn from(err: LibraryTrackError) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<LibraryAddFavoriteTrackError> for AddTrackError {
    fn from(err: LibraryAddFavoriteTrackError) -> Self {
        Self::Other(Box::new(err))
    }
}

impl From<LibraryRemoveFavoriteTrackError> for RemoveTrackError {
    fn from(err: LibraryRemoveFavoriteTrackError) -> Self {
        Self::Other(Box::new(err))
    }
}

#[derive(Debug, Error)]
pub enum TrackSizeError {
    #[error("Unsupported audio format: {0:?}")]
    UnsupportedFormat(AudioFormat),
    #[error("Unsupported track source: {0:?}")]
    UnsupportedSource(TrackSource),
}

#[derive(Clone)]
pub struct LibraryMusicApi {
    db: LibraryDatabase,
}

impl From<&LibraryMusicApi> for LibraryDatabase {
    fn from(value: &LibraryMusicApi) -> Self {
        value.db.clone()
    }
}

impl From<LibraryMusicApi> for LibraryDatabase {
    fn from(value: LibraryMusicApi) -> Self {
        value.db
    }
}

impl From<LibraryDatabase> for LibraryMusicApi {
    fn from(value: LibraryDatabase) -> Self {
        Self { db: value }
    }
}

impl LibraryMusicApi {
    #[must_use]
    pub const fn new(db: LibraryDatabase) -> Self {
        Self { db }
    }

    /// # Errors
    ///
    /// * If failed to get the library artist
    pub async fn library_artist(
        &self,
        artist_id: &Id,
    ) -> Result<Option<LibraryArtist>, ArtistError> {
        Ok(Some(artist(&self.db, artist_id).await?))
    }

    /// # Errors
    ///
    /// * If failed to get the library album artist
    pub async fn library_album_artist(
        &self,
        album_id: &Id,
    ) -> Result<Option<LibraryArtist>, ArtistError> {
        get_artist_by_album_id(&self.db, album_id.into())
            .await
            .map_err(|e| ArtistError::Other(e.into()))
    }

    /// # Errors
    ///
    /// * If failed to get the library album from source
    pub async fn library_album_from_source(
        &self,
        album_id: &Id,
        source: ApiSource,
    ) -> Result<Option<LibraryAlbum>, AlbumError> {
        Ok(album_from_source(&self.db, album_id, source).await?)
    }

    /// # Errors
    ///
    /// * If failed to get the library album
    pub async fn library_album(&self, album_id: &Id) -> Result<Option<LibraryAlbum>, AlbumError> {
        Ok(album(&self.db, album_id).await?)
    }

    /// # Errors
    ///
    /// * If failed to get the library album versions
    pub async fn library_album_versions(
        &self,
        album_id: &Id,
    ) -> Result<Vec<AlbumVersion>, LibraryAlbumTracksError> {
        album_versions(&self.db, album_id).await
    }

    /// # Errors
    ///
    /// * If failed to get the library albums
    pub async fn library_albums(
        &self,
        request: &AlbumsRequest,
    ) -> PagingResult<LibraryAlbum, LibraryFavoriteAlbumsError> {
        favorite_albums(&self.db, request).await
    }

    /// # Errors
    ///
    /// * If failed to get the library track
    pub async fn library_track(&self, track_id: &Id) -> Result<Option<LibraryTrack>, TrackError> {
        Ok(track(&self.db, track_id).await?)
    }

    /// # Errors
    ///
    /// * If failed to get the library album tracks
    pub async fn library_album_tracks(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<TrackOrder>,
        _order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<LibraryTrack, LibraryAlbumTracksError> {
        album_tracks(&self.db, album_id, offset, limit).await
    }
}

#[async_trait]
impl MusicApi for LibraryMusicApi {
    fn source(&self) -> ApiSource {
        ApiSource::Library
    }

    async fn artists(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<ArtistOrder>,
        order_direction: Option<ArtistOrderDirection>,
    ) -> PagingResult<Artist, ArtistsError> {
        Ok(favorite_artists(
            &self.db,
            offset,
            limit,
            order.map(Into::into),
            order_direction.map(Into::into),
        )
        .await?
        .inner_into())
    }

    async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, ArtistError> {
        Ok(self.library_artist(artist_id).await?.map(Into::into))
    }

    async fn add_artist(&self, artist_id: &Id) -> Result<(), AddArtistError> {
        Ok(add_favorite_artist(&self.db, artist_id)?)
    }

    async fn remove_artist(&self, artist_id: &Id) -> Result<(), RemoveArtistError> {
        Ok(remove_favorite_artist(&self.db, artist_id)?)
    }

    async fn album_artist(&self, album_id: &Id) -> Result<Option<Artist>, ArtistError> {
        Ok(self.library_album_artist(album_id).await?.map(Into::into))
    }

    async fn artist_cover_source(
        &self,
        artist: &Artist,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, ArtistError> {
        Ok(artist.cover.clone().map(ImageCoverSource::LocalFilePath))
    }

    async fn albums(&self, request: &AlbumsRequest) -> PagingResult<Album, AlbumsError> {
        Ok(self.library_albums(request).await?.inner_into())
    }

    async fn album(&self, album_id: &Id) -> Result<Option<Album>, AlbumError> {
        Ok(self.library_album(album_id).await?.map(Into::into))
    }

    async fn album_versions(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> PagingResult<AlbumVersion, TracksError> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(50);

        let value = self.library_album_versions(album_id).await?;

        let total = u32::try_from(value.len()).unwrap();
        let items = value
            .into_iter()
            .skip(offset as usize)
            .take(std::cmp::min(total - offset, limit) as usize)
            .map(Into::into)
            .collect();

        let page = PagingResponse::new(
            Page::WithTotal {
                items,
                offset,
                limit,
                total,
            },
            {
                let api = self.clone();
                let album_id = album_id.clone();

                move |offset, limit| {
                    let api = api.clone();
                    let album_id = album_id.clone();
                    Box::pin(async move {
                        api.album_versions(&album_id, Some(offset), Some(limit))
                            .await
                    })
                }
            },
        );

        Ok(page)
    }

    async fn artist_albums(
        &self,
        artist_id: &Id,
        album_type: Option<AlbumType>,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<AlbumOrder>,
        _order_direction: Option<AlbumOrderDirection>,
    ) -> PagingResult<Album, ArtistAlbumsError> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(100);

        Ok(if let Some(album_type) = album_type {
            artist_albums(
                &self.db,
                artist_id,
                Some(offset),
                Some(limit),
                Some(album_type.into()),
            )
            .await?
            .inner_into()
        } else {
            let pages = futures::future::join_all(
                vec![
                    LibraryAlbumType::Lp,
                    LibraryAlbumType::EpsAndSingles,
                    LibraryAlbumType::Compilations,
                ]
                .into_iter()
                .map(|album_type| {
                    artist_albums(
                        &self.db,
                        artist_id,
                        Some(offset),
                        Some(limit),
                        Some(album_type),
                    )
                }),
            )
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

            let total = pages.iter().map(|page| page.total().unwrap()).sum();

            let db = self.db.clone();
            let artist_id = artist_id.clone();

            PagingResponse {
                page: Page::WithTotal {
                    items: pages
                        .into_iter()
                        .flat_map(PagingResponse::into_items)
                        .collect::<Vec<_>>(),
                    offset,
                    limit,
                    total,
                },
                fetch: Arc::new(Mutex::new(Box::new(move |offset, limit| {
                    let db = db.clone();
                    let artist_id = artist_id.clone();

                    Box::pin(async move {
                        artist_albums(&db, &artist_id, Some(offset), Some(limit), None).await
                    })
                }))),
            }
            .inner_into()
        })
    }

    async fn add_album(&self, album_id: &Id) -> Result<(), AddAlbumError> {
        Ok(add_favorite_album(&self.db, album_id)?)
    }

    async fn remove_album(&self, album_id: &Id) -> Result<(), RemoveAlbumError> {
        Ok(remove_favorite_album(&self.db, album_id)?)
    }

    async fn album_cover_source(
        &self,
        album: &Album,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, AlbumError> {
        Ok(album.artwork.clone().map(ImageCoverSource::LocalFilePath))
    }

    async fn tracks(
        &self,
        track_ids: Option<&[Id]>,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, TracksError> {
        Ok(favorite_tracks(
            &self.db,
            track_ids,
            offset,
            limit,
            order.map(Into::into),
            order_direction.map(Into::into),
        )
        .await?
        .inner_into())
    }

    async fn album_tracks(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, TracksError> {
        Ok(self
            .library_album_tracks(album_id, offset, limit, order, order_direction)
            .await?
            .inner_into())
    }

    async fn track(&self, track_id: &Id) -> Result<Option<Track>, TrackError> {
        Ok(self.library_track(track_id).await?.map(Into::into))
    }

    async fn add_track(&self, track_id: &Id) -> Result<(), AddTrackError> {
        Ok(add_favorite_track(&self.db, track_id)?)
    }

    async fn remove_track(&self, track_id: &Id) -> Result<(), RemoveTrackError> {
        Ok(remove_favorite_track(&self.db, track_id)?)
    }

    async fn track_source(
        &self,
        track: TrackOrId,
        _quality: TrackAudioQuality,
    ) -> Result<Option<TrackSource>, TrackError> {
        let Some(track) = track.track(self).await? else {
            return Ok(None);
        };
        let mut path = if let Some(file) = &track.file {
            file.to_string()
        } else {
            return Ok(None);
        };

        static REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/mnt/(\w+)").unwrap());

        if std::env::consts::OS == "windows" {
            path = REGEX
                .replace(&path, |caps: &Captures| {
                    format!("{}:", caps[1].to_uppercase())
                })
                .replace('/', "\\");
        }

        Ok(Some(TrackSource::LocalFilePath {
            path,
            format: track.format.unwrap_or(AudioFormat::Source),
            track_id: Some(track.id.clone()),
            source: track.track_source,
        }))
    }

    async fn track_size(
        &self,
        track: TrackOrId,
        source: &TrackSource,
        quality: PlaybackQuality,
    ) -> Result<Option<u64>, TrackError> {
        log::debug!(
            "track_size: track_id={} source={source:?} quality={quality:?}",
            track.id()
        );

        if let Some(size) = db::get_track_size(&self.db, track.id(), &quality)
            .await
            .map_err(|e| TrackError::Other(Box::new(e)))?
        {
            return Ok(Some(size));
        }

        let bytes = match source {
            TrackSource::LocalFilePath { ref path, .. } => match quality.format {
                #[cfg(feature = "aac")]
                AudioFormat::Aac => {
                    let writer = moosicbox_stream_utils::ByteWriter::default();
                    moosicbox_audio_output::encoder::aac::encode_aac_spawn(path, writer.clone())
                        .await
                        .map_err(|e| TrackError::Other(Box::new(e)))?;
                    writer.bytes_written()
                }
                #[cfg(feature = "flac")]
                AudioFormat::Flac => {
                    return Err(TrackError::Other(Box::new(
                        TrackSizeError::UnsupportedFormat(quality.format),
                    )))
                }
                #[cfg(feature = "mp3")]
                AudioFormat::Mp3 => {
                    let writer = moosicbox_stream_utils::ByteWriter::default();
                    moosicbox_audio_output::encoder::mp3::encode_mp3_spawn(path, writer.clone())
                        .await
                        .map_err(|e| TrackError::Other(Box::new(e)))?;
                    writer.bytes_written()
                }
                #[cfg(feature = "opus")]
                AudioFormat::Opus => {
                    let writer = moosicbox_stream_utils::ByteWriter::default();
                    moosicbox_audio_output::encoder::opus::encode_opus_spawn(path, writer.clone())
                        .await
                        .map_err(|e| TrackError::Other(Box::new(e)))?;
                    writer.bytes_written()
                }
                AudioFormat::Source => File::open(path).unwrap().metadata().unwrap().len(),
                #[allow(unreachable_patterns)]
                _ => {
                    moosicbox_assert::die_or_panic!("Invalid library state");
                }
            },
            TrackSource::RemoteUrl { url, .. } => {
                if let Some(bytes) = get_content_length(url, None, None)
                    .await
                    .map_err(|e| TrackError::Other(Box::new(e)))?
                {
                    bytes
                } else {
                    return Ok(None);
                }
            }
        };

        db::set_track_size(
            &self.db,
            SetTrackSize {
                track_id: track.id().into(),
                quality,
                bytes: Some(Some(bytes)),
                bit_depth: Some(None),
                audio_bitrate: Some(None),
                overall_bitrate: Some(None),
                sample_rate: Some(None),
                channels: Some(None),
            },
        )
        .await
        .map_err(|e| TrackError::Other(Box::new(e)))?;

        Ok(Some(bytes))
    }
}

#[derive(Debug, Error)]
pub enum ReindexError {
    #[error(transparent)]
    Db(#[from] DbError),
    #[error(transparent)]
    RecreateIndex(#[from] RecreateIndexError),
    #[error(transparent)]
    PopulateIndex(#[from] PopulateIndexError),
}

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
    let reindex_start = std::time::SystemTime::now();

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
        .map(Into::into)
        .map(|album: Album| album.as_data_values())
        .collect::<Vec<_>>();

    populate_global_search_index(&albums, false).await?;

    let tracks = db::get_tracks(db, None)
        .await?
        .into_iter()
        .map(Into::into)
        .map(|track: Track| track.as_data_values())
        .collect::<Vec<_>>();

    populate_global_search_index(&tracks, false).await?;

    let reindex_end = std::time::SystemTime::now();
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
    use moosicbox_core::sqlite::models::AlbumSource;
    use moosicbox_music_api::models::AlbumFilters;

    use super::*;

    #[test]
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

    #[cfg(all(feature = "qobuz", feature = "tidal"))]
    #[test]
    fn filter_albums_filters_albums_of_sources_that_dont_match() {
        use moosicbox_core::sqlite::models::{AlbumVersionQuality, TrackApiSource};

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
                source: TrackApiSource::Tidal,
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
                source: TrackApiSource::Qobuz,
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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

    #[test]
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
}
