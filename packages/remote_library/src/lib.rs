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
use switchy_async::sync::Mutex;
use switchy_http::models::StatusCode;
use thiserror::Error;
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
    /// Base URL of the remote `MoosicBox` server (e.g., `"http://localhost:8000"`).
    host: String,
    /// The API source identifier for this connection.
    api_source: ApiSource,
    /// Profile name to use for authentication/authorization.
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
    /// Returns the API source identifier for this remote library connection.
    fn source(&self) -> &ApiSource {
        &self.api_source
    }

    /// Fetches a paginated list of artists from the remote server.
    ///
    /// # Errors
    ///
    /// * [`RequestError::Request`] - HTTP client error occurred
    /// * [`RequestError::Unsuccessful`] - Server returned unsuccessful status code (other than 404)
    ///
    /// # Panics
    ///
    /// Panics if the number of artists returned exceeds `u32::MAX`.
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

    /// Fetches a single artist by ID from the remote server.
    ///
    /// # Errors
    ///
    /// * [`RequestError::Request`] - HTTP client error occurred
    /// * [`RequestError::Unsuccessful`] - Server returned unsuccessful status code (other than 404)
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

    /// Adds an artist to the library.
    ///
    /// # Errors
    ///
    /// * [`moosicbox_music_api::Error::UnsupportedAction`] - This operation is not supported for remote libraries
    async fn add_artist(&self, _artist_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Err(moosicbox_music_api::Error::UnsupportedAction(
            "Adding artist is not implemented",
        ))
    }

    /// Removes an artist from the library.
    ///
    /// # Errors
    ///
    /// * [`moosicbox_music_api::Error::UnsupportedAction`] - This operation is not supported for remote libraries
    async fn remove_artist(&self, _artist_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Err(moosicbox_music_api::Error::UnsupportedAction(
            "Removing artist is not implemented",
        ))
    }

    /// Fetches the artist associated with a specific album from the remote server.
    ///
    /// # Errors
    ///
    /// * [`RequestError::Request`] - HTTP client error occurred
    /// * [`RequestError::Unsuccessful`] - Server returned unsuccessful status code (other than 404)
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

    /// Fetches the cover image source for a given artist from the remote server.
    ///
    /// # Errors
    ///
    /// * [`RequestError::Request`] - HTTP client error occurred
    /// * [`RequestError::Unsuccessful`] - Server returned unsuccessful status code (other than 404)
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

    /// Fetches a paginated list of albums from the remote server.
    ///
    /// # Errors
    ///
    /// * [`RequestError::Request`] - HTTP client error occurred
    /// * [`RequestError::Unsuccessful`] - Server returned unsuccessful status code (other than 404)
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

    /// Fetches a single album by ID from the remote server.
    ///
    /// # Errors
    ///
    /// * [`RequestError::Request`] - HTTP client error occurred
    /// * [`RequestError::Unsuccessful`] - Server returned unsuccessful status code (other than 404)
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

    /// Fetches all available versions of a specific album from the remote server.
    ///
    /// # Errors
    ///
    /// * [`RequestError::Request`] - HTTP client error occurred
    /// * [`RequestError::Unsuccessful`] - Server returned unsuccessful status code (other than 404)
    ///
    /// # Panics
    ///
    /// Panics if the number of album versions returned exceeds `u32::MAX`.
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

    /// Fetches all albums by a specific artist from the remote server.
    ///
    /// # Errors
    ///
    /// * [`RequestError::Request`] - HTTP client error occurred
    /// * [`RequestError::Unsuccessful`] - Server returned unsuccessful status code (other than 404)
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

    /// Adds an album to the library.
    ///
    /// # Errors
    ///
    /// * [`moosicbox_music_api::Error::UnsupportedAction`] - This operation is not supported for remote libraries
    async fn add_album(&self, _album_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Err(moosicbox_music_api::Error::UnsupportedAction(
            "Adding album is not implemented",
        ))
    }

    /// Removes an album from the library.
    ///
    /// # Errors
    ///
    /// * [`moosicbox_music_api::Error::UnsupportedAction`] - This operation is not supported for remote libraries
    async fn remove_album(&self, _album_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Err(moosicbox_music_api::Error::UnsupportedAction(
            "Removing album is not implemented",
        ))
    }

    /// Fetches the cover image source for a given album from the remote server.
    ///
    /// # Errors
    ///
    /// * [`RequestError::Request`] - HTTP client error occurred
    /// * [`RequestError::Unsuccessful`] - Server returned unsuccessful status code (other than 404)
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

    /// Fetches tracks by their IDs from the remote server.
    ///
    /// # Errors
    ///
    /// * [`moosicbox_music_api::Error::UnsupportedAction`] - When `track_ids` is `None` (fetching all tracks is not supported)
    /// * [`RequestError::Request`] - HTTP client error occurred
    /// * [`RequestError::Unsuccessful`] - Server returned unsuccessful status code
    ///
    /// # Panics
    ///
    /// Panics if the number of tracks returned exceeds `u32::MAX`.
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
            fetch: Arc::new(Mutex::new(Box::new({
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

    /// Fetches all tracks belonging to a specific album from the remote server.
    ///
    /// # Errors
    ///
    /// * [`RequestError::Request`] - HTTP client error occurred
    /// * [`RequestError::Unsuccessful`] - Server returned unsuccessful status code (other than 404)
    ///
    /// # Panics
    ///
    /// Panics if the number of tracks returned exceeds `u32::MAX`.
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

    /// Fetches a single track by ID from the remote server.
    ///
    /// # Errors
    ///
    /// * [`RequestError::Request`] - HTTP client error occurred
    /// * [`RequestError::Unsuccessful`] - Server returned unsuccessful status code (other than 404)
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

    /// Adds a track to the library.
    ///
    /// # Errors
    ///
    /// * [`moosicbox_music_api::Error::UnsupportedAction`] - This operation is not supported for remote libraries
    async fn add_track(&self, _track_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Err(moosicbox_music_api::Error::UnsupportedAction(
            "Adding track is not implemented",
        ))
    }

    /// Removes a track from the library.
    ///
    /// # Errors
    ///
    /// * [`moosicbox_music_api::Error::UnsupportedAction`] - This operation is not supported for remote libraries
    async fn remove_track(&self, _track_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        Err(moosicbox_music_api::Error::UnsupportedAction(
            "Removing track is not implemented",
        ))
    }

    /// Fetches the audio source URL for a given track from the remote server.
    ///
    /// # Errors
    ///
    /// * [`RequestError::Request`] - HTTP client error occurred
    /// * [`RequestError::Unsuccessful`] - Server returned unsuccessful status code (other than 404)
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

    /// Fetches the size of a track file in bytes.
    ///
    /// # Errors
    ///
    /// * [`moosicbox_music_api::Error::UnsupportedAction`] - This operation is not supported for remote libraries
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

    /// Returns whether this API implementation supports search functionality.
    ///
    /// Remote library APIs always support search.
    fn supports_search(&self) -> bool {
        true
    }

    /// Performs a global search across artists, albums, and tracks on the remote server.
    ///
    /// # Errors
    ///
    /// * [`RequestError::Request`] - HTTP client error occurred
    /// * [`RequestError::Unsuccessful`] - Server returned unsuccessful status code
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

#[cfg(test)]
mod tests {
    use super::*;
    use moosicbox_music_models::TrackApiSource;

    /// Creates a test instance of `RemoteLibraryMusicApi` for testing.
    fn create_test_api() -> RemoteLibraryMusicApi {
        RemoteLibraryMusicApi::new(
            "http://localhost:8000".to_string(),
            ApiSource::library(),
            "test-profile".to_string(),
        )
    }

    #[test_log::test]
    fn test_new_creates_instance_with_correct_fields() {
        let host = "http://example.com".to_string();
        let api_source = ApiSource::library();
        let profile = "my-profile".to_string();

        let api = RemoteLibraryMusicApi::new(host.clone(), api_source.clone(), profile.clone());

        assert_eq!(api.host, host);
        assert_eq!(api.api_source, api_source);
        assert_eq!(api.profile, profile);
    }

    #[test_log::test]
    fn test_source_returns_api_source() {
        let api = create_test_api();
        let source = api.source();

        assert_eq!(source, &ApiSource::library());
    }

    #[test_log::test]
    fn test_supports_search_returns_true() {
        let api = create_test_api();

        assert!(api.supports_search());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_add_artist_returns_unsupported_action_error() {
        let api = create_test_api();
        let artist_id = Id::Number(1);

        let result = api.add_artist(&artist_id).await;

        assert!(result.is_err());
        match result {
            Err(moosicbox_music_api::Error::UnsupportedAction(msg)) => {
                assert_eq!(msg, "Adding artist is not implemented");
            }
            _ => panic!("Expected UnsupportedAction error"),
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_remove_artist_returns_unsupported_action_error() {
        let api = create_test_api();
        let artist_id = Id::Number(1);

        let result = api.remove_artist(&artist_id).await;

        assert!(result.is_err());
        match result {
            Err(moosicbox_music_api::Error::UnsupportedAction(msg)) => {
                assert_eq!(msg, "Removing artist is not implemented");
            }
            _ => panic!("Expected UnsupportedAction error"),
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_add_album_returns_unsupported_action_error() {
        let api = create_test_api();
        let album_id = Id::Number(1);

        let result = api.add_album(&album_id).await;

        assert!(result.is_err());
        match result {
            Err(moosicbox_music_api::Error::UnsupportedAction(msg)) => {
                assert_eq!(msg, "Adding album is not implemented");
            }
            _ => panic!("Expected UnsupportedAction error"),
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_remove_album_returns_unsupported_action_error() {
        let api = create_test_api();
        let album_id = Id::Number(1);

        let result = api.remove_album(&album_id).await;

        assert!(result.is_err());
        match result {
            Err(moosicbox_music_api::Error::UnsupportedAction(msg)) => {
                assert_eq!(msg, "Removing album is not implemented");
            }
            _ => panic!("Expected UnsupportedAction error"),
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_add_track_returns_unsupported_action_error() {
        let api = create_test_api();
        let track_id = Id::Number(1);

        let result = api.add_track(&track_id).await;

        assert!(result.is_err());
        match result {
            Err(moosicbox_music_api::Error::UnsupportedAction(msg)) => {
                assert_eq!(msg, "Adding track is not implemented");
            }
            _ => panic!("Expected UnsupportedAction error"),
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_remove_track_returns_unsupported_action_error() {
        let api = create_test_api();
        let track_id = Id::Number(1);

        let result = api.remove_track(&track_id).await;

        assert!(result.is_err());
        match result {
            Err(moosicbox_music_api::Error::UnsupportedAction(msg)) => {
                assert_eq!(msg, "Removing track is not implemented");
            }
            _ => panic!("Expected UnsupportedAction error"),
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_track_size_returns_unsupported_action_error() {
        let api = create_test_api();
        let track_id = Id::Number(1);
        let track_or_id = TrackOrId::Id(track_id);
        let track_source = TrackSource::LocalFilePath {
            path: "/tmp/test.mp3".into(),
            format: AudioFormat::Source,
            track_id: None,
            source: TrackApiSource::Local,
        };
        let quality = PlaybackQuality {
            format: AudioFormat::Source,
        };

        let result = api.track_size(track_or_id, &track_source, quality).await;

        assert!(result.is_err());
        match result {
            Err(moosicbox_music_api::Error::UnsupportedAction(msg)) => {
                assert_eq!(msg, "Fetching track size is not implemented");
            }
            _ => panic!("Expected UnsupportedAction error"),
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_tracks_with_none_track_ids_returns_unsupported_action_error() {
        let api = create_test_api();

        let result = api.tracks(None, None, None, None, None).await;

        assert!(result.is_err());
        match result {
            Err(moosicbox_music_api::Error::UnsupportedAction(msg)) => {
                assert_eq!(msg, "Fetching all tracks is not implemented");
            }
            _ => panic!("Expected UnsupportedAction error"),
        }
    }
}
