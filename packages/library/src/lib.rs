#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;

pub mod cache;
pub mod db;
pub mod models;

use std::{cmp::Ordering, fs::File, ops::Deref, sync::Arc};

use db::{get_artist_by_album_id, SetTrackSize};
use models::{LibraryAlbum, LibraryArtist, LibraryTrack};

use async_recursion::async_recursion;
use async_trait::async_trait;
use moosicbox_core::{
    sqlite::{
        db::DbError,
        models::{Album, AlbumSort, ApiSource, Artist, Id, Track},
    },
    types::{AudioFormat, PlaybackQuality},
};
use moosicbox_database::Database;
use moosicbox_files::get_content_length;
use moosicbox_music_api::{
    AddAlbumError, AddArtistError, AddTrackError, AlbumError, AlbumOrder, AlbumOrderDirection,
    AlbumType, AlbumsError, AlbumsRequest, ArtistAlbumsError, ArtistError, ArtistOrder,
    ArtistOrderDirection, ArtistsError, ImageCoverSize, ImageCoverSource, MusicApi,
    RemoveAlbumError, RemoveArtistError, RemoveTrackError, TrackAudioQuality, TrackError,
    TrackOrId, TrackOrder, TrackOrderDirection, TrackSource, TracksError,
};
use moosicbox_paging::{Page, PagingRequest, PagingResponse, PagingResult};
use moosicbox_search::{
    data::AsDataValues as _, models::ApiGlobalSearchResult, populate_global_search_index,
    PopulateIndexError, RecreateIndexError,
};
use once_cell::sync::Lazy;
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct LibraryMusicApiState(LibraryMusicApi);

impl LibraryMusicApiState {
    pub fn new(api: LibraryMusicApi) -> Self {
        Self(api)
    }
}

impl Deref for LibraryMusicApiState {
    type Target = LibraryMusicApi;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryArtistOrder {
    Date,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
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
    db: Arc<Box<dyn Database>>,
    offset: Option<u32>,
    limit: Option<u32>,
    _order: Option<LibraryArtistOrder>,
    _order_direction: Option<LibraryArtistOrderDirection>,
) -> PagingResult<LibraryArtist, LibraryFavoriteArtistsError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let items = db::get_artists(&**db).await?;
    log::trace!("Received favorite artists response: {items:?}");

    let total = items.len() as u32;

    let db = db.clone();

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
                favorite_artists(db, Some(offset), Some(limit), _order, _order_direction).await
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

#[allow(clippy::too_many_arguments)]
pub async fn add_favorite_artist(
    _db: &dyn Database,
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

#[allow(clippy::too_many_arguments)]
pub async fn remove_favorite_artist(
    _db: &dyn Database,
    _artist_id: &Id,
) -> Result<(), LibraryRemoveFavoriteArtistError> {
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryAlbumOrder {
    Date,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
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
    albums
        .iter()
        .filter(|album| {
            !request.filters.as_ref().is_some_and(|x| {
                x.artist_id
                    .as_ref()
                    .is_some_and(|id| &Id::Number(album.artist_id as u64) != id)
            })
        })
        .filter(|album| {
            !request.filters.as_ref().is_some_and(|x| {
                x.tidal_artist_id
                    .as_ref()
                    .is_some_and(|id| !album.tidal_artist_id.is_some_and(|x| &Id::Number(x) == id))
            })
        })
        .filter(|album| {
            !request.filters.as_ref().is_some_and(|x| {
                x.qobuz_artist_id
                    .as_ref()
                    .is_some_and(|id| !album.qobuz_artist_id.is_some_and(|x| &Id::Number(x) == id))
            })
        })
        .filter(|album| {
            !request.sources.as_ref().is_some_and(|s| {
                !s.iter()
                    .any(|source| album.versions.iter().any(|v| v.source == (*source).into()))
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
        })
}

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
            albums.sort_by(|a, b| a.artist.to_lowercase().cmp(&b.artist.to_lowercase()))
        }
        Some(AlbumSort::NameAsc) => {
            albums.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        }
        Some(AlbumSort::ArtistDesc) => {
            albums.sort_by(|a, b| b.artist.to_lowercase().cmp(&a.artist.to_lowercase()))
        }
        Some(AlbumSort::NameDesc) => {
            albums.sort_by(|a, b| b.title.to_lowercase().cmp(&a.title.to_lowercase()))
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
    db: Arc<Box<dyn Database>>,
    request: &AlbumsRequest,
) -> PagingResult<LibraryAlbum, LibraryFavoriteAlbumsError> {
    let albums = db::get_albums(&**db).await?;
    let items = sort_albums(filter_albums(&albums, request).collect::<Vec<_>>(), request);

    let total = items.len() as u32;
    let offset = request.page.as_ref().map(|x| x.offset).unwrap_or(0);
    let limit = request.page.as_ref().map(|x| x.limit).unwrap_or(total);

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

    let db = db.clone();
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

            Box::pin(async move { favorite_albums(db, &request).await })
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

#[allow(clippy::too_many_arguments)]
pub async fn add_favorite_album(
    _db: &dyn Database,
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

#[allow(clippy::too_many_arguments)]
pub async fn remove_favorite_album(
    _db: &dyn Database,
    _album_id: &Id,
) -> Result<(), LibraryRemoveFavoriteAlbumError> {
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryTrackOrder {
    Date,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
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
    db: Arc<Box<dyn Database>>,
    track_ids: Option<&[Id]>,
    offset: Option<u32>,
    limit: Option<u32>,
    _order: Option<LibraryTrackOrder>,
    _order_direction: Option<LibraryTrackOrderDirection>,
) -> PagingResult<LibraryTrack, LibraryFavoriteTracksError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let items = db::get_tracks(&**db, track_ids).await?;
    log::trace!("Received favorite tracks response: {items:?}");

    let total = items.len() as u32;

    Ok(PagingResponse {
        page: Page::WithTotal {
            items,
            offset,
            limit,
            total,
        },
        fetch: Arc::new(Mutex::new(Box::new({
            let db = db.clone();
            let track_ids = track_ids.map(|x| x.to_vec());

            move |offset, limit| {
                let db = db.clone();
                let track_ids = track_ids.clone();

                Box::pin(async move {
                    favorite_tracks(
                        db,
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

#[allow(clippy::too_many_arguments)]
pub async fn add_favorite_track(
    _db: &dyn Database,
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

#[allow(clippy::too_many_arguments)]
pub async fn remove_favorite_track(
    _db: &dyn Database,
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

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, Copy, Clone)]
#[serde(rename_all = "UPPERCASE")]
#[strum(serialize_all = "UPPERCASE")]
pub enum LibraryAlbumType {
    All,
    Lp,
    EpsAndSingles,
    Compilations,
}

#[allow(clippy::too_many_arguments)]
#[async_recursion]
pub async fn artist_albums(
    db: Arc<Box<dyn Database>>,
    artist_id: &Id,
    offset: Option<u32>,
    limit: Option<u32>,
    _album_type: Option<LibraryAlbumType>,
) -> PagingResult<LibraryAlbum, LibraryArtistAlbumsError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let items = db::get_artist_albums(&**db, artist_id).await?;
    log::trace!("Received artist albums response: {items:?}");

    let total = items.len() as u32;

    let db = db.clone();
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
                artist_albums(db, &artist_id, Some(offset), Some(limit), _album_type).await
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
    db: Arc<Box<dyn Database>>,
    album_id: &Id,
    offset: Option<u32>,
    limit: Option<u32>,
) -> PagingResult<LibraryTrack, LibraryAlbumTracksError> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100);

    let items = db::get_album_tracks(&**db, album_id).await?;
    log::trace!("Received album tracks response: {items:?}");

    let total = items.len() as u32;

    let db = db.clone();
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

            Box::pin(async move { album_tracks(db, &album_id, Some(offset), Some(limit)).await })
        }))),
    })
}

#[derive(Debug, Error)]
pub enum LibraryAlbumError {
    #[error(transparent)]
    Db(#[from] DbError),
}

pub async fn album_from_source(
    db: &dyn Database,
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

pub async fn album(
    db: &dyn Database,
    album_id: &Id,
) -> Result<Option<LibraryAlbum>, LibraryAlbumError> {
    Ok(db::get_album(db, "id", album_id).await?)
}

#[derive(Debug, Error)]
pub enum LibraryArtistError {
    #[error("Not found")]
    NotFound,
    #[error(transparent)]
    Db(#[from] DbError),
}

#[allow(clippy::too_many_arguments)]
pub async fn artist(
    db: &dyn Database,
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

pub async fn track(db: &dyn Database, track_id: &Id) -> Result<LibraryTrack, LibraryTrackError> {
    db::get_track(db, track_id)
        .await?
        .ok_or(LibraryTrackError::NotFound)
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
            SearchType::Artists => LibrarySearchType::Artists,
            SearchType::Albums => LibrarySearchType::Albums,
            SearchType::Tracks => LibrarySearchType::Tracks,
            SearchType::Videos => LibrarySearchType::Videos,
            SearchType::Playlists => LibrarySearchType::Playlists,
            SearchType::UserProfiles => LibrarySearchType::UserProfiles,
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

#[allow(clippy::too_many_arguments)]
pub async fn search(
    _db: &dyn Database,
    _query: &str,
    _offset: Option<usize>,
    _limit: Option<usize>,
    _types: Option<Vec<LibrarySearchType>>,
) -> Result<Vec<ApiGlobalSearchResult>, LibrarySearchError> {
    let items = vec![];
    log::trace!("Received search response: {items:?}");

    Ok(items)
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
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

pub async fn track_file_url(
    db: &dyn Database,
    _audio_quality: LibraryAudioQuality,
    track_id: &Id,
) -> Result<String, LibraryTrackFileUrlError> {
    let track = track(db, track_id).await?;
    log::trace!("Received track file url response: {track:?}");

    track.file.ok_or(LibraryTrackFileUrlError::NoFile)
}

impl From<ArtistOrder> for LibraryArtistOrder {
    fn from(value: ArtistOrder) -> Self {
        match value {
            ArtistOrder::DateAdded => LibraryArtistOrder::Date,
        }
    }
}

impl From<ArtistOrderDirection> for LibraryArtistOrderDirection {
    fn from(value: ArtistOrderDirection) -> Self {
        match value {
            ArtistOrderDirection::Ascending => LibraryArtistOrderDirection::Asc,
            ArtistOrderDirection::Descending => LibraryArtistOrderDirection::Desc,
        }
    }
}

impl From<AlbumOrder> for LibraryAlbumOrder {
    fn from(value: AlbumOrder) -> Self {
        match value {
            AlbumOrder::DateAdded => LibraryAlbumOrder::Date,
        }
    }
}

impl From<AlbumOrderDirection> for LibraryAlbumOrderDirection {
    fn from(value: AlbumOrderDirection) -> Self {
        match value {
            AlbumOrderDirection::Ascending => LibraryAlbumOrderDirection::Asc,
            AlbumOrderDirection::Descending => LibraryAlbumOrderDirection::Desc,
        }
    }
}

impl From<TrackOrder> for LibraryTrackOrder {
    fn from(value: TrackOrder) -> Self {
        match value {
            TrackOrder::DateAdded => LibraryTrackOrder::Date,
        }
    }
}

impl From<TrackOrderDirection> for LibraryTrackOrderDirection {
    fn from(value: TrackOrderDirection) -> Self {
        match value {
            TrackOrderDirection::Ascending => LibraryTrackOrderDirection::Asc,
            TrackOrderDirection::Descending => LibraryTrackOrderDirection::Desc,
        }
    }
}

#[derive(Debug, Error)]
pub enum TryFromAlbumTypeError {
    #[error("Unsupported AlbumType")]
    UnsupportedAlbumType,
}

impl TryFrom<AlbumType> for LibraryAlbumType {
    type Error = TryFromAlbumTypeError;

    fn try_from(value: AlbumType) -> Result<Self, Self::Error> {
        match value {
            AlbumType::All => Ok(LibraryAlbumType::All),
            AlbumType::Lp => Ok(LibraryAlbumType::Lp),
            AlbumType::Compilations => Ok(LibraryAlbumType::Compilations),
            AlbumType::EpsAndSingles => Ok(LibraryAlbumType::EpsAndSingles),
            _ => Err(TryFromAlbumTypeError::UnsupportedAlbumType),
        }
    }
}

impl From<LibraryFavoriteArtistsError> for ArtistsError {
    fn from(err: LibraryFavoriteArtistsError) -> Self {
        ArtistsError::Other(Box::new(err))
    }
}

impl From<LibraryArtistError> for ArtistError {
    fn from(err: LibraryArtistError) -> Self {
        ArtistError::Other(Box::new(err))
    }
}

impl From<LibraryAddFavoriteArtistError> for AddArtistError {
    fn from(err: LibraryAddFavoriteArtistError) -> Self {
        AddArtistError::Other(Box::new(err))
    }
}

impl From<LibraryRemoveFavoriteArtistError> for RemoveArtistError {
    fn from(err: LibraryRemoveFavoriteArtistError) -> Self {
        RemoveArtistError::Other(Box::new(err))
    }
}

impl From<LibraryFavoriteAlbumsError> for AlbumsError {
    fn from(err: LibraryFavoriteAlbumsError) -> Self {
        AlbumsError::Other(Box::new(err))
    }
}

impl From<LibraryAlbumError> for AlbumError {
    fn from(err: LibraryAlbumError) -> Self {
        AlbumError::Other(Box::new(err))
    }
}

impl From<LibraryArtistAlbumsError> for ArtistAlbumsError {
    fn from(err: LibraryArtistAlbumsError) -> Self {
        ArtistAlbumsError::Other(Box::new(err))
    }
}

impl From<TryFromAlbumTypeError> for ArtistAlbumsError {
    fn from(err: TryFromAlbumTypeError) -> Self {
        ArtistAlbumsError::Other(Box::new(err))
    }
}

impl From<LibraryAddFavoriteAlbumError> for AddAlbumError {
    fn from(err: LibraryAddFavoriteAlbumError) -> Self {
        AddAlbumError::Other(Box::new(err))
    }
}

impl From<LibraryRemoveFavoriteAlbumError> for RemoveAlbumError {
    fn from(err: LibraryRemoveFavoriteAlbumError) -> Self {
        RemoveAlbumError::Other(Box::new(err))
    }
}

impl From<LibraryFavoriteTracksError> for TracksError {
    fn from(err: LibraryFavoriteTracksError) -> Self {
        TracksError::Other(Box::new(err))
    }
}

impl From<LibraryAlbumTracksError> for TracksError {
    fn from(err: LibraryAlbumTracksError) -> Self {
        TracksError::Other(Box::new(err))
    }
}

impl From<LibraryTrackError> for TrackError {
    fn from(err: LibraryTrackError) -> Self {
        TrackError::Other(Box::new(err))
    }
}

impl From<LibraryAddFavoriteTrackError> for AddTrackError {
    fn from(err: LibraryAddFavoriteTrackError) -> Self {
        AddTrackError::Other(Box::new(err))
    }
}

impl From<LibraryRemoveFavoriteTrackError> for RemoveTrackError {
    fn from(err: LibraryRemoveFavoriteTrackError) -> Self {
        RemoveTrackError::Other(Box::new(err))
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
    db: Arc<Box<dyn Database>>,
}

impl LibraryMusicApi {
    pub fn new(db: Arc<Box<dyn Database>>) -> Self {
        Self { db }
    }

    pub async fn library_artist(
        &self,
        artist_id: &Id,
    ) -> Result<Option<LibraryArtist>, ArtistError> {
        Ok(Some(artist(&**self.db, artist_id).await?))
    }

    pub async fn library_album_artist(
        &self,
        album_id: &Id,
    ) -> Result<Option<LibraryArtist>, ArtistError> {
        get_artist_by_album_id(&**self.db, album_id.into())
            .await
            .map_err(|e| ArtistError::Other(e.into()))
    }

    pub async fn library_album_from_source(
        &self,
        album_id: &Id,
        source: ApiSource,
    ) -> Result<Option<LibraryAlbum>, AlbumError> {
        Ok(album_from_source(&**self.db, album_id, source).await?)
    }

    pub async fn library_album(&self, album_id: &Id) -> Result<Option<LibraryAlbum>, AlbumError> {
        Ok(album(&**self.db, album_id).await?)
    }

    pub async fn library_albums(
        &self,
        request: &AlbumsRequest,
    ) -> PagingResult<LibraryAlbum, LibraryFavoriteAlbumsError> {
        favorite_albums(self.db.clone(), request).await
    }

    pub async fn library_track(&self, track_id: &Id) -> Result<Option<LibraryTrack>, TrackError> {
        Ok(Some(track(&**self.db, track_id).await?))
    }

    pub async fn library_album_tracks(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<TrackOrder>,
        _order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<LibraryTrack, LibraryAlbumTracksError> {
        album_tracks(self.db.clone(), album_id, offset, limit).await
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
            self.db.clone(),
            offset,
            limit,
            order.map(|x| x.into()),
            order_direction.map(|x| x.into()),
        )
        .await?
        .map(|x| x.into()))
    }

    async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, ArtistError> {
        Ok(self.library_artist(artist_id).await?.map(|x| x.into()))
    }

    async fn add_artist(&self, artist_id: &Id) -> Result<(), AddArtistError> {
        Ok(add_favorite_artist(&**self.db, artist_id).await?)
    }

    async fn remove_artist(&self, artist_id: &Id) -> Result<(), RemoveArtistError> {
        Ok(remove_favorite_artist(&**self.db, artist_id).await?)
    }

    async fn album_artist(&self, album_id: &Id) -> Result<Option<Artist>, ArtistError> {
        Ok(self.library_album_artist(album_id).await?.map(|x| x.into()))
    }

    async fn artist_cover_source(
        &self,
        artist: &Artist,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, ArtistError> {
        Ok(artist
            .cover
            .as_ref()
            .cloned()
            .map(ImageCoverSource::LocalFilePath))
    }

    async fn albums(&self, request: &AlbumsRequest) -> PagingResult<Album, AlbumsError> {
        Ok(self.library_albums(request).await?.map(|x| x.into()))
    }

    async fn album(&self, album_id: &Id) -> Result<Option<Album>, AlbumError> {
        Ok(self.library_album(album_id).await?.map(|x| x.into()))
    }

    async fn artist_albums(
        &self,
        artist_id: &Id,
        album_type: AlbumType,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<AlbumOrder>,
        _order_direction: Option<AlbumOrderDirection>,
    ) -> PagingResult<Album, ArtistAlbumsError> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(100);

        if album_type == AlbumType::All {
            let pages = futures::future::join_all(
                vec![
                    LibraryAlbumType::Lp,
                    LibraryAlbumType::EpsAndSingles,
                    LibraryAlbumType::Compilations,
                ]
                .into_iter()
                .map(|album_type| {
                    artist_albums(
                        self.db.clone(),
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
            let album_type = album_type.try_into()?;

            return Ok(PagingResponse {
                page: Page::WithTotal {
                    items: pages
                        .into_iter()
                        .flat_map(|page| page.items())
                        .collect::<Vec<_>>(),
                    offset,
                    limit,
                    total,
                },
                fetch: Arc::new(Mutex::new(Box::new(move |offset, limit| {
                    let db = db.clone();
                    let artist_id = artist_id.clone();

                    Box::pin(async move {
                        artist_albums(db, &artist_id, Some(offset), Some(limit), Some(album_type))
                            .await
                    })
                }))),
            }
            .map(|item| item.into()));
        }

        Ok(artist_albums(
            self.db.clone(),
            artist_id,
            Some(offset),
            Some(limit),
            Some(album_type.try_into()?),
        )
        .await?
        .map(|x| x.into()))
    }

    async fn add_album(&self, album_id: &Id) -> Result<(), AddAlbumError> {
        Ok(add_favorite_album(&**self.db, album_id).await?)
    }

    async fn remove_album(&self, album_id: &Id) -> Result<(), RemoveAlbumError> {
        Ok(remove_favorite_album(&**self.db, album_id).await?)
    }

    async fn album_cover_source(
        &self,
        album: &Album,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, AlbumError> {
        Ok(album
            .artwork
            .as_ref()
            .cloned()
            .map(ImageCoverSource::LocalFilePath))
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
            self.db.clone(),
            track_ids,
            offset,
            limit,
            order.map(|x| x.into()),
            order_direction.map(|x| x.into()),
        )
        .await?
        .map(|x| x.into()))
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
            .map(|x| x.into()))
    }

    async fn track(&self, track_id: &Id) -> Result<Option<Track>, TrackError> {
        Ok(self.library_track(track_id).await?.map(|x| x.into()))
    }

    async fn add_track(&self, track_id: &Id) -> Result<(), AddTrackError> {
        Ok(add_favorite_track(&**self.db, track_id).await?)
    }

    async fn remove_track(&self, track_id: &Id) -> Result<(), RemoveTrackError> {
        Ok(remove_favorite_track(&**self.db, track_id).await?)
    }

    async fn track_source(
        &self,
        track: TrackOrId,
        _quality: TrackAudioQuality,
    ) -> Result<Option<TrackSource>, TrackError> {
        let track = if let Some(track) = track.track(self).await? {
            track
        } else {
            return Ok(None);
        };
        let mut path = if let Some(file) = &track.file {
            file.to_owned()
        } else {
            return Ok(None);
        };

        static REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"/mnt/(\w+)").unwrap());

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
            track_id: Some(track.id.to_owned()),
            source: track.source,
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

        if let Some(size) = db::get_track_size(&**self.db, track.id(), &quality)
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
                    moosicbox_audio_output::encoder::aac::encode_aac_spawn(
                        path.to_string(),
                        writer.clone(),
                    )
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
                    moosicbox_audio_output::encoder::mp3::encode_mp3_spawn(
                        path.to_string(),
                        writer.clone(),
                    )
                    .await
                    .map_err(|e| TrackError::Other(Box::new(e)))?;
                    writer.bytes_written()
                }
                #[cfg(feature = "opus")]
                AudioFormat::Opus => {
                    let writer = moosicbox_stream_utils::ByteWriter::default();
                    moosicbox_audio_output::encoder::opus::encode_opus_spawn(
                        path.to_string(),
                        writer.clone(),
                    )
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
            &**self.db,
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

pub async fn reindex_global_search_index(db: &dyn Database) -> Result<(), ReindexError> {
    let reindex_start = std::time::SystemTime::now();

    moosicbox_search::data::recreate_global_search_index().await?;

    let artists = db::get_artists(db)
        .await?
        .into_iter()
        .map(|x| x.into())
        .map(|artist: Artist| artist.as_data_values())
        .collect::<Vec<_>>();

    populate_global_search_index(&artists, false)?;

    let albums = db::get_albums(db)
        .await?
        .into_iter()
        .map(|x| x.into())
        .map(|album: Album| album.as_data_values())
        .collect::<Vec<_>>();

    populate_global_search_index(&albums, false)?;

    let tracks = db::get_tracks(db, None)
        .await?
        .into_iter()
        .map(|x| x.into())
        .map(|track: Track| track.as_data_values())
        .collect::<Vec<_>>();

    populate_global_search_index(&tracks, false)?;

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
    use moosicbox_core::sqlite::models::{AlbumSource, AlbumVersionQuality, TrackApiSource};
    use moosicbox_music_api::AlbumFilters;

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

    #[test]
    fn filter_albums_filters_albums_of_sources_that_dont_match() {
        let local = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "".to_string(),
            artwork: None,
            versions: vec![AlbumVersionQuality {
                source: TrackApiSource::Local,
                ..Default::default()
            }],
            ..Default::default()
        };
        let tidal = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "".to_string(),
            artwork: None,
            versions: vec![AlbumVersionQuality {
                source: TrackApiSource::Tidal,
                ..Default::default()
            }],
            ..Default::default()
        };
        let qobuz = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "".to_string(),
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
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "test".to_string(),
            artist: "".to_string(),
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
                    artist: None,
                    search: None,
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
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
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "one test two".to_string(),
            artist: "".to_string(),
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
                    artist: None,
                    search: None,
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
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
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "".to_string(),
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
                    name: None,
                    artist: Some("test".to_string()),
                    search: None,
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
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
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "".to_string(),
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
                    name: None,
                    artist: Some("test".to_string()),
                    search: None,
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
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
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "".to_string(),
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
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
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
            title: "".to_string(),
            artist: "bob".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "".to_string(),
            artist: "sally".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "".to_string(),
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
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
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
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "test".to_string(),
            artist: "".to_string(),
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
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
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
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = LibraryAlbum {
            id: 0,
            title: "sally".to_string(),
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "one test two".to_string(),
            artist: "".to_string(),
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
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
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
            artist: "".to_string(),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = LibraryAlbum {
            id: 0,
            title: "one test two".to_string(),
            artist: "".to_string(),
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
                    name: None,
                    artist: None,
                    search: Some("test".to_string()),
                    artist_id: None,
                    tidal_artist_id: None,
                    qobuz_artist_id: None,
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
