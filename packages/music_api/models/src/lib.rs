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
    /// Formats the artist order as its string representation.
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
    /// Formats the artist order direction as its string representation.
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
    /// Formats the album order as its string representation.
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
    /// Formats the album order direction as its string representation.
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
    /// Formats the track order as its string representation.
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
    /// Formats the track order direction as its string representation.
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// Formats the image cover size as its pixel dimension value.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let num: u16 = (*self).into();
        f.write_str(&num.to_string())
    }
}

impl From<ImageCoverSize> for u16 {
    /// Converts an image cover size to its pixel dimension value.
    ///
    /// Returns the maximum dimension in pixels for each size tier.
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
    /// Converts a pixel dimension to the appropriate image cover size tier.
    ///
    /// Selects the size tier based on the pixel value:
    /// * 0-80px: Thumbnail
    /// * 81-160px: Small
    /// * 161-320px: Medium
    /// * 321-640px: Large
    /// * 641+px: Max
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
    /// Returns a clone of the string ID.
    fn as_string(&self) -> String {
        self.clone()
    }

    /// Converts a string slice to a `String` ID.
    fn into_id(str: &str) -> Self {
        str.to_string()
    }
}

impl FromId for u64 {
    /// Converts the `u64` ID to its string representation.
    fn as_string(&self) -> String {
        self.to_string()
    }

    /// Parses a string slice into a `u64` ID.
    ///
    /// # Panics
    ///
    /// Panics if the string cannot be parsed as a valid `u64` integer.
    fn into_id(str: &str) -> Self {
        str.parse::<Self>().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod image_cover_size {
        use super::*;

        #[test]
        fn test_from_u16_boundaries() {
            assert_eq!(ImageCoverSize::from(0_u16), ImageCoverSize::Thumbnail);
            assert_eq!(ImageCoverSize::from(80_u16), ImageCoverSize::Thumbnail);
            assert_eq!(ImageCoverSize::from(81_u16), ImageCoverSize::Small);
            assert_eq!(ImageCoverSize::from(160_u16), ImageCoverSize::Small);
            assert_eq!(ImageCoverSize::from(161_u16), ImageCoverSize::Medium);
            assert_eq!(ImageCoverSize::from(320_u16), ImageCoverSize::Medium);
            assert_eq!(ImageCoverSize::from(321_u16), ImageCoverSize::Large);
            assert_eq!(ImageCoverSize::from(640_u16), ImageCoverSize::Large);
            assert_eq!(ImageCoverSize::from(641_u16), ImageCoverSize::Max);
            assert_eq!(ImageCoverSize::from(1280_u16), ImageCoverSize::Max);
            assert_eq!(ImageCoverSize::from(5000_u16), ImageCoverSize::Max);
        }

        #[test]
        fn test_into_u16() {
            assert_eq!(u16::from(ImageCoverSize::Thumbnail), 80);
            assert_eq!(u16::from(ImageCoverSize::Small), 160);
            assert_eq!(u16::from(ImageCoverSize::Medium), 320);
            assert_eq!(u16::from(ImageCoverSize::Large), 640);
            assert_eq!(u16::from(ImageCoverSize::Max), 1280);
        }

        #[test]
        fn test_display() {
            assert_eq!(ImageCoverSize::Thumbnail.to_string(), "80");
            assert_eq!(ImageCoverSize::Small.to_string(), "160");
            assert_eq!(ImageCoverSize::Medium.to_string(), "320");
            assert_eq!(ImageCoverSize::Large.to_string(), "640");
            assert_eq!(ImageCoverSize::Max.to_string(), "1280");
        }

        #[test]
        fn test_roundtrip_conversion() {
            for size in &[
                ImageCoverSize::Thumbnail,
                ImageCoverSize::Small,
                ImageCoverSize::Medium,
                ImageCoverSize::Large,
                ImageCoverSize::Max,
            ] {
                let value: u16 = (*size).into();
                let back: ImageCoverSize = value.into();
                assert_eq!(*size as u8, back as u8);
            }
        }
    }

    mod track_audio_quality {
        use super::*;
        use serde_json::json;

        #[test]
        fn test_to_value_type_from_database_value_valid() {
            let db_value = DatabaseValue::String("LOW".to_string());
            let result: Result<TrackAudioQuality, ParseError> = db_value.to_value_type();
            assert_eq!(result.unwrap(), TrackAudioQuality::Low);

            let db_value = DatabaseValue::String("FLAC_LOSSLESS".to_string());
            let result: Result<TrackAudioQuality, ParseError> = db_value.to_value_type();
            assert_eq!(result.unwrap(), TrackAudioQuality::FlacLossless);

            let db_value = DatabaseValue::String("FLAC_HI_RES".to_string());
            let result: Result<TrackAudioQuality, ParseError> = db_value.to_value_type();
            assert_eq!(result.unwrap(), TrackAudioQuality::FlacHiRes);

            let db_value = DatabaseValue::String("FLAC_HIGHEST_RES".to_string());
            let result: Result<TrackAudioQuality, ParseError> = db_value.to_value_type();
            assert_eq!(result.unwrap(), TrackAudioQuality::FlacHighestRes);
        }

        #[test]
        fn test_to_value_type_from_database_value_invalid_type() {
            let db_value = DatabaseValue::Int32(123);
            let result: Result<TrackAudioQuality, ParseError> = db_value.to_value_type();
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
        }

        #[test]
        fn test_to_value_type_from_database_value_invalid_string() {
            let db_value = DatabaseValue::String("INVALID_QUALITY".to_string());
            let result: Result<TrackAudioQuality, ParseError> = db_value.to_value_type();
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
        }

        #[test]
        fn test_to_value_type_from_json_value_valid() {
            let json_value = json!("LOW");
            let result: Result<TrackAudioQuality, ParseError> = (&json_value).to_value_type();
            assert_eq!(result.unwrap(), TrackAudioQuality::Low);

            let json_value = json!("FLAC_LOSSLESS");
            let result: Result<TrackAudioQuality, ParseError> = (&json_value).to_value_type();
            assert_eq!(result.unwrap(), TrackAudioQuality::FlacLossless);
        }

        #[test]
        fn test_to_value_type_from_json_value_invalid_type() {
            let json_value = json!(123);
            let result: Result<TrackAudioQuality, ParseError> = (&json_value).to_value_type();
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
        }

        #[test]
        fn test_to_value_type_from_json_value_invalid_string() {
            let json_value = json!("NOT_A_QUALITY");
            let result: Result<TrackAudioQuality, ParseError> = (&json_value).to_value_type();
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), ParseError::ConvertType(_)));
        }

        #[test]
        fn test_default() {
            assert_eq!(
                TrackAudioQuality::default(),
                TrackAudioQuality::FlacHighestRes
            );
        }
    }

    mod from_id {
        use super::*;

        #[test]
        fn test_string_as_string() {
            let id = "test-id-123".to_string();
            assert_eq!(id.as_string(), "test-id-123");
        }

        #[test]
        fn test_string_into_id() {
            let result = String::into_id("another-id");
            assert_eq!(result, "another-id");
        }

        #[test]
        fn test_u64_as_string() {
            let id: u64 = 12345;
            assert_eq!(id.as_string(), "12345");

            let id: u64 = 0;
            assert_eq!(id.as_string(), "0");

            let id: u64 = u64::MAX;
            assert_eq!(id.as_string(), "18446744073709551615");
        }

        #[test]
        fn test_u64_into_id_valid() {
            assert_eq!(u64::into_id("12345"), 12345_u64);
            assert_eq!(u64::into_id("0"), 0_u64);
            assert_eq!(u64::into_id("18446744073709551615"), u64::MAX);
        }

        #[test]
        #[should_panic]
        fn test_u64_into_id_invalid_panics() {
            u64::into_id("not-a-number");
        }

        #[test]
        #[should_panic]
        fn test_u64_into_id_negative_panics() {
            u64::into_id("-123");
        }

        #[test]
        #[should_panic]
        fn test_u64_into_id_overflow_panics() {
            u64::into_id("18446744073709551616"); // u64::MAX + 1
        }
    }

    mod track_source {
        use super::*;

        #[test]
        fn test_format_local_file() {
            let source = TrackSource::LocalFilePath {
                path: "/path/to/file.flac".to_string(),
                format: AudioFormat::Source,
                track_id: Some(Id::Number(123)),
                source: TrackApiSource::Local,
            };
            assert_eq!(source.format(), AudioFormat::Source);
        }

        #[test]
        fn test_format_remote_url() {
            let source = TrackSource::RemoteUrl {
                url: "https://example.com/track.mp3".to_string(),
                format: AudioFormat::Source,
                track_id: Some(Id::Number(456)),
                source: TrackApiSource::Local,
                headers: None,
            };
            assert_eq!(source.format(), AudioFormat::Source);
        }

        #[test]
        fn test_track_id_some() {
            let source = TrackSource::LocalFilePath {
                path: "/path/to/file.flac".to_string(),
                format: AudioFormat::Source,
                track_id: Some(Id::Number(789)),
                source: TrackApiSource::Local,
            };
            assert_eq!(source.track_id(), Some(&Id::Number(789)));
        }

        #[test]
        fn test_track_id_none() {
            let source = TrackSource::RemoteUrl {
                url: "https://example.com/track.mp3".to_string(),
                format: AudioFormat::Source,
                track_id: None,
                source: TrackApiSource::Local,
                headers: None,
            };
            assert_eq!(source.track_id(), None);
        }
    }

    mod enum_display {
        use super::*;

        #[test]
        fn test_artist_order_display() {
            assert_eq!(ArtistOrder::DateAdded.to_string(), "DATE_ADDED");
        }

        #[test]
        fn test_artist_order_direction_display() {
            assert_eq!(ArtistOrderDirection::Ascending.to_string(), "ASCENDING");
            assert_eq!(ArtistOrderDirection::Descending.to_string(), "DESCENDING");
        }

        #[test]
        fn test_album_order_display() {
            assert_eq!(AlbumOrder::DateAdded.to_string(), "DATE_ADDED");
        }

        #[test]
        fn test_album_order_direction_display() {
            assert_eq!(AlbumOrderDirection::Ascending.to_string(), "ASCENDING");
            assert_eq!(AlbumOrderDirection::Descending.to_string(), "DESCENDING");
        }

        #[test]
        fn test_track_order_display() {
            assert_eq!(TrackOrder::DateAdded.to_string(), "DATE_ADDED");
        }

        #[test]
        fn test_track_order_direction_display() {
            assert_eq!(TrackOrderDirection::Ascending.to_string(), "ASCENDING");
            assert_eq!(TrackOrderDirection::Descending.to_string(), "DESCENDING");
        }
    }
}
