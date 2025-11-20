//! Data models for the `MoosicBox` local music library.
//!
//! This crate provides data structures representing music library entities (artists, albums, tracks)
//! and their conversions between internal formats, API representations, and database rows.
//!
//! # Features
//!
//! * `api` - Enables API-specific model types and conversions
//! * `db` - Enables database integration with model-to-row conversions
//! * `openapi` - Adds `OpenAPI` schema derivations for API types
//!
//! # Main Types
//!
//! * [`LibraryArtist`] - Represents an artist in the local library
//! * [`LibraryAlbum`] - Represents an album with metadata and version information
//! * [`LibraryTrack`] - Represents a track with audio quality metadata
//!
//! # Example
//!
//! ```rust
//! use moosicbox_library_models::{LibraryAlbum, LibraryAlbumType};
//! use moosicbox_music_models::AlbumSource;
//!
//! let album = LibraryAlbum {
//!     id: 1,
//!     title: "Example Album".to_string(),
//!     artist: "Example Artist".to_string(),
//!     artist_id: 1,
//!     album_type: LibraryAlbumType::Lp,
//!     source: AlbumSource::Local,
//!     ..Default::default()
//! };
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "api")]
pub mod api;

#[cfg(feature = "db")]
pub mod db;

use std::{path::PathBuf, str::FromStr as _};

use moosicbox_date_utils::chrono::{self, parse_date_time};
use moosicbox_json_utils::{ParseError, ToValueType};
use moosicbox_music_models::{
    Album, AlbumSource, AlbumType, AlbumVersionQuality, ApiSource, ApiSources, Artist, AudioFormat,
    Track, TrackApiSource, id::TryFromIdError,
};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

/// Represents an artist in the local music library.
///
/// Contains artist metadata including title, cover art, and references to API sources
/// for cross-platform music service integration.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct LibraryArtist {
    /// Unique identifier for the artist
    pub id: u64,
    /// Artist name
    pub title: String,
    /// Optional path or URL to artist cover image
    pub cover: Option<String>,
    /// Cross-references to this artist on external music services
    pub api_sources: ApiSources,
}

impl From<LibraryArtist> for Artist {
    /// Converts a library artist to a generic artist.
    ///
    /// Sets the API source to library and preserves all metadata.
    fn from(value: LibraryArtist) -> Self {
        Self {
            id: value.id.into(),
            title: value.title,
            cover: value.cover,
            api_source: ApiSource::library(),
            api_sources: value.api_sources,
        }
    }
}

/// API representation of a library artist.
///
/// Used for JSON serialization in API responses, with camelCase field names
/// and explicit external service IDs.
#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiLibraryArtist {
    /// Unique identifier for the artist
    pub artist_id: u64,
    /// Artist name
    pub title: String,
    /// Whether the artist has cover artwork available
    pub contains_cover: bool,
    /// Tidal service artist ID, if available
    pub tidal_id: Option<u64>,
    /// Qobuz service artist ID, if available
    pub qobuz_id: Option<u64>,
    /// `YouTube` Music artist ID, if available
    pub yt_id: Option<u64>,
}

/// Type classification for library albums.
///
/// Categorizes albums by their release type (LP, live recording, compilation, etc.).
#[derive(
    Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryAlbumType {
    /// Standard long-playing album
    #[default]
    Lp,
    /// Live performance recording
    Live,
    /// Compilation of various tracks
    Compilations,
    /// Extended play or single releases
    EpsAndSingles,
    /// Other album types
    Other,
}

impl From<AlbumType> for LibraryAlbumType {
    /// Converts a generic album type to a library album type.
    ///
    /// Maps both `AlbumType::Other` and `AlbumType::Download` to `LibraryAlbumType::Other`.
    fn from(value: AlbumType) -> Self {
        match value {
            AlbumType::Lp => Self::Lp,
            AlbumType::Live => Self::Live,
            AlbumType::Compilations => Self::Compilations,
            AlbumType::EpsAndSingles => Self::EpsAndSingles,
            AlbumType::Other | AlbumType::Download => Self::Other,
        }
    }
}

impl From<LibraryAlbumType> for AlbumType {
    /// Converts a library album type to a generic album type.
    ///
    /// Maps all library types to their corresponding generic types, with `LibraryAlbumType::Other`
    /// becoming `AlbumType::Other`.
    fn from(value: LibraryAlbumType) -> Self {
        match value {
            LibraryAlbumType::Lp => Self::Lp,
            LibraryAlbumType::Live => Self::Live,
            LibraryAlbumType::Compilations => Self::Compilations,
            LibraryAlbumType::EpsAndSingles => Self::EpsAndSingles,
            LibraryAlbumType::Other => Self::Other,
        }
    }
}

impl ToValueType<LibraryAlbumType> for &serde_json::Value {
    /// Parses a `LibraryAlbumType` from a JSON value.
    ///
    /// # Errors
    ///
    /// * If the value is not a string
    /// * If the string cannot be parsed as a valid `LibraryAlbumType`
    fn to_value_type(self) -> Result<LibraryAlbumType, ParseError> {
        LibraryAlbumType::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("AlbumType".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("AlbumType".into()))
    }
}

/// Represents an album in the local music library.
///
/// Contains comprehensive album metadata including artist information, release dates,
/// artwork, quality versions, and cross-references to external music services.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct LibraryAlbum {
    /// Unique identifier for the album
    pub id: u64,
    /// Album title
    pub title: String,
    /// Primary artist name
    pub artist: String,
    /// Primary artist ID
    pub artist_id: u64,
    /// Album classification (LP, live, compilation, etc.)
    pub album_type: LibraryAlbumType,
    /// Release date in ISO 8601 format
    pub date_released: Option<String>,
    /// Date added to library in ISO 8601 format
    pub date_added: Option<String>,
    /// Path or URL to album artwork
    pub artwork: Option<String>,
    /// Directory path containing album files
    pub directory: Option<String>,
    /// Source of the album (local, streaming service, etc.)
    pub source: AlbumSource,
    /// Whether artwork should be blurred (e.g., for explicit content)
    pub blur: bool,
    /// Available quality versions of this album
    pub versions: Vec<AlbumVersionQuality>,
    /// Cross-references to this album on external services
    pub album_sources: ApiSources,
    /// Cross-references to the artist on external services
    pub artist_sources: ApiSources,
}

impl TryFrom<LibraryAlbum> for Album {
    type Error = chrono::ParseError;

    /// Converts a library album to a generic album.
    ///
    /// Parses date strings and sets the API source to library.
    ///
    /// # Errors
    ///
    /// * If `date_released` or `date_added` contains an invalid date string
    fn try_from(value: LibraryAlbum) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id.into(),
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
            artwork: value.artwork,
            directory: value.directory,
            blur: value.blur,
            versions: value.versions,
            album_source: value.source,
            api_source: ApiSource::library(),
            artist_sources: value.artist_sources,
            album_sources: value.album_sources,
        })
    }
}

impl TryFrom<Album> for LibraryAlbum {
    type Error = TryFromIdError;

    /// Converts a generic album to a library album.
    ///
    /// Converts date values to RFC3339 strings and sets the source to Local.
    ///
    /// # Errors
    ///
    /// * If album or artist ID cannot be converted to `u64`
    fn try_from(value: Album) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id.try_into()?,
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.try_into()?,
            album_type: value.album_type.into(),
            date_released: value.date_released.map(|x| x.and_utc().to_rfc3339()),
            date_added: value.date_added.map(|x| x.and_utc().to_rfc3339()),
            artwork: value.artwork,
            directory: value.directory,
            blur: value.blur,
            versions: value.versions,
            source: AlbumSource::Local,
            album_sources: value.album_sources,
            artist_sources: value.artist_sources,
        })
    }
}

/// Sorts album versions by source, bit depth (descending), and sample rate (descending).
///
/// Performs a multi-level sort with priority:
/// 1. Source (ascending)
/// 2. Bit depth (descending - higher quality first)
/// 3. Sample rate (descending - higher quality first)
///
/// # Examples
///
/// ```rust
/// use moosicbox_library_models::sort_album_versions;
/// use moosicbox_music_models::{AlbumVersionQuality, TrackApiSource};
///
/// let mut versions = vec![
///     AlbumVersionQuality {
///         format: None,
///         bit_depth: Some(16),
///         sample_rate: Some(44100),
///         channels: None,
///         source: TrackApiSource::Local,
///     },
///     AlbumVersionQuality {
///         format: None,
///         bit_depth: Some(24),
///         sample_rate: Some(96000),
///         channels: None,
///         source: TrackApiSource::Local,
///     },
/// ];
///
/// sort_album_versions(&mut versions);
/// assert_eq!(versions[0].bit_depth, Some(24));
/// ```
pub fn sort_album_versions(versions: &mut [AlbumVersionQuality]) {
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

/// Represents a track in the local music library.
///
/// Contains detailed track metadata including audio quality information, album/artist
/// relationships, and file location data.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct LibraryTrack {
    /// Unique identifier for the track
    pub id: u64,
    /// Track number within the album
    pub number: u32,
    /// Track title
    pub title: String,
    /// Duration in seconds
    pub duration: f64,
    /// Album title
    pub album: String,
    /// Album ID
    pub album_id: u64,
    /// Album type classification
    pub album_type: LibraryAlbumType,
    /// Release date in ISO 8601 format
    pub date_released: Option<String>,
    /// Date added to library in ISO 8601 format
    pub date_added: Option<String>,
    /// Artist name
    pub artist: String,
    /// Artist ID
    pub artist_id: u64,
    /// Path to the audio file
    pub file: Option<String>,
    /// Path or URL to track artwork
    pub artwork: Option<String>,
    /// Whether artwork should be blurred
    pub blur: bool,
    /// File size in bytes
    pub bytes: u64,
    /// Audio format (FLAC, MP3, etc.)
    pub format: Option<AudioFormat>,
    /// Bit depth of the audio (16, 24, etc.)
    pub bit_depth: Option<u8>,
    /// Audio bitrate in bits per second
    pub audio_bitrate: Option<u32>,
    /// Overall bitrate including container overhead
    pub overall_bitrate: Option<u32>,
    /// Sample rate in Hz (44100, 48000, etc.)
    pub sample_rate: Option<u32>,
    /// Number of audio channels
    pub channels: Option<u8>,
    /// Source of the track (local file, streaming, etc.)
    pub source: TrackApiSource,
    /// Primary API source
    pub api_source: ApiSource,
    /// Cross-references to this track on external services
    pub api_sources: ApiSources,
}

impl LibraryTrack {
    /// Returns the directory path containing the track file.
    ///
    /// Extracts the parent directory from the track's file path.
    ///
    /// # Panics
    ///
    /// * If the file path has no parent directory
    /// * If the parent path contains invalid UTF-8
    #[must_use]
    pub fn directory(&self) -> Option<String> {
        self.file
            .as_ref()
            .and_then(|f| PathBuf::from_str(f).ok())
            .map(|p| p.parent().unwrap().to_str().unwrap().to_string())
    }
}

impl From<LibraryTrack> for Track {
    /// Converts a library track to a generic track.
    ///
    /// Sets the API source to library and preserves all metadata including audio quality information.
    fn from(value: LibraryTrack) -> Self {
        Self {
            id: value.id.into(),
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
            file: value.file,
            artwork: value.artwork,
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
            sources: value.api_sources,
        }
    }
}
