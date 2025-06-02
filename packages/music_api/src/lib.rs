#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::type_complexity)]

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use models::{
    AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder, ArtistOrderDirection,
    ImageCoverSize, ImageCoverSource, TrackAudioQuality, TrackOrder, TrackOrderDirection,
    TrackSource,
};
use moosicbox_menu_models::AlbumVersion;
pub use moosicbox_music_api_models as models;
use moosicbox_music_models::{Album, AlbumType, ApiSource, Artist, PlaybackQuality, Track, id::Id};
use moosicbox_paging::PagingResult;
use tokio::sync::{Mutex, RwLock};

pub mod profiles;

#[derive(Clone)]
pub struct MusicApis(Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>>);

impl Default for MusicApis {
    fn default() -> Self {
        Self::new()
    }
}

impl MusicApis {
    #[must_use]
    pub fn new() -> Self {
        Self(Arc::new(BTreeMap::new()))
    }

    pub fn add_source(&mut self, api: Arc<Box<dyn MusicApi>>) {
        let mut map = (*self.0).clone();
        map.insert(api.source().clone(), api);

        self.0 = Arc::new(map);
    }
}

impl From<&MusicApis> for Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>> {
    fn from(value: &MusicApis) -> Self {
        value.0.clone()
    }
}

impl From<MusicApis> for Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>> {
    fn from(value: MusicApis) -> Self {
        value.0
    }
}

impl From<Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>>> for MusicApis {
    fn from(value: Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>>) -> Self {
        Self(value)
    }
}

impl SourceToMusicApi for MusicApis {
    fn get(&self, source: &ApiSource) -> Option<Arc<Box<dyn MusicApi>>> {
        self.0.get(source).cloned()
    }
}

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

    fn into_iter(self) -> Self::IntoIter {
        MusicApisIter {
            inner: self.0.iter(),
        }
    }
}

pub trait SourceToMusicApi {
    /// # Errors
    ///
    /// * If the `MusicApi` is not found
    fn get(&self, source: &ApiSource) -> Option<Arc<Box<dyn MusicApi>>>;
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Music API for source not found: {0}")]
    MusicApiNotFound(ApiSource),
    #[error("Unsupported Action")]
    UnsupportedAction,
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

pub enum TrackOrId {
    Track(Box<Track>),
    Id(Id),
}

impl TrackOrId {
    /// # Errors
    ///
    /// * If failed to get the track from the `MusicApi`
    pub async fn track(self, api: &dyn MusicApi) -> Result<Option<Track>, Error> {
        Ok(match self {
            Self::Track(track) => Some(*track),
            Self::Id(id) => api.track(&id).await?,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &Id {
        match self {
            Self::Track(track) => &track.id,
            Self::Id(id) => id,
        }
    }
}

impl From<Id> for TrackOrId {
    fn from(value: Id) -> Self {
        Self::Id(value)
    }
}

impl From<&Id> for TrackOrId {
    fn from(value: &Id) -> Self {
        Self::Id(value.to_owned())
    }
}

impl From<Track> for TrackOrId {
    fn from(value: Track) -> Self {
        Self::Track(Box::new(value))
    }
}

impl From<&Track> for TrackOrId {
    fn from(value: &Track) -> Self {
        Self::Track(Box::new(value.to_owned()))
    }
}

#[async_trait]
pub trait MusicApi: Send + Sync {
    fn source(&self) -> &ApiSource;

    async fn artists(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<ArtistOrder>,
        order_direction: Option<ArtistOrderDirection>,
    ) -> PagingResult<Artist, Error>;

    async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, Error>;

    async fn add_artist(&self, artist_id: &Id) -> Result<(), Error>;

    async fn remove_artist(&self, artist_id: &Id) -> Result<(), Error>;

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

    async fn albums(&self, request: &AlbumsRequest) -> PagingResult<Album, Error>;

    async fn album(&self, album_id: &Id) -> Result<Option<Album>, Error>;

    async fn album_versions(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> PagingResult<AlbumVersion, Error>;

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

    async fn add_album(&self, album_id: &Id) -> Result<(), Error>;

    async fn remove_album(&self, album_id: &Id) -> Result<(), Error>;

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

    async fn tracks(
        &self,
        track_ids: Option<&[Id]>,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, Error>;

    async fn track(&self, track_id: &Id) -> Result<Option<Track>, Error>;

    async fn album_tracks(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, Error>;

    async fn add_track(&self, track_id: &Id) -> Result<(), Error>;

    async fn remove_track(&self, track_id: &Id) -> Result<(), Error>;

    async fn track_source(
        &self,
        track: TrackOrId,
        quality: TrackAudioQuality,
    ) -> Result<Option<TrackSource>, Error>;

    async fn track_size(
        &self,
        track: TrackOrId,
        source: &TrackSource,
        quality: PlaybackQuality,
    ) -> Result<Option<u64>, Error>;

    async fn scan(&self) -> Result<(), Error> {
        Err(Error::UnsupportedAction)
    }

    async fn authenticate(&self) -> Result<(), Error> {
        Err(Error::UnsupportedAction)
    }

    fn scan_enabled(&self) -> bool {
        false
    }

    fn authentication_enabled(&self) -> bool {
        false
    }

    async fn is_logged_in(&self) -> Result<bool, Error> {
        Err(Error::UnsupportedAction)
    }

    async fn logout(&self) -> Result<(), Error> {
        Err(Error::UnsupportedAction)
    }
}

pub struct CachedMusicApi<T: MusicApi> {
    inner: T,
    cascade_delete: bool,
    artists: Arc<RwLock<BTreeMap<Id, Option<Artist>>>>,
    albums: Arc<RwLock<BTreeMap<Id, Option<Album>>>>,
    tracks: Arc<RwLock<BTreeMap<Id, Option<Track>>>>,
}

impl<T: MusicApi> CachedMusicApi<T> {
    pub fn new(api: T) -> Self {
        Self {
            inner: api,
            cascade_delete: false,
            artists: Arc::new(RwLock::new(BTreeMap::new())),
            albums: Arc::new(RwLock::new(BTreeMap::new())),
            tracks: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    #[must_use]
    pub const fn with_cascade_delete(mut self, cascade_delete: bool) -> Self {
        self.cascade_delete = cascade_delete;
        self
    }

    pub const fn set_cascade_delete(&mut self, cascade_delete: bool) {
        self.cascade_delete = cascade_delete;
    }

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

    pub async fn cache_empty_artists(&self, ids: &[&Id]) {
        Self::cache_empty_values(&self.artists, ids).await;
    }

    pub async fn cache_empty_albums(&self, ids: &[&Id]) {
        Self::cache_empty_values(&self.albums, ids).await;
    }

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

    pub async fn cache_artists(&self, artists: &[Artist]) {
        Self::cache_artists_inner(&self.artists, artists).await;
    }

    async fn cache_artists_inner(cache: &RwLock<BTreeMap<Id, Option<Artist>>>, artists: &[Artist]) {
        let mut cache = cache.write().await;
        for artist in artists {
            cache.insert(artist.id.clone(), Some(artist.to_owned()));
        }
    }

    pub async fn cache_albums(&self, albums: &[Album]) {
        Self::cache_albums_inner(&self.albums, albums).await;
    }

    async fn cache_albums_inner(cache: &RwLock<BTreeMap<Id, Option<Album>>>, albums: &[Album]) {
        let mut cache = cache.write().await;
        for album in albums {
            cache.insert(album.id.clone(), Some(album.to_owned()));
        }
    }

    pub async fn cache_tracks(&self, tracks: &[Track]) {
        Self::cache_tracks_inner(&self.tracks, tracks).await;
    }

    async fn cache_tracks_inner(cache: &RwLock<BTreeMap<Id, Option<Track>>>, tracks: &[Track]) {
        let mut cache = cache.write().await;
        for track in tracks {
            cache.insert(track.id.clone(), Some(track.to_owned()));
        }
    }

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

    pub async fn remove_cache_album_ids(&self, ids: &[&Id]) {
        Self::remove_cache_album_ids_inner(&mut *self.albums.write().await, ids);
    }

    fn remove_cache_album_ids_inner(albums: &mut BTreeMap<Id, Option<Album>>, ids: &[&Id]) {
        Self::remove_cache_ids(albums, ids);
    }

    pub async fn remove_cache_track_ids(&self, ids: &[&Id]) {
        Self::remove_cache_ids(&mut *self.tracks.write().await, ids);
    }

    fn remove_cache_ids<E>(cache: &mut BTreeMap<Id, Option<E>>, ids: &[&Id]) {
        for id in ids {
            cache.remove(*id);
        }
    }

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
}

#[cfg(test)]
#[allow(clippy::module_name_repetitions)]
mod test {
    use std::sync::LazyLock;

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

    #[test_log::test(tokio::test)]
    async fn doesnt_cache_nothing_for_artists() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let one = api.artist(&1.into()).await.unwrap();

        assert_eq!(one, None);
    }

    #[test_log::test(tokio::test)]
    async fn can_cache_single_artist_by_id() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let artist = Artist {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };

        api.cache_artists(&[artist.clone()]).await;

        let one = api.artist(&artist.id).await.unwrap();

        assert_eq!(one, Some(artist));
    }

    #[test_log::test(tokio::test)]
    async fn doesnt_return_artist_from_cache_if_doesnt_exist() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let artist = Artist {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };

        api.cache_artists(&[artist.clone()]).await;

        let one = api.artist(&2.into()).await.unwrap();

        assert_eq!(one, None);
    }

    #[test_log::test(tokio::test)]
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

        api.cache_artists(&[artist1.clone()]).await;
        api.cache_artists(&[artist2.clone()]).await;

        let one = api.artist(&artist1.id).await.unwrap();
        let two = api.artist(&artist2.id).await.unwrap();

        assert_eq!(one, Some(artist1));
        assert_eq!(two, Some(artist2));
    }

    #[test_log::test(tokio::test)]
    async fn doesnt_cache_nothing_for_albums() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let one = api.album(&1.into()).await.unwrap();

        assert_eq!(one, None);
    }

    #[test_log::test(tokio::test)]
    async fn can_cache_single_album_by_id() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let album = Album {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };

        api.cache_albums(&[album.clone()]).await;

        let one = api.album(&album.id).await.unwrap();

        assert_eq!(one, Some(album));
    }

    #[test_log::test(tokio::test)]
    async fn doesnt_return_album_from_cache_if_doesnt_exist() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let album = Album {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };

        api.cache_albums(&[album.clone()]).await;

        let one = api.album(&2.into()).await.unwrap();

        assert_eq!(one, None);
    }

    #[test_log::test(tokio::test)]
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

        api.cache_albums(&[album1.clone()]).await;
        api.cache_albums(&[album2.clone()]).await;

        let one = api.album(&album1.id).await.unwrap();
        let two = api.album(&album2.id).await.unwrap();

        assert_eq!(one, Some(album1));
        assert_eq!(two, Some(album2));
    }

    #[test_log::test(tokio::test)]
    async fn doesnt_cache_nothing_for_tracks() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let one = api.track(&1.into()).await.unwrap();

        assert_eq!(one, None);
    }

    #[test_log::test(tokio::test)]
    async fn can_cache_single_track_by_id() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let track = Track {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };

        api.cache_tracks(&[track.clone()]).await;

        let one = api.track(&track.id).await.unwrap();

        assert_eq!(one, Some(track));
    }

    #[test_log::test(tokio::test)]
    async fn doesnt_return_track_from_cache_if_doesnt_exist() {
        let api = CachedMusicApi::new(TestMusicApi {});

        let track = Track {
            id: 1.into(),
            title: "bob".into(),
            ..Default::default()
        };

        api.cache_tracks(&[track.clone()]).await;

        let one = api.track(&2.into()).await.unwrap();

        assert_eq!(one, None);
    }

    #[test_log::test(tokio::test)]
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

        api.cache_tracks(&[track1.clone()]).await;
        api.cache_tracks(&[track2.clone()]).await;

        let one = api.track(&track1.id).await.unwrap();
        let two = api.track(&track2.id).await.unwrap();

        assert_eq!(one, Some(track1));
        assert_eq!(two, Some(track2));
    }

    #[test_log::test(tokio::test)]
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

        api.cache_artists(&[artist.clone()]).await;
        api.cache_albums(&[album.clone()]).await;

        api.remove_artist(&artist.id).await.unwrap();

        let cache_artist = api.artist(&artist.id).await.unwrap();
        assert_eq!(cache_artist, None);

        let cache_album = api.album(&album.id).await.unwrap();
        assert_eq!(cache_album, Some(album));
    }

    #[test_log::test(tokio::test)]
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

        api.cache_artists(&[artist.clone()]).await;
        api.cache_albums(&[album.clone()]).await;
        api.cache_tracks(&[track.clone()]).await;

        api.remove_artist(&artist.id).await.unwrap();

        let cache_artist = api.artist(&artist.id).await.unwrap();
        assert_eq!(cache_artist, None);

        let cache_album = api.album(&album.id).await.unwrap();
        assert_eq!(cache_album, Some(album));

        let cache_track = api.track(&track.id).await.unwrap();
        assert_eq!(cache_track, Some(track));
    }

    #[test_log::test(tokio::test)]
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

        api.cache_artists(&[artist.clone()]).await;
        api.cache_albums(&[album.clone()]).await;

        api.remove_artist(&artist.id).await.unwrap();

        let artist = api.artist(&artist.id).await.unwrap();
        assert_eq!(artist, None);

        let album = api.album(&album.id).await.unwrap();
        assert_eq!(album, None);
    }

    #[test_log::test(tokio::test)]
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

        api.cache_artists(&[artist.clone()]).await;
        api.cache_albums(&[album.clone()]).await;
        api.cache_tracks(&[track.clone()]).await;

        api.remove_artist(&artist.id).await.unwrap();

        let artist = api.artist(&artist.id).await.unwrap();
        assert_eq!(artist, None);

        let album = api.album(&album.id).await.unwrap();
        assert_eq!(album, None);

        let track = api.track(&track.id).await.unwrap();
        assert_eq!(track, None);
    }
}
