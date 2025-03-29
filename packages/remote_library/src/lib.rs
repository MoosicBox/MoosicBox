#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use moosicbox_http::{IClient as _, StatusCode};
use moosicbox_menu_models::{AlbumVersion, api::ApiAlbumVersion};
use moosicbox_music_api::{
    AddAlbumError, AddArtistError, AddTrackError, AlbumError, AlbumsError, ArtistAlbumsError,
    ArtistError, ArtistsError, MusicApi, RemoveAlbumError, RemoveArtistError, RemoveTrackError,
    TrackError, TrackOrId, TracksError,
    models::{
        AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder, ArtistOrderDirection,
        ImageCoverSize, ImageCoverSource, TrackAudioQuality, TrackOrder, TrackOrderDirection,
        TrackSource,
    },
};
use moosicbox_music_models::{
    Album, AlbumType, ApiSource, Artist, PlaybackQuality, Track,
    api::{ApiAlbum, ApiTrack},
    id::Id,
};
use moosicbox_paging::{Page, PagingResponse, PagingResult};
use thiserror::Error;

static CLIENT: LazyLock<moosicbox_http::Client> =
    LazyLock::new(|| moosicbox_http::Client::builder().build().unwrap());

#[derive(Debug, Error)]
pub enum RequestError {
    #[error(transparent)]
    Request(#[from] moosicbox_http::Error),
    #[error("Unsuccessful: {0}")]
    Unsuccessful(String),
}

#[derive(Clone)]
pub struct RemoteLibraryMusicApi {
    host: String,
    api_source: ApiSource,
    profile: String,
}

impl RemoteLibraryMusicApi {
    #[must_use]
    pub const fn new(host: String, api_source: ApiSource, profile: String) -> Self {
        Self {
            host,
            api_source,
            profile,
        }
    }
}

#[async_trait]
impl MusicApi for RemoteLibraryMusicApi {
    fn source(&self) -> ApiSource {
        unimplemented!("Dynamic MusicApi must be implemented by the struct")
    }

    async fn artists(
        &self,
        _offset: Option<u32>,
        _limit: Option<u32>,
        _order: Option<ArtistOrder>,
        _order_direction: Option<ArtistOrderDirection>,
    ) -> PagingResult<Artist, ArtistsError> {
        unimplemented!("Fetching artists is not implemented")
    }

    async fn artist(&self, _artist_id: &Id) -> Result<Option<Artist>, ArtistError> {
        unimplemented!("Fetching artist is not implemented")
    }

    async fn add_artist(&self, _artist_id: &Id) -> Result<(), AddArtistError> {
        unimplemented!("Adding artist is not implemented")
    }

    async fn remove_artist(&self, _artist_id: &Id) -> Result<(), RemoveArtistError> {
        unimplemented!("Removing artist is not implemented")
    }

    async fn album_artist(&self, _album_id: &Id) -> Result<Option<Artist>, ArtistError> {
        unimplemented!("Fetching album artist is not implemented")
    }

    async fn artist_cover_source(
        &self,
        _artist: &Artist,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, ArtistError> {
        unimplemented!("Fetching artist cover source is not implemented")
    }

    async fn albums(&self, _request: &AlbumsRequest) -> PagingResult<Album, AlbumsError> {
        unimplemented!("Fetching albums is not implemented")
    }

    async fn album(&self, album_id: &Id) -> Result<Option<Album>, AlbumError> {
        let request = CLIENT
            .request(
                moosicbox_http::Method::Get,
                &format!(
                    "{host}/menu/album?albumId={album_id}&source={source}",
                    host = self.host,
                    source = self.api_source
                ),
            )
            .header("moosicbox-profile", &self.profile);

        let response = request
            .send()
            .await
            .map_err(|e| AlbumError::Other(Box::new(e)))?;

        if !response.status().is_success() {
            if response.status() == StatusCode::NOT_FOUND {
                return Ok(None);
            }
            return Err(AlbumError::Other(Box::new(RequestError::Unsuccessful(
                format!("Status {}", response.status()),
            ))));
        }

        let value: ApiAlbum = response
            .json()
            .await
            .map_err(|e| AlbumError::Other(Box::new(e)))?;

        Ok(Some(
            value
                .try_into()
                .map_err(|e| AlbumError::Other(Box::new(e)))?,
        ))
    }

    async fn album_versions(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> PagingResult<AlbumVersion, TracksError> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(50);

        let request = CLIENT
            .request(
                moosicbox_http::Method::Get,
                &format!(
                    "{host}/menu/album/versions?albumId={album_id}&source={source}",
                    host = self.host,
                    source = self.api_source
                ),
            )
            .header("moosicbox-profile", &self.profile);

        let response = request
            .send()
            .await
            .map_err(|e| TracksError::Other(Box::new(e)))?;

        if !response.status().is_success() {
            if response.status() == StatusCode::NOT_FOUND {
                return Ok(PagingResponse::empty());
            }
            return Err(TracksError::Other(Box::new(RequestError::Unsuccessful(
                format!("Status {}", response.status()),
            ))));
        }

        let value: Vec<ApiAlbumVersion> = response
            .json()
            .await
            .map_err(|e| TracksError::Other(Box::new(e)))?;

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
        _artist_id: &Id,
        _album_type: Option<AlbumType>,
        _offset: Option<u32>,
        _limit: Option<u32>,
        _order: Option<AlbumOrder>,
        _order_direction: Option<AlbumOrderDirection>,
    ) -> PagingResult<Album, ArtistAlbumsError> {
        unimplemented!("Fetching artist albums is not implemented")
    }

    async fn add_album(&self, _album_id: &Id) -> Result<(), AddAlbumError> {
        unimplemented!("Adding album is not implemented")
    }

    async fn remove_album(&self, _album_id: &Id) -> Result<(), RemoveAlbumError> {
        unimplemented!("Removing album is not implemented")
    }

    async fn album_cover_source(
        &self,
        _album: &Album,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, AlbumError> {
        unimplemented!("Fetching album cover source is not implemented")
    }

    async fn tracks(
        &self,
        track_ids: Option<&[Id]>,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, TracksError> {
        let Some(track_ids) = track_ids else {
            unimplemented!("Fetching all tracks is not implemented");
        };

        let track_ids_str = track_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let request = CLIENT
            .request(
                moosicbox_http::Method::Get,
                &format!(
                    "{host}/menu/tracks?trackIds={track_ids_str}&source={source}",
                    host = self.host,
                    source = self.api_source
                ),
            )
            .header("moosicbox-profile", &self.profile);

        let response = request
            .send()
            .await
            .map_err(|e| TracksError::Other(Box::new(e)))?;

        if !response.status().is_success() {
            return Err(TracksError::Other(Box::new(RequestError::Unsuccessful(
                format!("Status {}", response.status()),
            ))));
        }

        let tracks: Vec<Track> = response
            .json::<Vec<ApiTrack>>()
            .await
            .map_err(|e| TracksError::Other(Box::new(e)))?
            .into_iter()
            .map(Into::into)
            .collect();

        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(100);

        Ok(PagingResponse {
            page: Page::WithTotal {
                total: u32::try_from(tracks.len()).unwrap(),
                items: tracks,
                offset,
                limit,
            },
            fetch: Arc::new(tokio::sync::Mutex::new(Box::new({
                let api = self.clone();
                let track_ids = track_ids.to_vec();

                move |offset, limit| {
                    let api = api.clone();
                    let track_ids = track_ids.clone();

                    Box::pin(async move {
                        api.tracks(
                            Some(&track_ids),
                            Some(offset),
                            Some(limit),
                            order,
                            order_direction,
                        )
                        .await
                    })
                }
            }))),
        })
    }

    async fn album_tracks(
        &self,
        _album_id: &Id,
        _offset: Option<u32>,
        _limit: Option<u32>,
        _order: Option<TrackOrder>,
        _order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, TracksError> {
        unimplemented!("Fetching album tracks is not implemented")
    }

    async fn track(&self, track_id: &Id) -> Result<Option<Track>, TrackError> {
        let request = CLIENT
            .request(
                moosicbox_http::Method::Get,
                &format!(
                    "{host}/menu/track?trackId={track_id}&source={source}",
                    host = self.host,
                    source = self.api_source
                ),
            )
            .header("moosicbox-profile", &self.profile);

        let response = request
            .send()
            .await
            .map_err(|e| TrackError::Other(Box::new(e)))?;

        if !response.status().is_success() {
            if response.status() == StatusCode::NOT_FOUND {
                return Ok(None);
            }
            return Err(TrackError::Other(Box::new(RequestError::Unsuccessful(
                format!("Status {}", response.status()),
            ))));
        }

        let value = response
            .json()
            .await
            .map_err(|e| TrackError::Other(Box::new(e)))?;

        Ok(Some(value))
    }

    async fn add_track(&self, _track_id: &Id) -> Result<(), AddTrackError> {
        unimplemented!("Adding track is not implemented")
    }

    async fn remove_track(&self, _track_id: &Id) -> Result<(), RemoveTrackError> {
        unimplemented!("Removing track is not implemented")
    }

    async fn track_source(
        &self,
        _track: TrackOrId,
        _quality: TrackAudioQuality,
    ) -> Result<Option<TrackSource>, TrackError> {
        unimplemented!("Fetching track source is not implemented")
    }

    async fn track_size(
        &self,
        _track: TrackOrId,
        _source: &TrackSource,
        _quality: PlaybackQuality,
    ) -> Result<Option<u64>, TrackError> {
        unimplemented!("Fetching track size is not implemented")
    }
}
