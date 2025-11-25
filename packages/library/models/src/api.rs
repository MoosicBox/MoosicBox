//! API-specific model types and conversions for the music library.
//!
//! This module provides API-friendly representations of library entities and conversions
//! between internal library models and external API types. These types are optimized for
//! JSON serialization and API responses.
//!
//! # Main Types
//!
//! * [`ApiLibraryAlbum`] - API representation of a library album
//! * [`ApiLibraryTrack`] - API representation of a library track
//!
//! The module also implements conversions between:
//! * Internal library types (e.g., [`LibraryAlbum`]) and API types (e.g., [`ApiAlbum`])
//! * API library types and general API types used across different music sources

use moosicbox_date_utils::chrono::{self, parse_date_time};
use moosicbox_music_models::{
    Album, AlbumSource, ApiSource, ApiSources, Artist, AudioFormat, Track, TrackApiSource,
    api::{ApiAlbum, ApiAlbumVersionQuality, ApiArtist, ApiTrack},
};
use serde::{Deserialize, Serialize};

use crate::{LibraryAlbum, LibraryAlbumType, LibraryArtist, LibraryTrack};

impl From<LibraryArtist> for ApiArtist {
    /// Converts a library artist to an API artist representation.
    ///
    /// Performs a two-step conversion through the generic `Artist` type.
    fn from(value: LibraryArtist) -> Self {
        let artist: Artist = value.into();
        artist.into()
    }
}

impl From<&LibraryAlbum> for ApiAlbum {
    /// Converts a library album reference to an API album representation.
    ///
    /// Clones the album and delegates to the owned conversion.
    fn from(value: &LibraryAlbum) -> Self {
        value.clone().into()
    }
}

impl From<LibraryAlbum> for ApiAlbum {
    /// Converts a library album to an API album representation.
    ///
    /// Converts file paths to boolean flags (e.g., `contains_cover`) and sets the API source to library.
    fn from(value: LibraryAlbum) -> Self {
        Self {
            album_id: value.id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            contains_cover: value.artwork.is_some(),
            blur: value.blur,
            versions: value.versions,
            album_source: value.source,
            api_source: ApiSource::library(),
            artist_sources: value.artist_sources,
            album_sources: value.album_sources,
        }
    }
}

/// API representation of a library album.
///
/// Optimized for JSON serialization with camelCase field names and boolean flags
/// instead of optional file paths.
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiLibraryAlbum {
    /// Unique identifier for the album
    pub album_id: u64,
    /// Album title
    pub title: String,
    /// Primary artist name
    pub artist: String,
    /// Primary artist ID
    pub artist_id: u64,
    /// Album classification
    pub album_type: LibraryAlbumType,
    /// Whether album artwork is available
    pub contains_cover: bool,
    /// Release date in ISO 8601 format
    pub date_released: Option<String>,
    /// Date added to library in ISO 8601 format
    pub date_added: Option<String>,
    /// Source of the album
    pub source: AlbumSource,
    /// Whether artwork should be blurred
    pub blur: bool,
    /// Available quality versions
    pub versions: Vec<ApiAlbumVersionQuality>,
    /// Cross-references to this album on external services
    pub album_sources: ApiSources,
    /// Cross-references to the artist on external services
    pub artist_sources: ApiSources,
}

impl From<ApiLibraryAlbum> for ApiAlbum {
    /// Converts an API library album to a generic API album representation.
    ///
    /// Converts version qualities and sets the API source to library.
    fn from(value: ApiLibraryAlbum) -> Self {
        Self {
            album_id: value.album_id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            contains_cover: value.contains_cover,
            blur: value.blur,
            versions: value
                .versions
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
            album_source: value.source,
            api_source: ApiSource::library(),
            album_sources: value.album_sources,
            artist_sources: value.artist_sources,
        }
    }
}

impl TryFrom<ApiLibraryAlbum> for Album {
    type Error = chrono::ParseError;

    /// Converts an API library album to a generic album.
    ///
    /// Parses date strings and creates artwork path from album ID if cover exists.
    ///
    /// # Errors
    ///
    /// * If `date_released` or `date_added` contains an invalid date string
    fn try_from(value: ApiLibraryAlbum) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.album_id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
            date_released: value
                .date_released
                .as_deref()
                .map(parse_date_time)
                .transpose()?,
            date_added: value
                .date_added
                .as_deref()
                .map(parse_date_time)
                .transpose()?,
            artwork: if value.contains_cover {
                Some(value.album_id.to_string())
            } else {
                None
            },
            directory: None,
            blur: value.blur,
            versions: vec![],
            album_source: value.source,
            api_source: ApiSource::library(),
            album_sources: value.album_sources,
            artist_sources: value.artist_sources,
        })
    }
}

impl From<LibraryAlbum> for ApiLibraryAlbum {
    /// Converts a library album to an API library album representation.
    ///
    /// Converts file paths to boolean flags and version qualities to API representations.
    fn from(value: LibraryAlbum) -> Self {
        Self {
            album_id: value.id,
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id,
            album_type: value.album_type,
            contains_cover: value.artwork.is_some(),
            date_released: value.date_released,
            date_added: value.date_added,
            source: value.source,
            blur: value.blur,
            versions: value.versions.into_iter().map(Into::into).collect(),
            album_sources: value.album_sources,
            artist_sources: value.artist_sources,
        }
    }
}

impl From<LibraryTrack> for ApiTrack {
    /// Converts a library track to an API track representation.
    ///
    /// Performs a two-step conversion through the generic `Track` type.
    fn from(value: LibraryTrack) -> Self {
        let track: Track = value.into();
        track.into()
    }
}

/// API representation of a library track.
///
/// Optimized for JSON serialization with camelCase field names and detailed
/// audio quality metadata for client applications.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiLibraryTrack {
    /// Unique identifier for the track
    pub track_id: u64,
    /// Track number within the album
    pub number: u32,
    /// Track title
    pub title: String,
    /// Duration in seconds
    pub duration: f64,
    /// Artist name
    pub artist: String,
    /// Artist ID
    pub artist_id: u64,
    /// Album type classification
    pub album_type: LibraryAlbumType,
    /// Release date in ISO 8601 format
    pub date_released: Option<String>,
    /// Date added to library in ISO 8601 format
    pub date_added: Option<String>,
    /// Album title
    pub album: String,
    /// Album ID
    pub album_id: u64,
    /// Whether track artwork is available
    pub contains_cover: bool,
    /// Whether artwork should be blurred
    pub blur: bool,
    /// File size in bytes
    pub bytes: u64,
    /// Audio format
    pub format: Option<AudioFormat>,
    /// Bit depth of the audio
    pub bit_depth: Option<u8>,
    /// Audio bitrate in bits per second
    pub audio_bitrate: Option<u32>,
    /// Overall bitrate including container overhead
    pub overall_bitrate: Option<u32>,
    /// Sample rate in Hz
    pub sample_rate: Option<u32>,
    /// Number of audio channels
    pub channels: Option<u8>,
    /// Source of the track
    pub source: TrackApiSource,
    /// Primary API source
    pub api_source: ApiSource,
}

impl From<&ApiLibraryTrack> for LibraryTrack {
    /// Converts an API library track reference to a library track.
    ///
    /// Clones the track and delegates to the owned conversion.
    fn from(value: &ApiLibraryTrack) -> Self {
        value.clone().into()
    }
}

impl From<ApiLibraryTrack> for LibraryTrack {
    /// Converts an API library track to a library track.
    ///
    /// Sets file and artwork paths to `None` since API representations use boolean flags instead.
    fn from(value: ApiLibraryTrack) -> Self {
        Self {
            id: value.track_id,
            number: value.number,
            title: value.title,
            duration: value.duration,
            album: value.album,
            album_id: value.album_id,
            album_type: value.album_type,
            date_released: value.date_released,
            date_added: value.date_added,
            artist: value.artist,
            artist_id: value.artist_id,
            file: None,
            artwork: None,
            blur: value.blur,
            bytes: value.bytes,
            format: value.format,
            bit_depth: value.bit_depth,
            audio_bitrate: value.audio_bitrate,
            overall_bitrate: value.overall_bitrate,
            sample_rate: value.sample_rate,
            channels: value.channels,
            source: value.source,
            api_source: value.api_source,
            api_sources: ApiSources::default()
                .with_source(ApiSource::library(), value.track_id.into()),
        }
    }
}

impl From<ApiLibraryTrack> for Track {
    /// Converts an API library track to a generic track.
    ///
    /// Sets file and artwork to `None` and API source to library.
    fn from(value: ApiLibraryTrack) -> Self {
        Self {
            id: value.track_id.into(),
            number: value.number,
            title: value.title,
            duration: value.duration,
            album: value.album,
            album_id: value.album_id.into(),
            album_type: value.album_type.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            file: None,
            artwork: None,
            blur: value.blur,
            bytes: value.bytes,
            format: value.format,
            bit_depth: value.bit_depth,
            audio_bitrate: value.audio_bitrate,
            overall_bitrate: value.overall_bitrate,
            sample_rate: value.sample_rate,
            channels: value.channels,
            track_source: value.source,
            api_source: ApiSource::library(),
            sources: ApiSources::default().with_source(ApiSource::library(), value.track_id.into()),
        }
    }
}
