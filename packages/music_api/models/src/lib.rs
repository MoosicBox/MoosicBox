//! Data models for music API requests and responses.
//!
//! This crate provides common data structures used across `MoosicBox` music APIs for querying
//! and retrieving music metadata including albums, artists, and tracks. It includes request
//! parameters for filtering and pagination, response models for search results, and types
//! for representing audio sources and quality levels.
//!
//! # Main Features
//!
//! * Request/response models for album, artist, and track queries
//! * Filtering and sorting criteria for music metadata
//! * Audio quality levels and format specifications
//! * Track and image source location types (local files and remote URLs)
//! * Search result models with pagination support (behind `search` feature)
//!
//! # Examples
//!
//! Building an album query request:
//!
//! ```rust
//! use moosicbox_music_api_models::{AlbumsRequest, AlbumFilters};
//! use moosicbox_paging::PagingRequest;
//!
//! let request = AlbumsRequest {
//!     sources: None,
//!     sort: None,
//!     filters: Some(AlbumFilters {
//!         artist: Some("The Beatles".to_string()),
//!         ..Default::default()
//!     }),
//!     page: Some(PagingRequest {
//!         offset: 0,
//!         limit: 20,
//!     }),
//! };
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "search")]
pub mod search;

use moosicbox_music_models::{
    AlbumSort, AlbumSource, AlbumType, AudioFormat, TrackApiSource,
    id::{ApiId, Id},
};
use std::str::FromStr as _;

use moosicbox_json_utils::{MissingValue, ParseError, ToValueType};
use moosicbox_paging::PagingRequest;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use switchy_database::DatabaseValue;

/// Request parameters for fetching albums from the music API.
///
/// This structure contains optional filters, sorting, and pagination parameters
/// for querying albums from various sources.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumsRequest {
    /// Optional list of album sources to query from
    pub sources: Option<Vec<AlbumSource>>,
    /// Optional sorting criteria for the results
    pub sort: Option<AlbumSort>,
    /// Optional filters to apply to the album query
    pub filters: Option<AlbumFilters>,
    /// Optional pagination parameters
    pub page: Option<PagingRequest>,
}

/// Filter criteria for album queries.
///
/// Allows filtering albums by various attributes including name, artist,
/// album type, and identifiers.
#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumFilters {
    /// Filter by album name
    pub name: Option<String>,
    /// Filter by artist name
    pub artist: Option<String>,
    /// General search query across album attributes
    pub search: Option<String>,
    /// Filter by album type (e.g., LP, Single, EP)
    pub album_type: Option<AlbumType>,
    /// Filter by internal artist ID
    pub artist_id: Option<Id>,
    /// Filter by API-specific artist ID
    pub artist_api_id: Option<ApiId>,
}

/// Sorting criteria for artist queries.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ArtistOrder {
    /// Sort by the date the artist was added to the library
    DateAdded,
}

impl std::fmt::Display for ArtistOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Sort direction for artist queries.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ArtistOrderDirection {
    /// Sort in ascending order (A-Z, oldest first)
    Ascending,
    /// Sort in descending order (Z-A, newest first)
    Descending,
}

impl std::fmt::Display for ArtistOrderDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Sorting criteria for album queries.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AlbumOrder {
    /// Sort by the date the album was added to the library
    DateAdded,
}

impl std::fmt::Display for AlbumOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Sort direction for album queries.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AlbumOrderDirection {
    /// Sort in ascending order (A-Z, oldest first)
    Ascending,
    /// Sort in descending order (Z-A, newest first)
    Descending,
}

impl std::fmt::Display for AlbumOrderDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Sorting criteria for track queries.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackOrder {
    /// Sort by the date the track was added to the library
    DateAdded,
}

impl std::fmt::Display for TrackOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Sort direction for track queries.
#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackOrderDirection {
    /// Sort in ascending order (A-Z, oldest first)
    Ascending,
    /// Sort in descending order (Z-A, newest first)
    Descending,
}

impl std::fmt::Display for TrackOrderDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Source location for an audio track.
///
/// Represents either a local file path or a remote URL where audio data can be accessed.
#[derive(Clone, Debug)]
pub enum TrackSource {
    /// Track located on the local filesystem
    LocalFilePath {
        /// Filesystem path to the audio file
        path: String,
        /// Audio format of the file
        format: AudioFormat,
        /// Optional track identifier
        track_id: Option<Id>,
        /// API source that provided this track
        source: TrackApiSource,
    },
    /// Track accessible via remote URL
    RemoteUrl {
        /// URL to fetch the audio data
        url: String,
        /// Audio format of the remote file
        format: AudioFormat,
        /// Optional track identifier
        track_id: Option<Id>,
        /// API source that provided this track
        source: TrackApiSource,
        /// Optional HTTP headers to include in the request
        headers: Option<Vec<(String, String)>>,
    },
}

impl TrackSource {
    /// Returns the audio format of this track source.
    #[must_use]
    pub const fn format(&self) -> AudioFormat {
        match self {
            Self::LocalFilePath { format, .. } | Self::RemoteUrl { format, .. } => *format,
        }
    }

    /// Returns the track ID if available.
    #[must_use]
    pub const fn track_id(&self) -> Option<&Id> {
        match self {
            Self::LocalFilePath { track_id, .. } | Self::RemoteUrl { track_id, .. } => {
                track_id.as_ref()
            }
        }
    }
}

/// Audio quality levels for track encoding.
///
/// Defines the quality tiers available for audio playback, from lossy compression
/// to high-resolution lossless formats.
#[derive(
    Debug, Default, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TrackAudioQuality {
    /// MP3 320 kbps (lossy compression)
    Low,
    /// FLAC 16-bit 44.1kHz (CD quality lossless)
    FlacLossless,
    /// FLAC 24-bit up to 96kHz (high-resolution lossless)
    FlacHiRes,
    /// FLAC 24-bit above 96kHz up to 192kHz (highest resolution lossless)
    #[default]
    FlacHighestRes,
}

impl MissingValue<TrackAudioQuality> for &switchy_database::Row {}
impl ToValueType<TrackAudioQuality> for DatabaseValue {
    /// Converts a database value to a `TrackAudioQuality`.
    ///
    /// # Errors
    ///
    /// Returns `ParseError` if:
    /// * The value is not a string
    /// * The string does not match a valid `TrackAudioQuality` variant
    fn to_value_type(self) -> Result<TrackAudioQuality, ParseError> {
        TrackAudioQuality::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("TrackAudioQuality".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("TrackAudioQuality".into()))
    }
}

impl ToValueType<TrackAudioQuality> for &serde_json::Value {
    /// Converts a JSON value to a `TrackAudioQuality`.
    ///
    /// # Errors
    ///
    /// Returns `ParseError` if:
    /// * The value is not a string
    /// * The string does not match a valid `TrackAudioQuality` variant
    fn to_value_type(self) -> Result<TrackAudioQuality, ParseError> {
        TrackAudioQuality::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("TrackAudioQuality".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("TrackAudioQuality".into()))
    }
}

/// Source location for album/artist cover images.
///
/// Specifies where cover art can be retrieved from, either locally or remotely.
#[derive(Debug, Clone)]
pub enum ImageCoverSource {
    /// Cover image stored on the local filesystem
    LocalFilePath(String),
    /// Cover image accessible via remote URL
    RemoteUrl {
        /// URL to fetch the image
        url: String,
        /// Optional HTTP headers for the request
        headers: Option<Vec<(String, String)>>,
    },
}

/// Predefined size options for cover images.
///
/// Defines standard image dimensions for different use cases, from thumbnails to full resolution.
#[derive(Clone, Copy, Debug)]
pub enum ImageCoverSize {
    /// Maximum resolution (1280px)
    Max,
    /// Large size (640px)
    Large,
    /// Medium size (320px)
    Medium,
    /// Small size (160px)
    Small,
    /// Thumbnail size (80px)
    Thumbnail,
}

impl std::fmt::Display for ImageCoverSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let num: u16 = (*self).into();
        f.write_str(&num.to_string())
    }
}

impl From<ImageCoverSize> for u16 {
    fn from(value: ImageCoverSize) -> Self {
        match value {
            ImageCoverSize::Max => 1280,
            ImageCoverSize::Large => 640,
            ImageCoverSize::Medium => 320,
            ImageCoverSize::Small => 160,
            ImageCoverSize::Thumbnail => 80,
        }
    }
}

impl From<u16> for ImageCoverSize {
    fn from(value: u16) -> Self {
        match value {
            0..=80 => Self::Thumbnail,
            81..=160 => Self::Small,
            161..=320 => Self::Medium,
            321..=640 => Self::Large,
            _ => Self::Max,
        }
    }
}

/// Trait for converting between string representations and ID types.
///
/// This trait allows converting IDs to and from string format for serialization
/// and deserialization purposes.
pub trait FromId {
    /// Converts the ID to its string representation.
    fn as_string(&self) -> String;

    /// Parses a string into an ID.
    ///
    /// # Panics
    ///
    /// Panics if the string cannot be parsed into the ID type (implementation-dependent).
    fn into_id(str: &str) -> Self;
}

impl FromId for String {
    fn as_string(&self) -> String {
        self.clone()
    }

    fn into_id(str: &str) -> Self {
        str.to_string()
    }
}

impl FromId for u64 {
    fn as_string(&self) -> String {
        self.to_string()
    }

    /// # Panics
    ///
    /// Panics if the string cannot be parsed as a valid `u64` integer.
    fn into_id(str: &str) -> Self {
        str.parse::<Self>().unwrap()
    }
}
