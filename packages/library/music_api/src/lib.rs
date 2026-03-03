//! Music API implementation for local library access.
//!
//! This crate provides a [`MusicApi`] implementation that operates against a local library
//! database, allowing access to artists, albums, tracks, and search functionality without
//! requiring external music services.
//!
//! # Main Types
//!
//! * [`LibraryMusicApi`] - Main API implementation for local library operations
//! * [`profiles::LibraryMusicApiProfiles`] - Manager for multiple library profiles
//!
//! # Example
//!
//! ```rust
//! # use moosicbox_library_music_api::LibraryMusicApi;
//! # use switchy_database::profiles::LibraryDatabase;
//! # fn example(db: LibraryDatabase) {
//! let api = LibraryMusicApi::new(db);
//! // Use the api for music operations...
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    fs::File,
    sync::{Arc, LazyLock},
};

use async_trait::async_trait;
use moosicbox_files::get_content_length;
use moosicbox_library::{
    LibraryAlbumTracksError, LibraryFavoriteAlbumsError, add_favorite_album, add_favorite_artist,
    add_favorite_track, album, album_from_source, album_tracks, album_versions, artist,
    artist_albums,
    db::{self, SetTrackSize, get_artist_by_album_id},
    favorite_albums, favorite_artists, favorite_tracks,
    models::{LibraryAlbum, LibraryAlbumType, LibraryArtist, LibraryTrack},
    remove_favorite_album, remove_favorite_artist, remove_favorite_track, search, track,
};
use moosicbox_menu_models::AlbumVersion;
use moosicbox_music_api::{
    MusicApi, TrackOrId,
    models::{
        AlbumOrder, AlbumOrderDirection, AlbumsRequest, ArtistOrder, ArtistOrderDirection,
        ImageCoverSize, ImageCoverSource, TrackAudioQuality, TrackOrder, TrackOrderDirection,
        TrackSource, search::api::ApiSearchResultsResponse,
    },
};
use moosicbox_music_models::{
    Album, AlbumType, ApiSource, Artist, AudioFormat, LIBRARY_API_SOURCE, PlaybackQuality, Track,
    id::Id,
};
use moosicbox_paging::{Page, PagingResponse, PagingResult};
use moosicbox_scan::ScanOrigin;
use regex::{Captures, Regex};
use switchy_async::sync::Mutex;
use switchy_database::profiles::LibraryDatabase;

/// Profile management for library music API instances.
///
/// This module provides functionality for managing multiple library music API instances
/// across different profiles, allowing applications to work with multiple library databases
/// simultaneously. See [`profiles::LibraryMusicApiProfiles`] for the profile manager.
pub mod profiles;

/// Music API implementation for local library access.
///
/// Provides music API operations against a local library database.
#[derive(Clone)]
pub struct LibraryMusicApi {
    db: LibraryDatabase,
}

impl From<&LibraryMusicApi> for LibraryDatabase {
    /// Converts a reference to `LibraryMusicApi` into a cloned `LibraryDatabase`.
    fn from(value: &LibraryMusicApi) -> Self {
        value.db.clone()
    }
}

impl From<LibraryMusicApi> for LibraryDatabase {
    /// Converts `LibraryMusicApi` into its underlying `LibraryDatabase`.
    fn from(value: LibraryMusicApi) -> Self {
        value.db
    }
}

impl From<LibraryDatabase> for LibraryMusicApi {
    /// Creates a `LibraryMusicApi` from a `LibraryDatabase`.
    fn from(value: LibraryDatabase) -> Self {
        Self { db: value }
    }
}

impl LibraryMusicApi {
    /// Creates a new `LibraryMusicApi` instance.
    #[must_use]
    pub const fn new(db: LibraryDatabase) -> Self {
        Self { db }
    }

    /// Retrieves a library artist by ID.
    ///
    /// # Errors
    ///
    /// * If failed to get the library artist
    pub async fn library_artist(
        &self,
        artist_id: &Id,
    ) -> Result<Option<LibraryArtist>, moosicbox_music_api::Error> {
        Ok(Some(artist(&self.db, artist_id).await.map_err(|e| {
            moosicbox_music_api::Error::Other(Box::new(e))
        })?))
    }

    /// Retrieves the artist associated with a library album.
    ///
    /// # Errors
    ///
    /// * If failed to get the library album artist
    pub async fn library_album_artist(
        &self,
        album_id: &Id,
    ) -> Result<Option<LibraryArtist>, moosicbox_music_api::Error> {
        get_artist_by_album_id(
            &self.db,
            album_id
                .try_into()
                .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?,
        )
        .await
        .map_err(|e| moosicbox_music_api::Error::Other(e.into()))
    }

    /// Retrieves a library album by ID and API source.
    ///
    /// # Errors
    ///
    /// * If failed to get the library album from source
    pub async fn library_album_from_source(
        &self,
        album_id: &Id,
        source: &ApiSource,
    ) -> Result<Option<LibraryAlbum>, moosicbox_music_api::Error> {
        album_from_source(&self.db, album_id, source)
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
    }

    /// Retrieves a library album by ID.
    ///
    /// # Errors
    ///
    /// * If failed to get the library album
    pub async fn library_album(
        &self,
        album_id: &Id,
    ) -> Result<Option<LibraryAlbum>, moosicbox_music_api::Error> {
        album(&self.db, album_id)
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
    }

    /// Retrieves all versions of a library album.
    ///
    /// # Errors
    ///
    /// * If failed to get the library album versions
    pub async fn library_album_versions(
        &self,
        album_id: &Id,
    ) -> Result<Vec<AlbumVersion>, LibraryAlbumTracksError> {
        album_versions(&self.db, album_id).await
    }

    /// Retrieves library albums based on the provided request parameters.
    ///
    /// # Errors
    ///
    /// * If failed to get the library albums
    pub async fn library_albums(
        &self,
        request: &AlbumsRequest,
    ) -> PagingResult<LibraryAlbum, LibraryFavoriteAlbumsError> {
        favorite_albums(&self.db, request).await
    }

    /// Retrieves a library track by ID.
    ///
    /// # Errors
    ///
    /// * If failed to get the library track
    pub async fn library_track(
        &self,
        track_id: &Id,
    ) -> Result<Option<LibraryTrack>, moosicbox_music_api::Error> {
        track(&self.db, track_id)
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(e.into()))
    }

    /// Retrieves tracks from a library album with pagination support.
    ///
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
    /// Returns the API source identifier for this library implementation.
    fn source(&self) -> &ApiSource {
        &LIBRARY_API_SOURCE
    }

    /// Retrieves a paginated list of favorite artists.
    ///
    /// # Errors
    ///
    /// * If database query fails
    /// * If failed to fetch artists from the library
    async fn artists(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<ArtistOrder>,
        order_direction: Option<ArtistOrderDirection>,
    ) -> PagingResult<Artist, moosicbox_music_api::Error> {
        Ok(favorite_artists(
            &self.db,
            offset,
            limit,
            order.map(Into::into),
            order_direction.map(Into::into),
        )
        .await
        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
        .inner_into())
    }

    /// Retrieves a library artist by ID.
    ///
    /// # Errors
    ///
    /// * If database query fails
    /// * If failed to fetch the artist from the library
    async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, moosicbox_music_api::Error> {
        Ok(self
            .library_artist(artist_id)
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
            .map(Into::into))
    }

    /// Adds an artist to the favorite artists list.
    ///
    /// # Errors
    ///
    /// * If database update fails
    async fn add_artist(&self, artist_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        add_favorite_artist(&self.db, artist_id)
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
    }

    /// Removes an artist from the favorite artists list.
    ///
    /// # Errors
    ///
    /// * If database update fails
    async fn remove_artist(&self, artist_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        remove_favorite_artist(&self.db, artist_id)
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
    }

    /// Retrieves the artist associated with an album.
    ///
    /// # Errors
    ///
    /// * If database query fails
    /// * If failed to fetch the album artist from the library
    async fn album_artist(
        &self,
        album_id: &Id,
    ) -> Result<Option<Artist>, moosicbox_music_api::Error> {
        Ok(self.library_album_artist(album_id).await?.map(Into::into))
    }

    /// Gets the cover image source for an artist.
    ///
    /// Returns the local file path to the artist's cover image if available.
    ///
    /// # Errors
    ///
    /// * This implementation does not return errors
    async fn artist_cover_source(
        &self,
        artist: &Artist,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, moosicbox_music_api::Error> {
        Ok(artist.cover.clone().map(ImageCoverSource::LocalFilePath))
    }

    /// Retrieves a paginated list of favorite albums based on the request parameters.
    ///
    /// # Errors
    ///
    /// * If database query fails
    /// * If failed to fetch albums from the library
    /// * If failed to convert library albums to API albums
    async fn albums(
        &self,
        request: &AlbumsRequest,
    ) -> PagingResult<Album, moosicbox_music_api::Error> {
        Ok(self
            .library_albums(request)
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
            .inner_try_into_map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?)
    }

    /// Retrieves a library album by ID.
    ///
    /// # Errors
    ///
    /// * If database query fails
    /// * If failed to fetch the album from the library
    /// * If failed to convert library album to API album
    async fn album(&self, album_id: &Id) -> Result<Option<Album>, moosicbox_music_api::Error> {
        Ok(self
            .library_album(album_id)
            .await?
            .map(TryInto::try_into)
            .transpose()
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?)
    }

    /// Retrieves paginated album versions for a specific album.
    ///
    /// # Errors
    ///
    /// * If database query fails
    /// * If failed to fetch album versions from the library
    ///
    /// # Panics
    ///
    /// * Will panic if the number of album versions exceeds `u32::MAX`
    async fn album_versions(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> PagingResult<AlbumVersion, moosicbox_music_api::Error> {
        let offset = offset.unwrap_or(0);
        let limit = limit.unwrap_or(50);

        let value = self
            .library_album_versions(album_id)
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        let total = u32::try_from(value.len()).unwrap();
        let items = value
            .into_iter()
            .skip(offset as usize)
            .take(std::cmp::min(total - offset, limit) as usize)
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

    /// Retrieves albums by a specific artist with optional filtering and pagination.
    ///
    /// # Errors
    ///
    /// * If database query fails
    /// * If failed to fetch artist albums from the library
    /// * If failed to convert library albums to API albums
    ///
    /// # Panics
    ///
    /// * Will panic if any page in the paging response doesn't have a total
    async fn artist_albums(
        &self,
        artist_id: &Id,
        album_type: Option<AlbumType>,
        offset: Option<u32>,
        limit: Option<u32>,
        _order: Option<AlbumOrder>,
        _order_direction: Option<AlbumOrderDirection>,
    ) -> PagingResult<Album, moosicbox_music_api::Error> {
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
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
            .inner_try_into_map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
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
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

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
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
            .inner_try_into_map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
        })
    }

    /// Adds an album to the favorite albums list.
    ///
    /// # Errors
    ///
    /// * If database update fails
    async fn add_album(&self, album_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        add_favorite_album(&self.db, album_id)
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
    }

    /// Removes an album from the favorite albums list.
    ///
    /// # Errors
    ///
    /// * If database update fails
    async fn remove_album(&self, album_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        remove_favorite_album(&self.db, album_id)
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
    }

    /// Gets the cover image source for an album.
    ///
    /// Returns the local file path to the album's artwork image if available.
    ///
    /// # Errors
    ///
    /// * This implementation does not return errors
    async fn album_cover_source(
        &self,
        album: &Album,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, moosicbox_music_api::Error> {
        Ok(album.artwork.clone().map(ImageCoverSource::LocalFilePath))
    }

    /// Retrieves a paginated list of favorite tracks.
    ///
    /// # Errors
    ///
    /// * If database query fails
    /// * If failed to fetch tracks from the library
    async fn tracks(
        &self,
        track_ids: Option<&[Id]>,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, moosicbox_music_api::Error> {
        Ok(favorite_tracks(
            &self.db,
            track_ids,
            offset,
            limit,
            order.map(Into::into),
            order_direction.map(Into::into),
        )
        .await
        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
        .inner_into())
    }

    /// Retrieves tracks from an album with pagination support.
    ///
    /// # Errors
    ///
    /// * If database query fails
    /// * If failed to fetch album tracks from the library
    async fn album_tracks(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, moosicbox_music_api::Error> {
        Ok(self
            .library_album_tracks(album_id, offset, limit, order, order_direction)
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
            .inner_into())
    }

    /// Retrieves a library track by ID.
    ///
    /// # Errors
    ///
    /// * If database query fails
    /// * If failed to fetch the track from the library
    async fn track(&self, track_id: &Id) -> Result<Option<Track>, moosicbox_music_api::Error> {
        Ok(self
            .library_track(track_id)
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
            .map(Into::into))
    }

    /// Adds a track to the favorite tracks list.
    ///
    /// # Errors
    ///
    /// * If database update fails
    async fn add_track(&self, track_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        add_favorite_track(&self.db, track_id)
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
    }

    /// Removes a track from the favorite tracks list.
    ///
    /// # Errors
    ///
    /// * If database update fails
    async fn remove_track(&self, track_id: &Id) -> Result<(), moosicbox_music_api::Error> {
        remove_favorite_track(&self.db, track_id)
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
    }

    /// Retrieves the source location for a track at the specified quality.
    ///
    /// # Errors
    ///
    /// * If failed to fetch the track from the library
    ///
    /// # Panics
    ///
    /// * Will panic if the regex pattern compilation fails (should never happen with a valid
    ///   static pattern)
    async fn track_source(
        &self,
        track: TrackOrId,
        _quality: TrackAudioQuality,
    ) -> Result<Option<TrackSource>, moosicbox_music_api::Error> {
        static REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"/mnt/(\w+)").unwrap());

        let Some(track) = track.track(self).await? else {
            return Ok(None);
        };
        let mut path = if let Some(file) = &track.file {
            file.clone()
        } else {
            return Ok(None);
        };

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

    /// Calculates or retrieves the size of a track in bytes at the specified quality.
    ///
    /// # Errors
    ///
    /// * If database query fails
    /// * If track ID conversion fails
    /// * If encoding operation fails (when calculating size for encoded formats)
    /// * If failed to get content length from remote URL
    ///
    /// # Panics
    ///
    /// * Will panic if the audio file cannot be opened or if metadata cannot be read when using
    ///   `AudioFormat::Source`
    async fn track_size(
        &self,
        track: TrackOrId,
        source: &TrackSource,
        quality: PlaybackQuality,
    ) -> Result<Option<u64>, moosicbox_music_api::Error> {
        log::debug!(
            "track_size: track_id={} source={source:?} quality={quality:?}",
            track.id()
        );

        if let Some(size) = db::get_track_size(&self.db, track.id(), &quality)
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
        {
            return Ok(Some(size));
        }

        let bytes = match source {
            TrackSource::LocalFilePath { path, .. } => match &quality.format {
                #[cfg(feature = "encoder-aac")]
                AudioFormat::Aac => {
                    let writer = moosicbox_stream_utils::ByteWriter::default();
                    moosicbox_audio_output::encoder::aac::encode_aac_spawn(path, writer.clone())
                        .await
                        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;
                    writer.bytes_written()
                }
                #[cfg(feature = "encoder-flac")]
                AudioFormat::Flac => {
                    return Err(moosicbox_music_api::Error::Other(Box::new(
                        moosicbox_library::TrackSizeError::UnsupportedFormat(quality.format),
                    )));
                }
                #[cfg(feature = "encoder-mp3")]
                AudioFormat::Mp3 => {
                    let writer = moosicbox_stream_utils::ByteWriter::default();
                    moosicbox_audio_output::encoder::mp3::encode_mp3_spawn(path, writer.clone())
                        .await
                        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;
                    writer.bytes_written()
                }
                #[cfg(feature = "encoder-opus")]
                AudioFormat::Opus => {
                    let writer = moosicbox_stream_utils::ByteWriter::default();
                    moosicbox_audio_output::encoder::opus::encode_opus_spawn(path, writer.clone())
                        .await
                        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;
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
                    .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?
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
                track_id: track
                    .id()
                    .try_into()
                    .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?,
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
        .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        Ok(Some(bytes))
    }

    /// Indicates whether this API implementation supports search functionality.
    fn supports_search(&self) -> bool {
        true
    }

    /// Searches the library for artists, albums, and tracks matching the query.
    ///
    /// # Errors
    ///
    /// * If database search query fails
    async fn search(
        &self,
        query: &str,
        offset: Option<u32>,
        limit: Option<u32>,
    ) -> Result<ApiSearchResultsResponse, moosicbox_music_api::Error> {
        let results = search(query, offset, limit, None)
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        Ok(results)
    }

    /// Indicates whether this API implementation supports library scanning.
    fn supports_scan(&self) -> bool {
        true
    }

    /// Enables library scanning for local files.
    ///
    /// # Errors
    ///
    /// * If database update fails
    async fn enable_scan(&self) -> Result<(), moosicbox_music_api::Error> {
        moosicbox_scan::enable_scan_origin(&self.db, &ScanOrigin::Local)
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
    }

    /// Checks whether library scanning is enabled.
    ///
    /// # Errors
    ///
    /// * If database query fails
    async fn scan_enabled(&self) -> Result<bool, moosicbox_music_api::Error> {
        moosicbox_scan::is_scan_origin_enabled(&self.db, &ScanOrigin::Local)
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
    }

    /// Initiates a scan of the local library.
    ///
    /// # Errors
    ///
    /// * If scanner initialization fails
    /// * If scan operation fails
    async fn scan(&self) -> Result<(), moosicbox_music_api::Error> {
        let scanner =
            moosicbox_scan::Scanner::from_origin(&self.db, moosicbox_scan::ScanOrigin::Local)
                .await
                .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))?;

        scanner
            .scan_all_local(&self.db)
            .await
            .map_err(|e| moosicbox_music_api::Error::Other(Box::new(e)))
    }
}

#[cfg(test)]
mod tests {
    use regex::{Captures, Regex};

    /// Helper function that applies Windows path conversion logic.
    /// This mirrors the conversion logic used in `track_source()`.
    fn convert_to_windows_path(regex: &Regex, path: &str) -> String {
        regex
            .replace(path, |caps: &Captures| {
                format!("{}:", caps[1].to_uppercase())
            })
            .replace('/', "\\")
    }

    /// Tests the Windows path conversion logic used in `track_source()`.
    /// Verifies that Unix-style mount paths like "/mnt/c" are correctly
    /// converted to Windows drive letters like "C:".
    #[test_log::test]
    fn test_windows_path_conversion_single_drive() {
        let regex = Regex::new(r"/mnt/(\w+)").unwrap();
        let path = "/mnt/c/Users/test/file.mp3";

        let result = convert_to_windows_path(&regex, path);

        assert_eq!(result, "C:\\Users\\test\\file.mp3");
    }

    /// Tests Windows path conversion with lowercase drive letters.
    /// Ensures that the drive letter is properly uppercased during conversion.
    #[test_log::test]
    fn test_windows_path_conversion_lowercase_drive() {
        let regex = Regex::new(r"/mnt/(\w+)").unwrap();
        let path = "/mnt/d/data/music.flac";

        let result = convert_to_windows_path(&regex, path);

        assert_eq!(result, "D:\\data\\music.flac");
    }

    /// Tests Windows path conversion when no mount point is present.
    /// Verifies that paths without "/mnt/" are still processed correctly
    /// (slashes converted to backslashes).
    #[test_log::test]
    fn test_windows_path_conversion_no_mount() {
        let regex = Regex::new(r"/mnt/(\w+)").unwrap();
        let path = "/some/other/path.mp3";

        let result = convert_to_windows_path(&regex, path);

        assert_eq!(result, "\\some\\other\\path.mp3");
    }

    /// Tests path conversion with multiple directory levels.
    /// Ensures deep directory structures are handled correctly.
    #[test_log::test]
    fn test_windows_path_conversion_deep_directory() {
        let regex = Regex::new(r"/mnt/(\w+)").unwrap();
        let path = "/mnt/e/Music/Albums/2023/Best/track.flac";

        let result = convert_to_windows_path(&regex, path);

        assert_eq!(result, "E:\\Music\\Albums\\2023\\Best\\track.flac");
    }

    /// Tests path conversion with various drive letters.
    /// Ensures all common drive letters are handled correctly.
    #[test_log::test]
    fn test_windows_path_conversion_various_drive_letters() {
        let regex = Regex::new(r"/mnt/(\w+)").unwrap();

        // Test drive A
        assert_eq!(
            convert_to_windows_path(&regex, "/mnt/a/file.mp3"),
            "A:\\file.mp3"
        );

        // Test drive Z
        assert_eq!(
            convert_to_windows_path(&regex, "/mnt/z/file.mp3"),
            "Z:\\file.mp3"
        );

        // Test uppercase in path (should work since \w matches word chars)
        assert_eq!(
            convert_to_windows_path(&regex, "/mnt/C/file.mp3"),
            "C:\\file.mp3"
        );
    }

    /// Tests path conversion with special characters in filename.
    /// Ensures filenames with spaces and special chars are preserved.
    #[test_log::test]
    fn test_windows_path_conversion_special_characters_in_filename() {
        let regex = Regex::new(r"/mnt/(\w+)").unwrap();

        let path = "/mnt/c/Music/Artist Name - Album (2023)/01 - Track Name.flac";
        let result = convert_to_windows_path(&regex, path);

        assert_eq!(
            result,
            "C:\\Music\\Artist Name - Album (2023)\\01 - Track Name.flac"
        );
    }

    /// Tests that the regex only matches the first /mnt/ prefix.
    /// Verifies that subsequent path components are not affected.
    #[test_log::test]
    fn test_windows_path_conversion_only_replaces_first_mnt() {
        let regex = Regex::new(r"/mnt/(\w+)").unwrap();

        // Path that might look confusing but /mnt/ should only be replaced at start
        let path = "/mnt/c/backup/mnt/old/file.mp3";
        let result = convert_to_windows_path(&regex, path);

        // Only the first /mnt/c is replaced, the "mnt" in path stays (as \mnt)
        assert_eq!(result, "C:\\backup\\mnt\\old\\file.mp3");
    }

    /// Tests path conversion with file at root of drive.
    /// Ensures paths directly at drive root work correctly.
    #[test_log::test]
    fn test_windows_path_conversion_file_at_root() {
        let regex = Regex::new(r"/mnt/(\w+)").unwrap();

        let path = "/mnt/d/file.mp3";
        let result = convert_to_windows_path(&regex, path);

        assert_eq!(result, "D:\\file.mp3");
    }

    /// Tests path conversion with empty path after mount point.
    /// Edge case where path ends right after drive letter.
    #[test_log::test]
    fn test_windows_path_conversion_just_drive() {
        let regex = Regex::new(r"/mnt/(\w+)").unwrap();

        let path = "/mnt/c";
        let result = convert_to_windows_path(&regex, path);

        assert_eq!(result, "C:");
    }
}

#[cfg(all(test, feature = "simulator"))]
mod simulator_tests {
    use std::sync::Arc;

    use moosicbox_music_api::{MusicApi, models::ImageCoverSize};
    use moosicbox_music_models::{Album, Artist};
    use switchy_database::{Database, profiles::LibraryDatabase, simulator::SimulationDatabase};

    use super::LibraryMusicApi;

    /// Creates a test `LibraryDatabase` using a simulation database.
    fn create_test_db() -> LibraryDatabase {
        let db = Arc::new(Box::new(SimulationDatabase::new().unwrap()) as Box<dyn Database>);
        LibraryDatabase::from(db)
    }

    /// Tests that `artist_cover_source` returns a `LocalFilePath` when artist has a cover.
    /// This differs from the default `MusicApi` trait which returns `RemoteUrl`.
    #[test_log::test(switchy_async::test)]
    async fn test_artist_cover_source_returns_local_file_path_when_cover_exists() {
        let db = create_test_db();
        let api = LibraryMusicApi::new(db);

        let artist = Artist {
            id: 1.into(),
            title: "Test Artist".into(),
            cover: Some("/path/to/cover.jpg".to_owned()),
            ..Default::default()
        };

        let result = api
            .artist_cover_source(&artist, ImageCoverSize::Max)
            .await
            .unwrap();

        assert!(matches!(
            result,
            Some(moosicbox_music_api::models::ImageCoverSource::LocalFilePath(path))
            if path == "/path/to/cover.jpg"
        ));
    }

    /// Tests that `artist_cover_source` returns `None` when artist has no cover.
    #[test_log::test(switchy_async::test)]
    async fn test_artist_cover_source_returns_none_when_no_cover() {
        let db = create_test_db();
        let api = LibraryMusicApi::new(db);

        let artist = Artist {
            id: 1.into(),
            title: "Test Artist".into(),
            cover: None,
            ..Default::default()
        };

        let result = api
            .artist_cover_source(&artist, ImageCoverSize::Max)
            .await
            .unwrap();

        assert!(result.is_none());
    }

    /// Tests that `album_cover_source` returns a `LocalFilePath` when album has artwork.
    /// This differs from the default `MusicApi` trait which returns `RemoteUrl`.
    #[test_log::test(switchy_async::test)]
    async fn test_album_cover_source_returns_local_file_path_when_artwork_exists() {
        let db = create_test_db();
        let api = LibraryMusicApi::new(db);

        let album = Album {
            id: 1.into(),
            title: "Test Album".into(),
            artwork: Some("/path/to/artwork.jpg".to_owned()),
            ..Default::default()
        };

        let result = api
            .album_cover_source(&album, ImageCoverSize::Max)
            .await
            .unwrap();

        assert!(matches!(
            result,
            Some(moosicbox_music_api::models::ImageCoverSource::LocalFilePath(path))
            if path == "/path/to/artwork.jpg"
        ));
    }

    /// Tests that `album_cover_source` returns `None` when album has no artwork.
    #[test_log::test(switchy_async::test)]
    async fn test_album_cover_source_returns_none_when_no_artwork() {
        let db = create_test_db();
        let api = LibraryMusicApi::new(db);

        let album = Album {
            id: 1.into(),
            title: "Test Album".into(),
            artwork: None,
            ..Default::default()
        };

        let result = api
            .album_cover_source(&album, ImageCoverSize::Max)
            .await
            .unwrap();

        assert!(result.is_none());
    }
}
