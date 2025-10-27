//! Remote `MoosicBox` server music library API client.
//!
//! This crate provides a [`MusicApi`] implementation that connects to a remote `MoosicBox`
//! server over HTTP, allowing you to access and query a remote music library as if it were local.
//!
//! # Example
//!
//! ```rust
//! # use moosicbox_remote_library::RemoteLibraryMusicApi;
//! # use moosicbox_music_models::ApiSource;
//! # use moosicbox_music_api::MusicApi;
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let api = RemoteLibraryMusicApi::new(
//!     "http://localhost:8000".to_string(),
//!     ApiSource::library(),
//!     "default".to_string(),
//! );
//!
//! // Use the API to fetch artists, albums, tracks, etc.
//! let artists = api.artists(None, None, None, None).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Main Components
//!
//! * [`RemoteLibraryMusicApi`] - The main client for accessing remote music libraries
//! * [`RequestError`] - Error type for HTTP request failures

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::{Arc, LazyLock};

use async_trait::async_trait;
use moosicbox_menu_models::{AlbumVersion, api::ApiAlbumVersion};
use moosicbox_music_api::{
    MusicApi, TrackOrId,
    models::{
        AlbumFilters, AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder,
        ArtistOrderDirection, ImageCoverSize, ImageCoverSource, TrackAudioQuality, TrackOrder,
        TrackOrderDirection, TrackSource, search::api::ApiSearchResultsResponse,
    },
};
use moosicbox_music_models::{
    Album, AlbumType, ApiSource, Artist, AudioFormat, PlaybackQuality, Track,
    api::{ApiAlbum, ApiArtist, ApiTrack},
    id::Id,
};
use moosicbox_paging::{Page, PagingRequest, PagingResponse, PagingResult};
use switchy_http::models::StatusCode;
use thiserror::Error;
use tokio::sync::Mutex;
use urlencoding::encode;

static CLIENT: LazyLock<switchy_http::Client> =
    LazyLock::new(|| switchy_http::Client::builder().build().unwrap());

/// Errors that can occur when making HTTP requests to a remote `MoosicBox` server.
#[derive(Debug, Error)]
pub enum RequestError {
    /// HTTP request error from the underlying HTTP client.
    #[error(transparent)]
    Request(#[from] switchy_http::Error),
    /// HTTP request returned an unsuccessful status code.
    #[error("Unsuccessful: {0}")]
    Unsuccessful(String),
}

/// A [`MusicApi`] implementation that proxies requests to a remote `MoosicBox` server.
///
/// This implementation allows accessing a remote `MoosicBox` instance's music library
/// via HTTP requests, supporting all standard music API operations.
#[derive(Clone)]
pub struct RemoteLibraryMusicApi {
    host: String,
    api_source: ApiSource,
    profile: String,
}

impl RemoteLibraryMusicApi {
    /// Creates a new remote library API client.
    ///
    /// # Parameters
    ///
    /// * `host` - Base URL of the remote `MoosicBox` server (e.g., `"http://localhost:8000"`)
    /// * `api_source` - The API source identifier for this connection
    /// * `profile` - Profile name to use for authentication/authorization
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
    fn source(&self) -> &ApiSource {
        &self.api_source
    }

    async fn artists(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<ArtistOrder>,
        order_direction: Option<ArtistOrderDirection>,
    ) -> PagingResult<Artist, moosicbox_music_api::Error> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(100);

        let request = CLIENT
            .request(
                switchy_http::models::Method::Get,
                &format!(
                    "{host}/menu/artists{sort}{direction}",
                    host = self.host,
                    sort = order
                        .as_ref()
                        .map_or_else(String::new, |x| format!("?sort={x}")),
                    direction = order_direction
                        .as_ref()
                        .map_or_else(String::new, |x| format!("&direction={x}")),
                ),
            )
            .header("moosicbox-profile", &self.profile);

        let response = request
            .send()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        if !response.status().is_success() {
            if response.status() == StatusCode::NotFound {
                return Ok(PagingResponse::empty());
            }
            return Err(moosicbox_music_api::Error::Other(Box::new(
                RequestError::Unsuccessful(format!("Status {}", response.status())),
            )));
        }

        let value: Vec<ApiArtist> = response
            .json()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        let total = u32::try_from(value.len()).unwrap();
        let items: Result<Vec<_>, _> = value
            .into_iter()
            .skip(offset as usize)
            .take(std::cmp::min(total - offset, limit) as usize)
            .map(TryInto::try_into)
            .collect();
        let items = items.map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        let page = PagingResponse::new(
            Page::WithTotal {
                items,
                offset,
                limit,
                total,
            },
            {
                let api = self.clone();

                move |offset, limit| {
                    let api = api.clone();
                    Box::pin(async move {
                        api.artists(Some(offset), Some(limit), order, order_direction)
                            .await
                    })
                }
            },
        );

        Ok(page)
    }

    async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, moosicbox_music_api::Error> {
        let request = CLIENT
            .request(
                switchy_http::models::Method::Get,
                &format!(
                    "{host}/menu/artist?artistId={artist_id}&source={source}",
                    host = self.host,
                    source = self.api_source
                ),
            )
            .header("moosicbox-profile", &self.profile);

        let response = request
            .send()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        if !response.status().is_success() {
            if response.status() == StatusCode::NotFound {
                return Ok(None);
            }
            return Err(moosicbox_music_api::Error::Other(Box::new(
                RequestError::Unsuccessful(format!("Status {}", response.status())),
            )));
        }

        let value: ApiArtist = response
            .json()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        Ok(Some(value.into()))
    }

    async fn add_artist(&self, _artist_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Err(moosicbox_music_api::Error::UnsupportedAction(
            "Adding artist is not implemented",
        ))
    }

    async fn remove_artist(&self, _artist_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Err(moosicbox_music_api::Error::UnsupportedAction(
            "Removing artist is not implemented",
        ))
    }

    async fn album_artist(
        &self,
        album_id: &Id,
    ) -> Result<Option<Artist>, moosicbox_music_api::Error> {
        let request = CLIENT
            .request(
                switchy_http::models::Method::Get,
                &format!(
                    "{host}/menu/artist?albumId={album_id}&source={source}",
                    host = self.host,
                    source = self.api_source
                ),
            )
            .header("moosicbox-profile", &self.profile);

        let response = request
            .send()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        if !response.status().is_success() {
            if response.status() == StatusCode::NotFound {
                return Ok(None);
            }
            return Err(moosicbox_music_api::Error::Other(Box::new(
                RequestError::Unsuccessful(format!("Status {}", response.status())),
            )));
        }

        let value: ApiArtist = response
            .json()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        Ok(Some(value.into()))
    }

    async fn artist_cover_source(
        &self,
        artist: &Artist,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, moosicbox_music_api::Error> {
        let artist_id = &artist.id;
        let url = format!("{host}/files/artists/{artist_id}/source", host = self.host);
        let request = CLIENT
            .request(switchy_http::models::Method::Head, &url)
            .header("moosicbox-profile", &self.profile);

        let response = request
            .send()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        if !response.status().is_success() {
            if response.status() == StatusCode::NotFound {
                return Ok(None);
            }
            return Err(moosicbox_music_api::Error::Other(Box::new(
                RequestError::Unsuccessful(format!("Status {}", response.status())),
            )));
        }

        Ok(Some(ImageCoverSource::RemoteUrl {
            url,
            headers: Some(vec![(
                "moosicbox-profile".to_string(),
                self.profile.clone(),
            )]),
        }))
    }

    async fn albums(
        &self,
        request: &AlbumsRequest,
    ) -> PagingResult<Album, moosicbox_music_api::Error> {
        let url = format!(
            "{host}/menu/albums?offset={offset}&limit={limit}{sort}{sources}{filters}",
            host = self.host,
            offset = request.page.as_ref().map_or(0, |x| x.offset),
            limit = request.page.as_ref().map_or(100, |x| x.limit),
            sort = request
                .sort
                .as_ref()
                .map_or_else(String::new, |x| format!("&sort={x}")),
            sources = request.sources.as_ref().map_or_else(String::new, |x| {
                format!(
                    "&sources={sources}",
                    sources = x
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            }),
            filters = request.filters.as_ref().map_or_else(String::new, |x| {
                format!(
                    "{name}{artist}{search}{album_type}{artist_id}{api_source}",
                    name = x
                        .name
                        .as_ref()
                        .map_or_else(String::new, |x| format!("&name={x}")),
                    artist = x
                        .artist
                        .as_ref()
                        .map_or_else(String::new, |x| format!("&artist={x}")),
                    search = x
                        .search
                        .as_ref()
                        .map_or_else(String::new, |x| format!("&search={x}")),
                    album_type = x
                        .album_type
                        .as_ref()
                        .map_or_else(String::new, |x| format!("&albumType={x}")),
                    artist_id = x
                        .artist_id
                        .as_ref()
                        .map_or_else(String::new, |x| format!("&artistId={x}")),
                    api_source = x
                        .artist_api_id
                        .as_ref()
                        .map_or_else(String::new, |x| format!(
                            "&artistId={}&apiSource={}",
                            x.id, x.source
                        )),
                )
            }),
        );

        let req = CLIENT
            .request(switchy_http::models::Method::Get, &url)
            .header("moosicbox-profile", &self.profile);

        let response = req
            .send()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        if !response.status().is_success() {
            if response.status() == StatusCode::NotFound {
                return Ok(PagingResponse::empty());
            }
            return Err(moosicbox_music_api::Error::Other(Box::new(
                RequestError::Unsuccessful(format!("Status {}", response.status())),
            )));
        }

        let page: Page<ApiAlbum> = response
            .json()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        let page: Page<Result<Album, _>> = page.map(TryInto::try_into);
        let page = page
            .transpose()
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        let page = PagingResponse::new(page, {
            let api = self.clone();
            let request = request.clone();

            move |offset, limit| {
                let api = api.clone();
                let mut request = request.clone();
                request.page = Some(PagingRequest { offset, limit });
                Box::pin(async move { api.albums(&request).await })
            }
        });

        Ok(page)
    }

    async fn album(&self, album_id: &Id) -> Result<Option<Album>, moosicbox_music_api::Error> {
        let request = CLIENT
            .request(
                switchy_http::models::Method::Get,
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
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        if !response.status().is_success() {
            if response.status() == StatusCode::NotFound {
                return Ok(None);
            }
            return Err(moosicbox_music_api::Error::Other(Box::new(
                RequestError::Unsuccessful(format!("Status {}", response.status())),
            )));
        }

        let value: ApiAlbum = response
            .json()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        Ok(Some(value.try_into().map_err(|e| {
            moosicbox_music_api::Error::Other(Box::new(e))
        })?))
    }

    async fn album_versions(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> PagingResult<AlbumVersion, moosicbox_music_api::Error> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(50);

        let request = CLIENT
            .request(
                switchy_http::models::Method::Get,
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
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        if !response.status().is_success() {
            if response.status() == StatusCode::NotFound {
                return Ok(PagingResponse::empty());
            }
            return Err(moosicbox_music_api::Error::Other(Box::new(
                RequestError::Unsuccessful(format!("Status {}", response.status())),
            )));
        }

        let value: Vec<ApiAlbumVersion> = response
            .json()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

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
    ) -> PagingResult<Album, moosicbox_music_api::Error> {
        let request = AlbumsRequest {
            page: Some(PagingRequest {
                offset: offset.unwrap_or(0),
                limit: limit.unwrap_or(100),
            }),
            sources: None,
            sort: None,
            filters: Some(AlbumFilters {
                name: None,
                artist: None,
                search: None,
                album_type,
                artist_id: Some(artist_id.clone()),
                artist_api_id: None,
            }),
        };

        self.albums(&request)
            .await
            .map(|x| x.map_err(|e| moosicbox_music_api::Error::Other(Box::new(e))))
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
    }

    async fn add_album(&self, _album_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Err(moosicbox_music_api::Error::UnsupportedAction(
            "Adding album is not implemented",
        ))
    }

    async fn remove_album(&self, _album_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Err(moosicbox_music_api::Error::UnsupportedAction(
            "Removing album is not implemented",
        ))
    }

    async fn album_cover_source(
        &self,
        album: &Album,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, moosicbox_music_api::Error> {
        let album_id = &album.id;
        let url = format!("{host}/files/albums/{album_id}/source", host = self.host);
        let request = CLIENT
            .request(switchy_http::models::Method::Head, &url)
            .header("moosicbox-profile", &self.profile);

        let response = request
            .send()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        if !response.status().is_success() {
            if response.status() == StatusCode::NotFound {
                return Ok(None);
            }
            return Err(moosicbox_music_api::Error::Other(Box::new(
                RequestError::Unsuccessful(format!("Status {}", response.status())),
            )));
        }

        Ok(Some(ImageCoverSource::RemoteUrl {
            url,
            headers: Some(vec![(
                "moosicbox-profile".to_string(),
                self.profile.clone(),
            )]),
        }))
    }

    async fn tracks(
        &self,
        track_ids: Option<&[Id]>,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, moosicbox_music_api::Error> {
        let Some(track_ids) = track_ids else {
            return Err(moosicbox_music_api::Error::UnsupportedAction(
                "Fetching all tracks is not implemented",
            ));
        };

        let track_ids_str = track_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let request = CLIENT
            .request(
                switchy_http::models::Method::Get,
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
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        if !response.status().is_success() {
            return Err(moosicbox_music_api::Error::Other(Box::new(
                RequestError::Unsuccessful(format!("Status {}", response.status())),
            )));
        }

        let tracks: Vec<Track> = response
            .json::<Vec<ApiTrack>>()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
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
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<TrackOrder>,
        _order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, moosicbox_music_api::Error> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(100);

        let request = CLIENT
            .request(
                switchy_http::models::Method::Get,
                &format!(
                    "{host}/menu/album/tracks?albumId={album_id}&source={source}",
                    host = self.host,
                    source = self.api_source
                ),
            )
            .header("moosicbox-profile", &self.profile);

        let response = request
            .send()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        if !response.status().is_success() {
            if response.status() == StatusCode::NotFound {
                return Ok(PagingResponse::empty());
            }
            return Err(moosicbox_music_api::Error::Other(Box::new(
                RequestError::Unsuccessful(format!("Status {}", response.status())),
            )));
        }

        let value: Vec<ApiTrack> = response
            .json()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        let total = u32::try_from(value.len()).unwrap();

        let tracks = value
            .into_iter()
            .skip(std::cmp::min(offset, total) as usize)
            .take(std::cmp::min(limit, total - offset) as usize)
            .map(Into::into)
            .collect();

        Ok(PagingResponse {
            page: Page::WithTotal {
                items: tracks,
                offset,
                limit,
                total,
            },
            fetch: Arc::new(Mutex::new(Box::new({
                let api = self.clone();
                let album_id = album_id.clone();

                move |offset, limit| {
                    let api = api.clone();
                    let album_id = album_id.clone();

                    Box::pin(async move {
                        api.album_tracks(&album_id, Some(offset), Some(limit), None, None)
                            .await
                    })
                }
            }))),
        })
    }

    async fn track(&self, track_id: &Id) -> Result<Option<Track>, moosicbox_music_api::Error> {
        let track_ids_str = track_id.to_string();

        let request = CLIENT
            .request(
                switchy_http::models::Method::Get,
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
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        if !response.status().is_success() {
            if response.status() == StatusCode::NotFound {
                return Ok(None);
            }
            return Err(moosicbox_music_api::Error::Other(Box::new(
                RequestError::Unsuccessful(format!("Status {}", response.status())),
            )));
        }

        let mut tracks = response
            .json::<Vec<ApiTrack>>()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
            .into_iter()
            .map(Into::into);

        Ok(tracks.next())
    }

    async fn add_track(&self, _track_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Err(moosicbox_music_api::Error::UnsupportedAction(
            "Adding track is not implemented",
        ))
    }

    async fn remove_track(&self, _track_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Err(moosicbox_music_api::Error::UnsupportedAction(
            "Removing track is not implemented",
        ))
    }

    async fn track_source(
        &self,
        track: TrackOrId,
        _quality: TrackAudioQuality,
    ) -> Result<Option<TrackSource>, moosicbox_music_api::Error> {
        let track_id = track.id();
        let url = format!("{host}/files/track?trackId={track_id}", host = self.host);

        Ok(track
            .track(self)
            .await?
            .map(|track| TrackSource::RemoteUrl {
                url,
                format: track.format.unwrap_or(AudioFormat::Source),
                track_id: Some(track.id.clone()),
                source: track.track_source,
                headers: Some(vec![(
                    "moosicbox-profile".to_string(),
                    self.profile.clone(),
                )]),
            }))
    }

    async fn track_size(
        &self,
        _track: TrackOrId,
        _source: &TrackSource,
        _quality: PlaybackQuality,
    ) -> Result<Option<u64>, moosicbox_music_api::Error> {
        Err(moosicbox_music_api::Error::UnsupportedAction(
            "Fetching track size is not implemented",
        ))
    }

    fn supports_search(&self) -> bool {
        true
    }

    async fn search(
        &self,
        query: &str,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> Result<ApiSearchResultsResponse, moosicbox_music_api::Error> {
        let url = format!(
            "{host}/search/global-search?query={query}{offset}{limit}",
            host = self.host,
            query = encode(query),
            offset = offset.map_or_else(String::new, |x| format!("&offset={x}")),
            limit = limit.map_or_else(String::new, |x| format!("&limit={x}")),
        );

        let request = CLIENT
            .request(switchy_http::models::Method::Get, &url)
            .header("moosicbox-profile", &self.profile);

        let response = request
            .send()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        if !response.status().is_success() {
            return Err(moosicbox_music_api::Error::Other(Box::new(
                RequestError::Unsuccessful(format!("Status {}", response.status())),
            )));
        }

        let results = response
            .json::<ApiSearchResultsResponse>()
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        Ok(results)
    }
}
