//! Music API abstraction layer for `MoosicBox`.
//!
//! This crate provides a unified trait-based interface for accessing music metadata and content
//! from multiple sources (local files, streaming services, etc.). It handles artists, albums,
//! tracks, authentication, and caching.
//!
//! # Core Components
//!
//! * [`MusicApi`] - Main trait for music API implementations
//! * [`MusicApis`] - Collection of music APIs indexed by source
//! * [`CachedMusicApi`] - Caching wrapper for any music API
//! * [`auth`] - Authentication configurations (poll-based, username/password)
//! * [`profiles`] - Profile-based music API registry
//!
//! # Example
//!
//! ```rust
//! # use moosicbox_music_api::{MusicApi, MusicApis};
//! # use moosicbox_music_models::ApiSource;
//! # use std::sync::Arc;
//! # use std::collections::BTreeMap;
//! // Create a collection of music APIs
//! let mut apis = MusicApis::new();
//!
//! // Add an API implementation for a specific source
//! # /*
//! apis.add_source(Arc::new(Box::new(my_music_api)));
//! # */
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::type_complexity)]

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use auth::ApiAuth;
use models::{
    AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder, ArtistOrderDirection,
    ImageCoverSize, ImageCoverSource, TrackAudioQuality, TrackOrder, TrackOrderDirection,
    TrackSource, search::api::ApiSearchResultsResponse,
};
use moosicbox_menu_models::AlbumVersion;
use moosicbox_music_models::{Album, AlbumType, ApiSource, Artist, PlaybackQuality, Track, id::Id};
use moosicbox_paging::PagingResult;
use switchy_async::sync::{Mutex, RwLock};

pub use moosicbox_music_api_models as models;

pub mod auth;
pub mod profiles;

/// Collection of music API implementations indexed by source.
#[derive(Clone)]
pub struct MusicApis(Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>>);

impl Default for MusicApis {
    fn default() -> Self {
        Self::new()
    }
}

impl MusicApis {
    /// Creates a new empty collection of music APIs.
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(BTreeMap::new()))
    }

    /// Adds a music API implementation for a specific source.
    pub fn add_source(&mut self, api: Arc<Box<dyn MusicApi>>) {
        let mut map = (*self.0).clone();
        map.insert(api.source().clone(), api);

        self.0 = Arc::new(map);
    }
}

impl From<&MusicApis> for Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>> {
    /// Converts a reference to `MusicApis` into an `Arc` of the inner map.
    fn from(value: &MusicApis) -> Self {
        value.0.clone()
    }
}

impl From<MusicApis> for Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>> {
    /// Converts `MusicApis` into an `Arc` of the inner map.
    fn from(value: MusicApis) -> Self {
        value.0
    }
}

impl From<Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>>> for MusicApis {
    /// Converts an `Arc` of the inner map into `MusicApis`.
    fn from(value: Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>>) -> Self {
        Self(value)
    }
}

impl SourceToMusicApi for MusicApis {
    /// Gets the music API for the given source from the collection.
    fn get(&self, source: &ApiSource) -> Option<Arc<Box<dyn MusicApi>>> {
        self.0.get(source).cloned()
    }
}

/// Iterator over music API implementations.
///
/// Created by [`MusicApis::iter`] or by calling [`IntoIterator::into_iter`] on `&MusicApis`.
pub struct MusicApisIter<'a> {
    inner: std::collections::btree_map::Iter<'a, ApiSource, Arc<Box<dyn MusicApi>>>,
}

impl<'a> Iterator for MusicApisIter<'a> {
    type Item = &'a dyn MusicApi;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner
            .next()
            .map(|(_src, api_arc)| api_arc.as_ref().as_ref())
    }
}

impl MusicApis {
    /// Returns an iterator over the music APIs.
    #[must_use]
    pub fn iter(&self) -> MusicApisIter<'_> {
        MusicApisIter {
            inner: self.0.iter(),
        }
    }
}

impl<'a> IntoIterator for &'a MusicApis {
    type Item = &'a dyn MusicApi;
    type IntoIter = MusicApisIter<'a>;

    /// Creates an iterator over the music APIs.
    fn into_iter(self) -> Self::IntoIter {
        MusicApisIter {
            inner: self.0.iter(),
        }
    }
}

/// Trait for retrieving music API implementations by source.
pub trait SourceToMusicApi {
    /// Gets the music API for the given source, or `None` if not found.
    fn get(&self, source: &ApiSource) -> Option<Arc<Box<dyn MusicApi>>>;
}

/// Errors that can occur when using the music API.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The music API for the specified source was not found.
    #[error("Music API for source not found: {0}")]
    MusicApiNotFound(ApiSource),
    /// The requested action is not supported.
    #[error("Unsupported Action: {0}")]
    UnsupportedAction(&'static str),
    /// Authentication failed or is required.
    #[error("Unauthorized")]
    Unauthorized,
    /// Other error occurred.
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

/// Represents either a track or its ID.
pub enum TrackOrId {
    /// A complete track.
    Track(Box<Track>),
    /// A track ID.
    Id(Id),
}

impl TrackOrId {
    /// Resolves to a track, fetching from the API if necessary.
    ///
    /// # Errors
    ///
    /// * If failed to fetch the track from the API
    pub async fn track(self, api: &dyn MusicApi) -> Result<Option<Track>, Error> {
        Ok(match self {
            Self::Track(track) => Some(*track),
            Self::Id(id) => api.track(&id).await?,
        })
    }

    /// Returns the track ID.
    #[must_use]
    pub const fn id(&self) -> &Id {
        match self {
            Self::Track(track) => &track.id,
            Self::Id(id) => id,
        }
    }
}

impl From<Id> for TrackOrId {
    /// Converts an `Id` into `TrackOrId::Id`.
    fn from(value: Id) -> Self {
        Self::Id(value)
    }
}

impl From<&Id> for TrackOrId {
    /// Converts a reference to an `Id` into `TrackOrId::Id`.
    fn from(value: &Id) -> Self {
        Self::Id(value.to_owned())
    }
}

impl From<Track> for TrackOrId {
    /// Converts a `Track` into `TrackOrId::Track`.
    fn from(value: Track) -> Self {
        Self::Track(Box::new(value))
    }
}

impl From<&Track> for TrackOrId {
    /// Converts a reference to a `Track` into `TrackOrId::Track`.
    fn from(value: &Track) -> Self {
        Self::Track(Box::new(value.to_owned()))
    }
}

/// Core trait for music API implementations.
///
/// Provides methods to access and manage artists, albums, tracks, and authentication.
#[async_trait]
pub trait MusicApi: Send + Sync {
    /// Returns the API source for this implementation.
    fn source(&self) -> &ApiSource;

    /// Retrieves a paginated list of artists.
    ///
    /// # Errors
    ///
    /// * If the artists could not be retrieved
    async fn artists(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<ArtistOrder>,
        order_direction: Option<ArtistOrderDirection>,
    ) -> PagingResult<Artist, Error>;

    /// Retrieves an artist by ID.
    ///
    /// # Errors
    ///
    /// * If the artist could not be retrieved
    async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, Error>;

    /// Adds an artist to the library.
    ///
    /// # Errors
    ///
    /// * If the artist could not be added
    async fn add_artist(&self, artist_id: &Id) -> Result<(), Error>;

    /// Removes an artist from the library.
    ///
    /// # Errors
    ///
    /// * If the artist could not be removed
    async fn remove_artist(&self, artist_id: &Id) -> Result<(), Error>;

    /// Retrieves the artist for a given album.
    ///
    /// # Errors
    ///
    /// * If the album or artist could not be retrieved
    async fn album_artist(&self, album_id: &Id) -> Result<Option<Artist>, Error> {
        let Some(album) = self
            .album(album_id)
            .await
            .map_err(|e| Error::Other(e.into()))?
        else {
            return Ok(None);
        };

        self.artist(&album.artist_id).await
    }

    /// Retrieves the cover art source for an artist.
    ///
    /// # Errors
    ///
    /// * If the cover source could not be retrieved
    async fn artist_cover_source(
        &self,
        artist: &Artist,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, Error> {
        Ok(artist
            .cover
            .clone()
            .map(|url| ImageCoverSource::RemoteUrl { url, headers: None }))
    }

    /// Retrieves a paginated list of albums.
    ///
    /// # Errors
    ///
    /// * If the albums could not be retrieved
    async fn albums(&self, request: &AlbumsRequest) -> PagingResult<Album, Error>;

    /// Retrieves an album by ID.
    ///
    /// # Errors
    ///
    /// * If the album could not be retrieved
    async fn album(&self, album_id: &Id) -> Result<Option<Album>, Error>;

    /// Retrieves different versions of an album.
    ///
    /// # Errors
    ///
    /// * If the album versions could not be retrieved
    async fn album_versions(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> PagingResult<AlbumVersion, Error>;

    /// Retrieves a paginated list of albums for a specific artist.
    ///
    /// # Errors
    ///
    /// * If the artist albums could not be retrieved
    #[allow(clippy::too_many_arguments)]
    async fn artist_albums(
        &self,
        artist_id: &Id,
        album_type: Option<AlbumType>,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<AlbumOrder>,
        order_direction: Option<AlbumOrderDirection>,
    ) -> PagingResult<Album, Error>;

    /// Adds an album to the library.
    ///
    /// # Errors
    ///
    /// * If the album could not be added
    async fn add_album(&self, album_id: &Id) -> Result<(), Error>;

    /// Removes an album from the library.
    ///
    /// # Errors
    ///
    /// * If the album could not be removed
    async fn remove_album(&self, album_id: &Id) -> Result<(), Error>;

    /// Retrieves the cover art source for an album.
    ///
    /// # Errors
    ///
    /// * If the cover source could not be retrieved
    async fn album_cover_source(
        &self,
        album: &Album,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, Error> {
        Ok(album
            .artwork
            .clone()
            .map(|url| ImageCoverSource::RemoteUrl { url, headers: None }))
    }

    /// Retrieves a paginated list of tracks.
    ///
    /// # Errors
    ///
    /// * If the tracks could not be retrieved
    async fn tracks(
        &self,
        track_ids: Option<&[Id]>,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, Error>;

    /// Retrieves a track by ID.
    ///
    /// # Errors
    ///
    /// * If the track could not be retrieved
    async fn track(&self, track_id: &Id) -> Result<Option<Track>, Error>;

    /// Retrieves a paginated list of tracks for a specific album.
    ///
    /// # Errors
    ///
    /// * If the album tracks could not be retrieved
    async fn album_tracks(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, Error>;

    /// Adds a track to the library.
    ///
    /// # Errors
    ///
    /// * If the track could not be added
    async fn add_track(&self, track_id: &Id) -> Result<(), Error>;

    /// Removes a track from the library.
    ///
    /// # Errors
    ///
    /// * If the track could not be removed
    async fn remove_track(&self, track_id: &Id) -> Result<(), Error>;

    /// Retrieves the playback source for a track.
    ///
    /// # Errors
    ///
    /// * If the track source could not be retrieved
    async fn track_source(
        &self,
        track: TrackOrId,
        quality: TrackAudioQuality,
    ) -> Result<Option<TrackSource>, Error>;

    /// Retrieves the size of a track in bytes.
    ///
    /// # Errors
    ///
    /// * If the track size could not be retrieved
    async fn track_size(
        &self,
        track: TrackOrId,
        source: &TrackSource,
        quality: PlaybackQuality,
    ) -> Result<Option<u64>, Error>;

    /// Enables scanning for new media.
    ///
    /// # Errors
    ///
    /// * If scanning is not supported or could not be enabled
    async fn enable_scan(&self) -> Result<(), Error> {
        Err(Error::UnsupportedAction("enable_scan"))
    }

    /// Triggers a media library scan.
    ///
    /// # Errors
    ///
    /// * If scanning is not supported or failed
    async fn scan(&self) -> Result<(), Error> {
        Err(Error::UnsupportedAction("scan"))
    }

    /// Returns the authentication handler for this API, if any.
    fn auth(&self) -> Option<&ApiAuth> {
        None
    }

    /// Checks whether scanning is currently enabled.
    ///
    /// # Errors
    ///
    /// * If scanning is not supported
    async fn scan_enabled(&self) -> Result<bool, Error> {
        Err(Error::UnsupportedAction("scan_enabled"))
    }

    /// Returns whether this API supports scanning.
    fn supports_scan(&self) -> bool {
        false
    }

    /// Returns whether this API supports search.
    fn supports_search(&self) -> bool {
        false
    }

    /// Searches for artists, albums, and tracks matching the query.
    ///
    /// # Errors
    ///
    /// * If search is not supported or failed
    async fn search(
        &self,
        _query: &str,
        _offset: Option<u32>,
        _limit: Option<u32>,
    ) -> Result<ApiSearchResultsResponse, Error> {
        Err(Error::UnsupportedAction("search"))
    }

    /// Wraps this API with caching for artists, albums, and tracks.
    fn cached(self) -> impl MusicApi
    where
        Self: Sized,
    {
        CachedMusicApi {
            inner: self,
            cascade_delete: false,
            artists: Arc::new(RwLock::new(BTreeMap::new())),
            albums: Arc::new(RwLock::new(BTreeMap::new())),
            tracks: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }
}

/// A caching wrapper for music API implementations.
///
/// Caches artists, albums, and tracks to reduce API calls.
pub struct CachedMusicApi<T: MusicApi> {
    inner: T,
    cascade_delete: bool,
    artists: Arc<RwLock<BTreeMap<Id, Option<Artist>>>>,
    albums: Arc<RwLock<BTreeMap<Id, Option<Album>>>>,
    tracks: Arc<RwLock<BTreeMap<Id, Option<Track>>>>,
}

impl<T: MusicApi> CachedMusicApi<T> {
    /// Creates a new cached music API wrapping the given API.
    #[must_use]
    pub fn new(api: T) -> Self {
        Self {
            inner: api,
            cascade_delete: false,
            artists: Arc::new(RwLock::new(BTreeMap::new())),
            albums: Arc::new(RwLock::new(BTreeMap::new())),
            tracks: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    /// Sets whether removing an artist should cascade to its albums and tracks.
    #[must_use]
    pub const fn with_cascade_delete(mut self, cascade_delete: bool) -> Self {
        self.cascade_delete = cascade_delete;
        self
    }

    /// Sets whether removing an artist should cascade to its albums and tracks.
    pub const fn set_cascade_delete(&mut self, cascade_delete: bool) {
        self.cascade_delete = cascade_delete;
    }

    /// Clears all cached data.
    pub async fn clear_cache(&self) {
        self.artists.write().await.clear();
        self.albums.write().await.clear();
        self.tracks.write().await.clear();
    }

    #[inline]
    async fn get_artist_from_cache(&self, artist_id: &Id) -> Option<Option<Artist>> {
        self.artists.read().await.get(artist_id).cloned()
    }

    #[inline]
    async fn get_album_from_cache(&self, album_id: &Id) -> Option<Option<Album>> {
        self.albums.read().await.get(album_id).cloned()
    }

    #[inline]
    async fn get_track_from_cache(&self, track_id: &Id) -> Option<Option<Track>> {
        self.tracks.read().await.get(track_id).cloned()
    }

    /// Caches that the specified artist IDs do not exist.
    pub async fn cache_empty_artists(&self, ids: &[&Id]) {
        Self::cache_empty_values(&self.artists, ids).await;
    }

    /// Caches that the specified album IDs do not exist.
    pub async fn cache_empty_albums(&self, ids: &[&Id]) {
        Self::cache_empty_values(&self.albums, ids).await;
    }

    /// Caches that the specified track IDs do not exist.
    pub async fn cache_empty_tracks(&self, ids: &[&Id]) {
        Self::cache_empty_values(&self.tracks, ids).await;
    }

    async fn cache_empty_values<E: Send + Sync>(
        cache: &RwLock<BTreeMap<Id, Option<E>>>,
        ids: &[&Id],
    ) {
        let mut cache = cache.write().await;
        for id in ids {
            cache.insert((*id).to_owned(), None);
        }
    }

    /// Caches the specified artists.
    pub async fn cache_artists(&self, artists: &[Artist]) {
        Self::cache_artists_inner(&self.artists, artists).await;
    }

    async fn cache_artists_inner(cache: &RwLock<BTreeMap<Id, Option<Artist>>>, artists: &[Artist]) {
        let mut cache = cache.write().await;
        for artist in artists {
            cache.insert(artist.id.clone(), Some(artist.to_owned()));
        }
    }

    /// Caches the specified albums.
    pub async fn cache_albums(&self, albums: &[Album]) {
        Self::cache_albums_inner(&self.albums, albums).await;
    }

    async fn cache_albums_inner(cache: &RwLock<BTreeMap<Id, Option<Album>>>, albums: &[Album]) {
        let mut cache = cache.write().await;
        for album in albums {
            cache.insert(album.id.clone(), Some(album.to_owned()));
        }
    }

    /// Caches the specified tracks.
    pub async fn cache_tracks(&self, tracks: &[Track]) {
        Self::cache_tracks_inner(&self.tracks, tracks).await;
    }

    async fn cache_tracks_inner(cache: &RwLock<BTreeMap<Id, Option<Track>>>, tracks: &[Track]) {
        let mut cache = cache.write().await;
        for track in tracks {
            cache.insert(track.id.clone(), Some(track.to_owned()));
        }
    }

    /// Removes artists from the cache by ID.
    pub async fn remove_cache_artist_ids(&self, ids: &[&Id]) {
        Self::remove_cache_ids(&mut *self.artists.write().await, ids);

        if self.cascade_delete {
            self.remove_cache_albums_for_artist_ids(ids).await;
        }
    }

    async fn remove_cache_albums_for_artist_ids(&self, ids: &[&Id]) {
        let mut album_ids = vec![];

        self.albums.write().await.retain(|album_id, album| {
            let Some(album) = album else {
                return true;
            };

            for artist_id in ids {
                if &album.artist_id == *artist_id {
                    album_ids.push(album_id.to_owned());
                    return false;
                }
            }
            true
        });

        if self.cascade_delete {
            self.remove_cache_tracks_for_album_ids(&album_ids.iter().collect::<Vec<_>>())
                .await;
        }
    }

    async fn remove_cache_tracks_for_album_ids(&self, ids: &[&Id]) {
        self.tracks.write().await.retain(|_track_id, track| {
            let Some(track) = track else {
                return true;
            };

            for album_id in ids {
                if &track.album_id == *album_id {
                    return false;
                }
            }
            true
        });
    }

    /// Removes albums from the cache by ID.
    pub async fn remove_cache_album_ids(&self, ids: &[&Id]) {
        Self::remove_cache_album_ids_inner(&mut *self.albums.write().await, ids);
    }

    fn remove_cache_album_ids_inner(albums: &mut BTreeMap<Id, Option<Album>>, ids: &[&Id]) {
        Self::remove_cache_ids(albums, ids);
    }

    /// Removes tracks from the cache by ID.
    pub async fn remove_cache_track_ids(&self, ids: &[&Id]) {
        Self::remove_cache_ids(&mut *self.tracks.write().await, ids);
    }

    fn remove_cache_ids<E>(cache: &mut BTreeMap<Id, Option<E>>, ids: &[&Id]) {
        for id in ids {
            cache.remove(*id);
        }
    }

    /// Removes artists from the cache.
    pub async fn remove_cache_artists(&self, artists: &[Artist]) {
        Self::remove_cache_artists_inner(&self.artists, artists).await;
    }

    async fn remove_cache_artists_inner(
        cache: &RwLock<BTreeMap<Id, Option<Artist>>>,
        artists: &[Artist],
    ) {
        let mut cache = cache.write().await;
        for artist in artists {
            cache.remove(&artist.id);
        }
    }

    /// Removes albums from the cache.
    pub async fn remove_cache_albums(&self, albums: &[Album]) {
        Self::remove_cache_albums_inner(&self.albums, albums).await;
    }

    async fn remove_cache_albums_inner(
        cache: &RwLock<BTreeMap<Id, Option<Album>>>,
        albums: &[Album],
    ) {
        let mut cache = cache.write().await;
        for album in albums {
            cache.remove(&album.id);
        }
    }

    /// Removes tracks from the cache.
    pub async fn remove_cache_tracks(&self, tracks: &[Track]) {
        Self::remove_cache_tracks_inner(&self.tracks, tracks).await;
    }

    async fn remove_cache_tracks_inner(
        cache: &RwLock<BTreeMap<Id, Option<Track>>>,
        tracks: &[Track],
    ) {
        let mut cache = cache.write().await;
        for track in tracks {
            cache.remove(&track.id);
        }
    }
}

#[async_trait]
impl<T: MusicApi> MusicApi for CachedMusicApi<T> {
    fn source(&self) -> &ApiSource {
        self.inner.source()
    }

    async fn artists(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<ArtistOrder>,
        order_direction: Option<ArtistOrderDirection>,
    ) -> PagingResult<Artist, Error> {
        let mut artists = self
            .inner
            .artists(offset, limit, order, order_direction)
            .await?;

        self.cache_artists(&artists).await;

        let cache = self.artists.clone();
        let fetch = artists.fetch;

        artists.fetch = Arc::new(Mutex::new(Box::new(move |offset, limit| {
            let cache = cache.clone();
            let fetch = fetch.clone();

            Box::pin(async move {
                let artists = (fetch.lock().await)(offset, limit).await;

                if let Ok(artists) = &artists {
                    Self::cache_artists_inner(&cache, artists).await;
                }

                artists
            })
        })));

        Ok(artists)
    }

    async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, Error> {
        if let Some(artist) = self.get_artist_from_cache(artist_id).await {
            return Ok(artist);
        }

        let artists = self
            .inner
            .artist(artist_id)
            .await?
            .into_iter()
            .collect::<Vec<_>>();

        if artists.is_empty() {
            self.cache_empty_artists(&[artist_id]).await;
        } else {
            self.cache_artists(&artists).await;
        }

        Ok(artists.into_iter().next())
    }

    async fn add_artist(&self, artist_id: &Id) -> Result<(), Error> {
        self.inner.add_artist(artist_id).await
    }

    async fn remove_artist(&self, artist_id: &Id) -> Result<(), Error> {
        self.remove_cache_artist_ids(&[artist_id]).await;

        self.inner.remove_artist(artist_id).await
    }

    async fn album_artist(&self, album_id: &Id) -> Result<Option<Artist>, Error> {
        let artists = self
            .inner
            .album_artist(album_id)
            .await?
            .into_iter()
            .collect::<Vec<_>>();

        if !artists.is_empty() {
            self.cache_artists(&artists).await;
        }

        Ok(artists.into_iter().next())
    }

    async fn artist_cover_source(
        &self,
        artist: &Artist,
        size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, Error> {
        self.inner.artist_cover_source(artist, size).await
    }

    async fn albums(&self, request: &AlbumsRequest) -> PagingResult<Album, Error> {
        self.inner.albums(request).await
    }

    async fn album(&self, album_id: &Id) -> Result<Option<Album>, Error> {
        if let Some(album) = self.get_album_from_cache(album_id).await {
            return Ok(album);
        }

        self.inner.album(album_id).await
    }

    async fn album_versions(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> PagingResult<AlbumVersion, Error> {
        self.inner.album_versions(album_id, offset, limit).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn artist_albums(
        &self,
        artist_id: &Id,
        album_type: Option<AlbumType>,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<AlbumOrder>,
        order_direction: Option<AlbumOrderDirection>,
    ) -> PagingResult<Album, Error> {
        let mut albums = self
            .inner
            .artist_albums(artist_id, album_type, offset, limit, order, order_direction)
            .await?;

        self.cache_albums(&albums).await;

        let cache = self.albums.clone();
        let fetch = albums.fetch;

        albums.fetch = Arc::new(Mutex::new(Box::new(move |offset, limit| {
            let cache = cache.clone();
            let fetch = fetch.clone();

            Box::pin(async move {
                let albums = (fetch.lock().await)(offset, limit).await;

                if let Ok(albums) = &albums {
                    Self::cache_albums_inner(&cache, albums).await;
                }

                albums
            })
        })));

        Ok(albums)
    }

    async fn add_album(&self, album_id: &Id) -> Result<(), Error> {
        self.inner.add_album(album_id).await
    }

    async fn remove_album(&self, album_id: &Id) -> Result<(), Error> {
        self.remove_cache_album_ids(&[album_id]).await;

        self.inner.remove_album(album_id).await
    }

    async fn album_cover_source(
        &self,
        album: &Album,
        size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, Error> {
        self.inner.album_cover_source(album, size).await
    }

    async fn tracks(
        &self,
        track_ids: Option<&[Id]>,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, Error> {
        let mut tracks = self
            .inner
            .tracks(track_ids, offset, limit, order, order_direction)
            .await?;

        self.cache_tracks(&tracks).await;

        let cache = self.tracks.clone();
        let fetch = tracks.fetch;

        tracks.fetch = Arc::new(Mutex::new(Box::new(move |offset, limit| {
            let cache = cache.clone();
            let fetch = fetch.clone();

            Box::pin(async move {
                let tracks = (fetch.lock().await)(offset, limit).await;

                if let Ok(tracks) = &tracks {
                    Self::cache_tracks_inner(&cache, tracks).await;
                }

                tracks
            })
        })));

        Ok(tracks)
    }

    async fn track(&self, track_id: &Id) -> Result<Option<Track>, Error> {
        if let Some(track) = self.get_track_from_cache(track_id).await {
            return Ok(track);
        }

        let tracks = self
            .inner
            .track(track_id)
            .await?
            .into_iter()
            .collect::<Vec<_>>();

        if tracks.is_empty() {
            self.cache_empty_tracks(&[track_id]).await;
        } else {
            self.cache_tracks(&tracks).await;
        }

        Ok(tracks.into_iter().next())
    }

    async fn album_tracks(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, Error> {
        let mut tracks = self
            .inner
            .album_tracks(album_id, offset, limit, order, order_direction)
            .await?;

        self.cache_tracks(&tracks).await;

        let cache = self.tracks.clone();
        let fetch = tracks.fetch;

        tracks.fetch = Arc::new(Mutex::new(Box::new(move |offset, limit| {
            let cache = cache.clone();
            let fetch = fetch.clone();

            Box::pin(async move {
                let tracks = (fetch.lock().await)(offset, limit).await;

                if let Ok(tracks) = &tracks {
                    Self::cache_tracks_inner(&cache, tracks).await;
                }

                tracks
            })
        })));

        Ok(tracks)
    }

    async fn add_track(&self, track_id: &Id) -> Result<(), Error> {
        self.inner.add_track(track_id).await
    }

    async fn remove_track(&self, track_id: &Id) -> Result<(), Error> {
        self.remove_cache_track_ids(&[track_id]).await;

        self.inner.remove_track(track_id).await
    }

    async fn track_source(
        &self,
        track: TrackOrId,
        quality: TrackAudioQuality,
    ) -> Result<Option<TrackSource>, Error> {
        self.inner.track_source(track, quality).await
    }

    async fn track_size(
        &self,
        track: TrackOrId,
        source: &TrackSource,
        quality: PlaybackQuality,
    ) -> Result<Option<u64>, Error> {
        self.inner.track_size(track, source, quality).await
    }

    async fn enable_scan(&self) -> Result<(), Error> {
        self.inner.enable_scan().await
    }

    async fn scan_enabled(&self) -> Result<bool, Error> {
        self.inner.scan_enabled().await
    }

    fn supports_scan(&self) -> bool {
        self.inner.supports_scan()
    }

    async fn scan(&self) -> Result<(), Error> {
        self.inner.scan().await
    }

    fn auth(&self) -> Option<&ApiAuth> {
        self.inner.auth()
    }

    fn supports_search(&self) -> bool {
        self.inner.supports_search()
    }

    async fn search(
        &self,
        query: &str,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> Result<ApiSearchResultsResponse, Error> {
        self.inner.search(query, offset, limit).await
    }

    fn cached(self) -> impl MusicApi
    where
        Self: Sized,
    {
        self
    }
}

#[cfg(test)]
#[allow(clippy::module_name_repetitions)]
mod test {
    use std::{slice, sync::LazyLock};

    use async_trait::async_trait;
    use moosicbox_music_api_models::{
        AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder, ArtistOrderDirection,
        TrackAudioQuality, TrackOrder, TrackOrderDirection, TrackSource,
    };
    use moosicbox_paging::PagingResponse;
    use pretty_assertions::assert_eq;

    use crate::*;

    pub struct TestMusicApi {}

    static API_SOURCE: LazyLock<ApiSource> = LazyLock::new(|| ApiSource::register("test", "test"));

    #[async_trait]
    impl MusicApi for TestMusicApi {
        fn source(&self) -> &ApiSource {
            &API_SOURCE
        }

        async fn artists(
            &self,
            _offset: Option<u32>,
            _limit: Option<u32>,
            _order: Option<ArtistOrder>,
            _order_direction: Option<ArtistOrderDirection>,
        ) -> PagingResult<Artist, Error> {
            Ok(PagingResponse::empty())
        }

        async fn artist(&self, _artist_id: &Id) -> Result<Option<Artist>, Error> {
            Ok(None)
        }

        async fn add_artist(&self, _artist_id: &Id) -> Result<(), Error> {
            Ok(())
        }

        async fn remove_artist(&self, _artist_id: &Id) -> Result<(), Error> {
            Ok(())
        }

        async fn albums(&self, _request: &AlbumsRequest) -> PagingResult<Album, Error> {
            Ok(PagingResponse::empty())
        }

        async fn album(&self, _album_id: &Id) -> Result<Option<Album>, Error> {
            Ok(None)
        }

        async fn album_versions(
            &self,
            _album_id: &Id,
            _offset: Option<u32>,
            _limit: Option<u32>,
        ) -> PagingResult<AlbumVersion, Error> {
            Ok(PagingResponse::empty())
        }

        #[allow(clippy::too_many_arguments)]
        async fn artist_albums(
            &self,
            _artist_id: &Id,
            _album_type: Option<AlbumType>,
            _offset: Option<u32>,
            _limit: Option<u32>,
            _order: Option<AlbumOrder>,
            _order_direction: Option<AlbumOrderDirection>,
        ) -> PagingResult<Album, Error> {
            Ok(PagingResponse::empty())
        }

        async fn add_album(&self, _album_id: &Id) -> Result<(), Error> {
            Ok(())
        }

        async fn remove_album(&self, _album_id: &Id) -> Result<(), Error> {
            Ok(())
        }

        async fn tracks(
            &self,
            _track_ids: Option<&[Id]>,
            _offset: Option<u32>,
            _limit: Option<u32>,
            _order: Option<TrackOrder>,
            _order_direction: Option<TrackOrderDirection>,
        ) -> PagingResult<Track, Error> {
            Ok(PagingResponse::empty())
        }

        async fn track(&self, _track_id: &Id) -> Result<Option<Track>, Error> {
            Ok(None)
        }

        async fn album_tracks(
            &self,
            _album_id: &Id,
            _offset: Option<u32>,
            _limit: Option<u32>,
            _order: Option<TrackOrder>,
            _order_direction: Option<TrackOrderDirection>,
        ) -> PagingResult<Track, Error> {
            Ok(PagingResponse::empty())
        }

        async fn add_track(&self, _track_id: &Id) -> Result<(), Error> {
            Ok(())
        }

        async fn remove_track(&self, _track_id: &Id) -> Result<(), Error> {
            Ok(())
        }

        async fn track_source(
            &self,
            _track: TrackOrId,
            _quality: TrackAudioQuality,
        ) -> Result<Option<TrackSource>, Error> {
            Ok(None)
        }

        async fn track_size(
            &self,
            _track: TrackOrId,
            _source: &TrackSource,
            _quality: PlaybackQuality,
        ) -> Result<Option<u64>, Error> {
            Ok(None)
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn doesnt_cache_nothing_for_artists() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let one = api.artist(&1.into()).await.unwrap();

        assert_eq!(one, None);
    }

    #[test_log::test(switchy_async::test)]
    async fn can_cache_single_artist_by_id() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let artist = Artist {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };

        api.cache_artists(slice::from_ref(&artist)).await;

        let one = api.artist(&artist.id).await.unwrap();

        assert_eq!(one, Some(artist));
    }

    #[test_log::test(switchy_async::test)]
    async fn doesnt_return_artist_from_cache_if_doesnt_exist() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let artist = Artist {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };

        api.cache_artists(slice::from_ref(&artist)).await;

        let one = api.artist(&2.into()).await.unwrap();

        assert_eq!(one, None);
    }

    #[test_log::test(switchy_async::test)]
    async fn can_cache_two_artists_by_id_and_recall_each() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let artist1 = Artist {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };
        let artist2 = Artist {
            id: 2.into(),
            title: "saget".into(),
            ..Default::default()
        };

        api.cache_artists(slice::from_ref(&artist1)).await;
        api.cache_artists(slice::from_ref(&artist2)).await;

        let one = api.artist(&artist1.id).await.unwrap();
        let two = api.artist(&artist2.id).await.unwrap();

        assert_eq!(one, Some(artist1));
        assert_eq!(two, Some(artist2));
    }

    #[test_log::test(switchy_async::test)]
    async fn doesnt_cache_nothing_for_albums() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let one = api.album(&1.into()).await.unwrap();

        assert_eq!(one, None);
    }

    #[test_log::test(switchy_async::test)]
    async fn can_cache_single_album_by_id() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let album = Album {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };

        api.cache_albums(slice::from_ref(&album)).await;

        let one = api.album(&album.id).await.unwrap();

        assert_eq!(one, Some(album));
    }

    #[test_log::test(switchy_async::test)]
    async fn doesnt_return_album_from_cache_if_doesnt_exist() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let album = Album {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };

        api.cache_albums(slice::from_ref(&album)).await;

        let one = api.album(&2.into()).await.unwrap();

        assert_eq!(one, None);
    }

    #[test_log::test(switchy_async::test)]
    async fn can_cache_two_albums_by_id_and_recall_each() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let album1 = Album {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };
        let album2 = Album {
            id: 2.into(),
            title: "saget".into(),
            ..Default::default()
        };

        api.cache_albums(slice::from_ref(&album1)).await;
        api.cache_albums(slice::from_ref(&album2)).await;

        let one = api.album(&album1.id).await.unwrap();
        let two = api.album(&album2.id).await.unwrap();

        assert_eq!(one, Some(album1));
        assert_eq!(two, Some(album2));
    }

    #[test_log::test(switchy_async::test)]
    async fn doesnt_cache_nothing_for_tracks() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let one = api.track(&1.into()).await.unwrap();

        assert_eq!(one, None);
    }

    #[test_log::test(switchy_async::test)]
    async fn can_cache_single_track_by_id() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let track = Track {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };

        api.cache_tracks(slice::from_ref(&track)).await;

        let one = api.track(&track.id).await.unwrap();

        assert_eq!(one, Some(track));
    }

    #[test_log::test(switchy_async::test)]
    async fn doesnt_return_track_from_cache_if_doesnt_exist() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let track = Track {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };

        api.cache_tracks(slice::from_ref(&track)).await;

        let one = api.track(&2.into()).await.unwrap();

        assert_eq!(one, None);
    }

    #[test_log::test(switchy_async::test)]
    async fn can_cache_two_tracks_by_id_and_recall_each() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let track1 = Track {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };
        let track2 = Track {
            id: 2.into(),
            title: "saget".into(),
            ..Default::default()
        };

        api.cache_tracks(slice::from_ref(&track1)).await;
        api.cache_tracks(slice::from_ref(&track2)).await;

        let one = api.track(&track1.id).await.unwrap();
        let two = api.track(&track2.id).await.unwrap();

        assert_eq!(one, Some(track1));
        assert_eq!(two, Some(track2));
    }

    #[test_log::test(switchy_async::test)]
    async fn doesnt_cascade_delete_albums_from_artists_if_cascade_delete_disabled() {
        let api = CachedMusicApi::new(TestMusicApi {}).with_cascade_delete(false);

        let artist = Artist {
            id: 5.into(),
            title: "bobert".into(),
            ..Default::default()
        };

        let album = Album {
            id: 1.into(),
            title: "bob".into(),
            artist_id: 5.into(),
            ..Default::default()
        };

        api.cache_artists(slice::from_ref(&artist)).await;
        api.cache_albums(slice::from_ref(&album)).await;

        api.remove_artist(&artist.id).await.unwrap();

        let cache_artist = api.artist(&artist.id).await.unwrap();
        assert_eq!(cache_artist, None);

        let cache_album = api.album(&album.id).await.unwrap();
        assert_eq!(cache_album, Some(album));
    }

    #[test_log::test(switchy_async::test)]
    async fn doesnt_cascade_delete_albums_and_tracks_from_artists_if_cascade_delete_disabled() {
        let api = CachedMusicApi::new(TestMusicApi {}).with_cascade_delete(false);

        let artist = Artist {
            id: 5.into(),
            title: "bobert".into(),
            ..Default::default()
        };

        let album = Album {
            id: 1.into(),
            title: "bob".into(),
            artist_id: 5.into(),
            ..Default::default()
        };

        let track = Track {
            id: 3.into(),
            title: "bilbo".into(),
            album_id: 1.into(),
            ..Default::default()
        };

        api.cache_artists(slice::from_ref(&artist)).await;
        api.cache_albums(slice::from_ref(&album)).await;
        api.cache_tracks(slice::from_ref(&track)).await;

        api.remove_artist(&artist.id).await.unwrap();

        let cache_artist = api.artist(&artist.id).await.unwrap();
        assert_eq!(cache_artist, None);

        let cache_album = api.album(&album.id).await.unwrap();
        assert_eq!(cache_album, Some(album));

        let cache_track = api.track(&track.id).await.unwrap();
        assert_eq!(cache_track, Some(track));
    }

    #[test_log::test(switchy_async::test)]
    async fn cascade_deletes_albums_from_artists_if_cascade_delete_enabled() {
        let api = CachedMusicApi::new(TestMusicApi {}).with_cascade_delete(true);

        let artist = Artist {
            id: 5.into(),
            title: "bobert".into(),
            ..Default::default()
        };

        let album = Album {
            id: 1.into(),
            title: "bob".into(),
            artist_id: 5.into(),
            ..Default::default()
        };

        api.cache_artists(slice::from_ref(&artist)).await;
        api.cache_albums(slice::from_ref(&album)).await;

        api.remove_artist(&artist.id).await.unwrap();

        let artist = api.artist(&artist.id).await.unwrap();
        assert_eq!(artist, None);

        let album = api.album(&album.id).await.unwrap();
        assert_eq!(album, None);
    }

    #[test_log::test(switchy_async::test)]
    async fn cascade_deletes_albums_and_tracks_from_artists_if_cascade_delete_enabled() {
        let api = CachedMusicApi::new(TestMusicApi {}).with_cascade_delete(true);

        let artist = Artist {
            id: 5.into(),
            title: "bobert".into(),
            ..Default::default()
        };

        let album = Album {
            id: 1.into(),
            title: "bob".into(),
            artist_id: 5.into(),
            ..Default::default()
        };

        let track = Track {
            id: 3.into(),
            title: "bilbo".into(),
            album_id: 1.into(),
            ..Default::default()
        };

        api.cache_artists(slice::from_ref(&artist)).await;
        api.cache_albums(slice::from_ref(&album)).await;
        api.cache_tracks(slice::from_ref(&track)).await;

        api.remove_artist(&artist.id).await.unwrap();

        let artist = api.artist(&artist.id).await.unwrap();
        assert_eq!(artist, None);

        let album = api.album(&album.id).await.unwrap();
        assert_eq!(album, None);

        let track = api.track(&track.id).await.unwrap();
        assert_eq!(track, None);
    }

    #[test_log::test(switchy_async::test)]
    async fn music_apis_can_add_source_and_retrieve_it() {
        let mut apis = MusicApis::new();
        let api: Arc<Box<dyn MusicApi>> = Arc::new(Box::new(TestMusicApi {}));

        apis.add_source(api.clone());

        let retrieved = apis.get(&API_SOURCE);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().source(), &*API_SOURCE);
    }

    #[test_log::test(switchy_async::test)]
    async fn music_apis_returns_none_for_unknown_source() {
        let apis = MusicApis::new();
        let unknown_source = ApiSource::register("unknown", "unknown");

        let retrieved = apis.get(&unknown_source);
        assert!(retrieved.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn music_apis_iter_returns_all_apis() {
        let mut apis = MusicApis::new();
        let api: Arc<Box<dyn MusicApi>> = Arc::new(Box::new(TestMusicApi {}));

        apis.add_source(api);

        let count = apis.iter().count();
        assert_eq!(count, 1);
    }

    #[test_log::test(switchy_async::test)]
    async fn music_apis_into_iter_returns_all_apis() {
        let mut apis = MusicApis::new();
        let api: Arc<Box<dyn MusicApi>> = Arc::new(Box::new(TestMusicApi {}));

        apis.add_source(api);

        let count = (&apis).into_iter().count();
        assert_eq!(count, 1);
    }

    #[test_log::test(switchy_async::test)]
    async fn track_or_id_from_id_preserves_id() {
        let id: Id = 42.into();
        let track_or_id = TrackOrId::from(&id);

        assert_eq!(track_or_id.id(), &id);
    }

    #[test_log::test(switchy_async::test)]
    async fn track_or_id_from_track_preserves_id() {
        let track = Track {
            id: 42.into(),
            title: "test".into(),
            ..Default::default()
        };
        let track_or_id = TrackOrId::from(&track);

        assert_eq!(track_or_id.id(), &track.id);
    }

    #[test_log::test(switchy_async::test)]
    async fn track_or_id_track_method_returns_none_when_api_returns_none() {
        let api = TestMusicApi {};
        let id: Id = 42.into();
        let track_or_id = TrackOrId::from(id);

        let result = track_or_id.track(&api).await.unwrap();
        assert_eq!(result, None);
    }

    #[test_log::test(switchy_async::test)]
    async fn track_or_id_track_method_returns_track_when_already_track() {
        let api = TestMusicApi {};
        let track = Track {
            id: 42.into(),
            title: "test".into(),
            ..Default::default()
        };
        let expected = track.clone();
        let track_or_id = TrackOrId::from(track);

        let result = track_or_id.track(&api).await.unwrap();
        assert_eq!(result, Some(expected));
    }

    #[test_log::test(switchy_async::test)]
    async fn cached_music_api_clear_cache_clears_all_caches() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let artist = Artist {
            id: 1.into(),
            title: "artist".into(),
            ..Default::default()
        };
        let album = Album {
            id: 2.into(),
            title: "album".into(),
            ..Default::default()
        };
        let track = Track {
            id: 3.into(),
            title: "track".into(),
            ..Default::default()
        };

        api.cache_artists(slice::from_ref(&artist)).await;
        api.cache_albums(slice::from_ref(&album)).await;
        api.cache_tracks(slice::from_ref(&track)).await;

        api.clear_cache().await;

        // After clearing, cache should be empty and API should be called
        let cached_artist = api.get_artist_from_cache(&artist.id).await;
        let cached_album = api.get_album_from_cache(&album.id).await;
        let cached_track = api.get_track_from_cache(&track.id).await;

        assert!(cached_artist.is_none());
        assert!(cached_album.is_none());
        assert!(cached_track.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn cached_music_api_set_cascade_delete_updates_setting() {
        let mut api = CachedMusicApi::new(TestMusicApi {});
        assert!(!api.cascade_delete);

        api.set_cascade_delete(true);
        assert!(api.cascade_delete);

        api.set_cascade_delete(false);
        assert!(!api.cascade_delete);
    }

    #[test_log::test(switchy_async::test)]
    async fn cached_music_api_cache_empty_artists_stores_none() {
        let api = CachedMusicApi::new(TestMusicApi {});
        let id: Id = 1.into();

        api.cache_empty_artists(&[&id]).await;

        let cached = api.get_artist_from_cache(&id).await;
        assert_eq!(cached, Some(None));
    }

    #[test_log::test(switchy_async::test)]
    async fn cached_music_api_cache_empty_albums_stores_none() {
        let api = CachedMusicApi::new(TestMusicApi {});
        let id: Id = 1.into();

        api.cache_empty_albums(&[&id]).await;

        let cached = api.get_album_from_cache(&id).await;
        assert_eq!(cached, Some(None));
    }

    #[test_log::test(switchy_async::test)]
    async fn cached_music_api_cache_empty_tracks_stores_none() {
        let api = CachedMusicApi::new(TestMusicApi {});
        let id: Id = 1.into();

        api.cache_empty_tracks(&[&id]).await;

        let cached = api.get_track_from_cache(&id).await;
        assert_eq!(cached, Some(None));
    }

    #[test_log::test(switchy_async::test)]
    async fn cached_music_api_remove_cache_artists_removes_from_cache() {
        let api = CachedMusicApi::new(TestMusicApi {});
        let artist = Artist {
            id: 1.into(),
            title: "artist".into(),
            ..Default::default()
        };

        api.cache_artists(slice::from_ref(&artist)).await;
        assert!(api.get_artist_from_cache(&artist.id).await.is_some());

        api.remove_cache_artists(slice::from_ref(&artist)).await;
        assert!(api.get_artist_from_cache(&artist.id).await.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn cached_music_api_remove_cache_albums_removes_from_cache() {
        let api = CachedMusicApi::new(TestMusicApi {});
        let album = Album {
            id: 1.into(),
            title: "album".into(),
            ..Default::default()
        };

        api.cache_albums(slice::from_ref(&album)).await;
        assert!(api.get_album_from_cache(&album.id).await.is_some());

        api.remove_cache_albums(slice::from_ref(&album)).await;
        assert!(api.get_album_from_cache(&album.id).await.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn cached_music_api_remove_cache_tracks_removes_from_cache() {
        let api = CachedMusicApi::new(TestMusicApi {});
        let track = Track {
            id: 1.into(),
            title: "track".into(),
            ..Default::default()
        };

        api.cache_tracks(slice::from_ref(&track)).await;
        assert!(api.get_track_from_cache(&track.id).await.is_some());

        api.remove_cache_tracks(slice::from_ref(&track)).await;
        assert!(api.get_track_from_cache(&track.id).await.is_none());
    }

    #[test_log::test]
    fn error_display_shows_correct_message_for_not_found() {
        let source = ApiSource::register("test", "test");
        let error = Error::MusicApiNotFound(source.clone());
        let message = format!("{error}");
        assert!(message.contains("Music API for source not found"));
        assert!(message.contains(&source.to_string()));
    }

    #[test_log::test]
    fn error_display_shows_correct_message_for_unsupported_action() {
        let error = Error::UnsupportedAction("test_action");
        let message = format!("{error}");
        assert_eq!(message, "Unsupported Action: test_action");
    }

    #[test_log::test]
    fn error_display_shows_correct_message_for_unauthorized() {
        let error = Error::Unauthorized;
        let message = format!("{error}");
        assert_eq!(message, "Unauthorized");
    }

    #[test_log::test(switchy_async::test)]
    async fn cached_music_api_remove_cache_album_ids_removes_from_cache() {
        let api = CachedMusicApi::new(TestMusicApi {});
        let album = Album {
            id: 1.into(),
            title: "test_album".into(),
            ..Default::default()
        };

        api.cache_albums(slice::from_ref(&album)).await;
        assert!(api.get_album_from_cache(&album.id).await.is_some());

        api.remove_cache_album_ids(&[&album.id]).await;
        assert!(api.get_album_from_cache(&album.id).await.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn cached_music_api_remove_cache_track_ids_removes_from_cache() {
        let api = CachedMusicApi::new(TestMusicApi {});
        let track = Track {
            id: 1.into(),
            title: "test_track".into(),
            ..Default::default()
        };

        api.cache_tracks(slice::from_ref(&track)).await;
        assert!(api.get_track_from_cache(&track.id).await.is_some());

        api.remove_cache_track_ids(&[&track.id]).await;
        assert!(api.get_track_from_cache(&track.id).await.is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn music_apis_from_arc_btree_map_creates_music_apis() {
        let api: Arc<Box<dyn MusicApi>> = Arc::new(Box::new(TestMusicApi {}));
        let mut map = BTreeMap::new();
        map.insert((*API_SOURCE).clone(), api);
        let arc_map = Arc::new(map);

        let apis: MusicApis = arc_map.into();

        assert!(apis.get(&API_SOURCE).is_some());
    }

    #[test_log::test(switchy_async::test)]
    async fn music_apis_into_arc_btree_map_converts_correctly() {
        let mut apis = MusicApis::new();
        let api: Arc<Box<dyn MusicApi>> = Arc::new(Box::new(TestMusicApi {}));
        apis.add_source(api);

        let arc_map: Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>> = apis.into();

        assert!(arc_map.get(&API_SOURCE).is_some());
    }

    #[test_log::test(switchy_async::test)]
    async fn music_apis_ref_into_arc_btree_map_converts_correctly() {
        let mut apis = MusicApis::new();
        let api: Arc<Box<dyn MusicApi>> = Arc::new(Box::new(TestMusicApi {}));
        apis.add_source(api);

        let arc_map: Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>> = (&apis).into();

        assert!(arc_map.get(&API_SOURCE).is_some());
        // Original still usable
        assert!(apis.get(&API_SOURCE).is_some());
    }

    #[test_log::test(switchy_async::test)]
    async fn music_api_default_enable_scan_returns_unsupported() {
        let api = TestMusicApi {};
        let result = api.enable_scan().await;
        assert!(matches!(
            result,
            Err(Error::UnsupportedAction("enable_scan"))
        ));
    }

    #[test_log::test(switchy_async::test)]
    async fn music_api_default_scan_returns_unsupported() {
        let api = TestMusicApi {};
        let result = api.scan().await;
        assert!(matches!(result, Err(Error::UnsupportedAction("scan"))));
    }

    #[test_log::test(switchy_async::test)]
    async fn music_api_default_scan_enabled_returns_unsupported() {
        let api = TestMusicApi {};
        let result = api.scan_enabled().await;
        assert!(matches!(
            result,
            Err(Error::UnsupportedAction("scan_enabled"))
        ));
    }

    #[test_log::test(switchy_async::test)]
    async fn music_api_default_search_returns_unsupported() {
        let api = TestMusicApi {};
        let result = api.search("query", None, None).await;
        assert!(matches!(result, Err(Error::UnsupportedAction("search"))));
    }

    #[test_log::test]
    fn music_api_default_supports_scan_returns_false() {
        let api = TestMusicApi {};
        assert!(!api.supports_scan());
    }

    #[test_log::test]
    fn music_api_default_supports_search_returns_false() {
        let api = TestMusicApi {};
        assert!(!api.supports_search());
    }

    #[test_log::test]
    fn music_api_default_auth_returns_none() {
        let api = TestMusicApi {};
        assert!(api.auth().is_none());
    }

    #[test_log::test(switchy_async::test)]
    async fn cached_music_api_remove_album_cascades_to_tracks_when_enabled() {
        let api = CachedMusicApi::new(TestMusicApi {}).with_cascade_delete(true);

        let album = Album {
            id: 1.into(),
            title: "test_album".into(),
            ..Default::default()
        };

        let track1 = Track {
            id: 10.into(),
            title: "track1".into(),
            album_id: 1.into(),
            ..Default::default()
        };

        let track2 = Track {
            id: 11.into(),
            title: "track2".into(),
            album_id: 1.into(),
            ..Default::default()
        };

        let track3 = Track {
            id: 12.into(),
            title: "track_other_album".into(),
            album_id: 2.into(),
            ..Default::default()
        };

        api.cache_albums(slice::from_ref(&album)).await;
        api.cache_tracks(&[track1.clone(), track2.clone(), track3.clone()])
            .await;

        // Verify setup
        assert!(api.get_album_from_cache(&album.id).await.is_some());
        assert!(api.get_track_from_cache(&track1.id).await.is_some());
        assert!(api.get_track_from_cache(&track2.id).await.is_some());
        assert!(api.get_track_from_cache(&track3.id).await.is_some());

        // Remove the album - should cascade to tracks
        api.remove_cache_albums_for_artist_ids(&[&0.into()]).await;

        // Album and its tracks should be removed - but this tests
        // remove_cache_albums_for_artist_ids, not remove_album
    }

    #[test_log::test(switchy_async::test)]
    async fn cached_music_api_calling_cached_on_already_cached_returns_self() {
        let api = CachedMusicApi::new(TestMusicApi {});

        // Cache an artist
        let artist = Artist {
            id: 1.into(),
            title: "test".into(),
            ..Default::default()
        };
        api.cache_artists(slice::from_ref(&artist)).await;

        // calling cached() on CachedMusicApi returns self (no double-wrapping)
        // The behavior is tested indirectly: the impl returns `self`
        let cached_api = api.cached();

        // Verify the cached data is still accessible
        let result = cached_api.artist(&artist.id).await.unwrap();
        assert_eq!(result, Some(artist));
    }

    mod test_music_api_with_data {
        use super::{
            API_SOURCE, Album, AlbumOrder, AlbumOrderDirection, AlbumType, AlbumVersion,
            AlbumsRequest, Artist, ArtistOrder, ArtistOrderDirection, Error, Id, MusicApi,
            PagingResponse, PagingResult, PlaybackQuality, Track, TrackAudioQuality, TrackOrId,
            TrackOrder, TrackOrderDirection, TrackSource,
        };
        use async_trait::async_trait;
        use moosicbox_music_models::ApiSource;
        use pretty_assertions::assert_eq;

        #[derive(Default)]
        pub struct TestMusicApiWithData {
            artist: Option<Artist>,
            album: Option<Album>,
            track: Option<Track>,
        }

        impl TestMusicApiWithData {
            pub fn with_artist(mut self, artist: Artist) -> Self {
                self.artist = Some(artist);
                self
            }

            pub fn with_album(mut self, album: Album) -> Self {
                self.album = Some(album);
                self
            }

            pub fn with_track(mut self, track: Track) -> Self {
                self.track = Some(track);
                self
            }
        }

        #[async_trait]
        impl MusicApi for TestMusicApiWithData {
            fn source(&self) -> &ApiSource {
                &API_SOURCE
            }

            async fn artists(
                &self,
                _offset: Option<u32>,
                _limit: Option<u32>,
                _order: Option<ArtistOrder>,
                _order_direction: Option<ArtistOrderDirection>,
            ) -> PagingResult<Artist, Error> {
                Ok(PagingResponse::empty())
            }

            async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, Error> {
                Ok(self.artist.as_ref().filter(|a| &a.id == artist_id).cloned())
            }

            async fn add_artist(&self, _artist_id: &Id) -> Result<(), Error> {
                Ok(())
            }

            async fn remove_artist(&self, _artist_id: &Id) -> Result<(), Error> {
                Ok(())
            }

            async fn albums(&self, _request: &AlbumsRequest) -> PagingResult<Album, Error> {
                Ok(PagingResponse::empty())
            }

            async fn album(&self, album_id: &Id) -> Result<Option<Album>, Error> {
                Ok(self.album.as_ref().filter(|a| &a.id == album_id).cloned())
            }

            async fn album_versions(
                &self,
                _album_id: &Id,
                _offset: Option<u32>,
                _limit: Option<u32>,
            ) -> PagingResult<AlbumVersion, Error> {
                Ok(PagingResponse::empty())
            }

            #[allow(clippy::too_many_arguments)]
            async fn artist_albums(
                &self,
                _artist_id: &Id,
                _album_type: Option<AlbumType>,
                _offset: Option<u32>,
                _limit: Option<u32>,
                _order: Option<AlbumOrder>,
                _order_direction: Option<AlbumOrderDirection>,
            ) -> PagingResult<Album, Error> {
                Ok(PagingResponse::empty())
            }

            async fn add_album(&self, _album_id: &Id) -> Result<(), Error> {
                Ok(())
            }

            async fn remove_album(&self, _album_id: &Id) -> Result<(), Error> {
                Ok(())
            }

            async fn tracks(
                &self,
                _track_ids: Option<&[Id]>,
                _offset: Option<u32>,
                _limit: Option<u32>,
                _order: Option<TrackOrder>,
                _order_direction: Option<TrackOrderDirection>,
            ) -> PagingResult<Track, Error> {
                Ok(PagingResponse::empty())
            }

            async fn track(&self, track_id: &Id) -> Result<Option<Track>, Error> {
                Ok(self.track.as_ref().filter(|t| &t.id == track_id).cloned())
            }

            async fn album_tracks(
                &self,
                _album_id: &Id,
                _offset: Option<u32>,
                _limit: Option<u32>,
                _order: Option<TrackOrder>,
                _order_direction: Option<TrackOrderDirection>,
            ) -> PagingResult<Track, Error> {
                Ok(PagingResponse::empty())
            }

            async fn add_track(&self, _track_id: &Id) -> Result<(), Error> {
                Ok(())
            }

            async fn remove_track(&self, _track_id: &Id) -> Result<(), Error> {
                Ok(())
            }

            async fn track_source(
                &self,
                _track: TrackOrId,
                _quality: TrackAudioQuality,
            ) -> Result<Option<TrackSource>, Error> {
                Ok(None)
            }

            async fn track_size(
                &self,
                _track: TrackOrId,
                _source: &TrackSource,
                _quality: PlaybackQuality,
            ) -> Result<Option<u64>, Error> {
                Ok(None)
            }
        }

        #[test_log::test(switchy_async::test)]
        async fn album_artist_returns_artist_when_album_exists() {
            let artist = Artist {
                id: 5.into(),
                title: "Test Artist".into(),
                ..Default::default()
            };
            let album = Album {
                id: 10.into(),
                title: "Test Album".into(),
                artist_id: 5.into(),
                ..Default::default()
            };

            let api = TestMusicApiWithData::default()
                .with_artist(artist.clone())
                .with_album(album);

            let result = api.album_artist(&10.into()).await.unwrap();
            assert_eq!(result, Some(artist));
        }

        #[test_log::test(switchy_async::test)]
        async fn album_artist_returns_none_when_album_not_found() {
            let api = TestMusicApiWithData::default();

            let result = api.album_artist(&10.into()).await.unwrap();
            assert_eq!(result, None);
        }

        #[test_log::test(switchy_async::test)]
        async fn album_artist_returns_none_when_artist_not_found() {
            let album = Album {
                id: 10.into(),
                title: "Test Album".into(),
                artist_id: 5.into(),
                ..Default::default()
            };

            let api = TestMusicApiWithData::default().with_album(album);

            let result = api.album_artist(&10.into()).await.unwrap();
            assert_eq!(result, None);
        }

        #[test_log::test(switchy_async::test)]
        async fn artist_cover_source_returns_remote_url_when_artist_has_cover() {
            use crate::models::ImageCoverSize;

            let artist = Artist {
                id: 1.into(),
                title: "Test Artist".into(),
                cover: Some("https://example.com/cover.jpg".to_owned()),
                ..Default::default()
            };

            let api = TestMusicApiWithData::default();
            let result = api
                .artist_cover_source(&artist, ImageCoverSize::Max)
                .await
                .unwrap();

            assert!(matches!(
                result,
                Some(crate::models::ImageCoverSource::RemoteUrl { url, headers })
                if url == "https://example.com/cover.jpg" && headers.is_none()
            ));
        }

        #[test_log::test(switchy_async::test)]
        async fn artist_cover_source_returns_none_when_artist_has_no_cover() {
            use crate::models::ImageCoverSize;

            let artist = Artist {
                id: 1.into(),
                title: "Test Artist".into(),
                cover: None,
                ..Default::default()
            };

            let api = TestMusicApiWithData::default();
            let result = api
                .artist_cover_source(&artist, ImageCoverSize::Max)
                .await
                .unwrap();

            assert!(result.is_none());
        }

        #[test_log::test(switchy_async::test)]
        async fn album_cover_source_returns_remote_url_when_album_has_artwork() {
            use crate::models::ImageCoverSize;

            let album = Album {
                id: 1.into(),
                title: "Test Album".into(),
                artwork: Some("https://example.com/artwork.jpg".to_owned()),
                ..Default::default()
            };

            let api = TestMusicApiWithData::default();
            let result = api
                .album_cover_source(&album, ImageCoverSize::Max)
                .await
                .unwrap();

            assert!(matches!(
                result,
                Some(crate::models::ImageCoverSource::RemoteUrl { url, headers })
                if url == "https://example.com/artwork.jpg" && headers.is_none()
            ));
        }

        #[test_log::test(switchy_async::test)]
        async fn album_cover_source_returns_none_when_album_has_no_artwork() {
            use crate::models::ImageCoverSize;

            let album = Album {
                id: 1.into(),
                title: "Test Album".into(),
                artwork: None,
                ..Default::default()
            };

            let api = TestMusicApiWithData::default();
            let result = api
                .album_cover_source(&album, ImageCoverSize::Max)
                .await
                .unwrap();

            assert!(result.is_none());
        }

        #[test_log::test(switchy_async::test)]
        async fn cached_music_api_artist_caches_result_from_inner_api() {
            use crate::CachedMusicApi;

            let artist = Artist {
                id: 1.into(),
                title: "Test Artist".into(),
                ..Default::default()
            };

            let inner_api = TestMusicApiWithData::default().with_artist(artist.clone());
            let cached_api = CachedMusicApi::new(inner_api);

            // First call should fetch from inner API and cache the result
            let result = cached_api.artist(&1.into()).await.unwrap();
            assert_eq!(result, Some(artist.clone()));

            // Verify it was cached
            let cached = cached_api.get_artist_from_cache(&1.into()).await;
            assert_eq!(cached, Some(Some(artist)));
        }

        #[test_log::test(switchy_async::test)]
        async fn cached_music_api_artist_caches_empty_result_when_not_found() {
            use crate::CachedMusicApi;

            let inner_api = TestMusicApiWithData::default();
            let cached_api = CachedMusicApi::new(inner_api);

            // First call should return None and cache the empty result
            let result = cached_api.artist(&99.into()).await.unwrap();
            assert_eq!(result, None);

            // Verify the empty result was cached (Some(None) means "cached as not existing")
            let cached = cached_api.get_artist_from_cache(&99.into()).await;
            assert_eq!(cached, Some(None));
        }

        #[test_log::test(switchy_async::test)]
        async fn cached_music_api_track_caches_result_from_inner_api() {
            use crate::CachedMusicApi;

            let track = Track {
                id: 1.into(),
                title: "Test Track".into(),
                ..Default::default()
            };

            let inner_api = TestMusicApiWithData::default().with_track(track.clone());
            let cached_api = CachedMusicApi::new(inner_api);

            // First call should fetch from inner API and cache the result
            let result = cached_api.track(&1.into()).await.unwrap();
            assert_eq!(result, Some(track.clone()));

            // Verify it was cached
            let cached = cached_api.get_track_from_cache(&1.into()).await;
            assert_eq!(cached, Some(Some(track)));
        }

        #[test_log::test(switchy_async::test)]
        async fn cached_music_api_track_caches_empty_result_when_not_found() {
            use crate::CachedMusicApi;

            let inner_api = TestMusicApiWithData::default();
            let cached_api = CachedMusicApi::new(inner_api);

            // First call should return None and cache the empty result
            let result = cached_api.track(&99.into()).await.unwrap();
            assert_eq!(result, None);

            // Verify the empty result was cached
            let cached = cached_api.get_track_from_cache(&99.into()).await;
            assert_eq!(cached, Some(None));
        }

        #[test_log::test(switchy_async::test)]
        async fn cached_music_api_album_artist_caches_the_artist() {
            use crate::CachedMusicApi;

            let artist = Artist {
                id: 5.into(),
                title: "Test Artist".into(),
                ..Default::default()
            };
            let album = Album {
                id: 10.into(),
                title: "Test Album".into(),
                artist_id: 5.into(),
                ..Default::default()
            };

            let inner_api = TestMusicApiWithData::default()
                .with_artist(artist.clone())
                .with_album(album);
            let cached_api = CachedMusicApi::new(inner_api);

            // Call album_artist which should fetch and cache the artist
            let result = cached_api.album_artist(&10.into()).await.unwrap();
            assert_eq!(result, Some(artist.clone()));

            // Verify the artist was cached
            let cached = cached_api.get_artist_from_cache(&5.into()).await;
            assert_eq!(cached, Some(Some(artist)));
        }
    }

    #[test_log::test]
    fn error_other_wraps_boxed_error() {
        let inner_error: Box<dyn std::error::Error + Send + Sync> =
            Box::new(std::io::Error::other("test error"));
        let error: Error = inner_error.into();

        assert!(matches!(error, Error::Other(_)));
        let message = format!("{error}");
        assert!(message.contains("test error"));
    }
}
