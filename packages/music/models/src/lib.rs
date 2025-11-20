//! Core data models for music metadata and playback.
//!
//! This crate provides the fundamental data structures for representing music entities
//! (artists, albums, tracks) and their metadata across different API sources. It supports
//! both local library content and external music service APIs.
//!
//! # Main Types
//!
//! * [`Artist`] - Represents a music artist with metadata
//! * [`Album`] - Represents a music album with versions and quality information
//! * [`Track`] - Represents a music track with audio properties and metadata
//! * [`ApiSource`] - Identifies the source of music content (Library, Tidal, Qobuz, etc.)
//! * [`id::Id`] - Flexible identifier supporting both numeric and string IDs
//!
//! # Features
//!
//! * `api` - Enables API-specific model types for serialization
//! * `db` - Enables database integration and query support
//! * Audio format support: `aac`, `flac`, `mp3`, `opus`

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeSet,
    path::PathBuf,
    str::FromStr,
    sync::{LazyLock, RwLock},
};

use id::{ApiId, Id};
use moosicbox_date_utils::chrono::{self, NaiveDateTime, parse_date_time};
use moosicbox_json_utils::{ParseError, ToValueType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum::{AsRefStr, EnumString};

/// Flexible ID types supporting both numeric and string identifiers.
///
/// Provides [`id::Id`] and [`id::ApiId`] types for identifying music entities
/// across different API sources.
pub mod id;

/// API-specific model types for serialization and network transfer.
///
/// Provides lightweight versions of core types optimized for API responses,
/// with `contains_cover` boolean flags instead of full cover URLs.
#[cfg(feature = "api")]
pub mod api;

/// Database integration for model types.
///
/// Provides database value conversions, query support, and model deserialization
/// for the `switchy_database` library.
#[cfg(feature = "db")]
pub mod db;

/// Represents a music artist.
///
/// Contains basic metadata about an artist including their unique identifier,
/// name (title), optional cover artwork, and associated API sources.
#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    /// Unique identifier for the artist
    pub id: Id,
    /// Artist name
    pub title: String,
    /// Optional cover artwork URL
    pub cover: Option<String>,
    /// The primary API source for this artist
    pub api_source: ApiSource,
    /// All API sources where this artist is available
    pub api_sources: ApiSources,
}

/// Sort order for artist listings.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum ArtistSort {
    /// Sort by artist name in ascending order
    NameAsc,
    /// Sort by artist name in descending order
    NameDesc,
}

impl FromStr for ArtistSort {
    type Err = ();

    /// Parses an artist sort string.
    ///
    /// # Errors
    ///
    /// * If the input string doesn't match a known sort variant
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "name-asc" | "name" => Ok(Self::NameAsc),
            "name-desc" => Ok(Self::NameDesc),
            _ => Err(()),
        }
    }
}

/// Global registry of all registered API sources.
///
/// # Panics
///
/// * Methods that read or write to this lock will panic if the `RwLock` is poisoned
pub static API_SOURCES: LazyLock<RwLock<BTreeSet<ApiSource>>> =
    LazyLock::new(|| RwLock::new(BTreeSet::new()));

/// The built-in "Library" API source representing locally stored music.
pub static LIBRARY_API_SOURCE: LazyLock<ApiSource> =
    LazyLock::new(|| ApiSource::register("Library", "Library"));

/// An identifier for a music API source (e.g., "Library", "Tidal", "Qobuz").
///
/// API sources distinguish between different origins of music content within the system.
/// Each source has both an internal ID and a display name.
#[derive(Debug, Eq, PartialEq, Clone, Hash, Ord, PartialOrd)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
// TODO: Potentially make the inner type a `Arc<...>` instead of a `String`
pub struct ApiSource {
    id: String,
    display: String,
}

impl ApiSource {
    /// Registers a new API source and adds it to the global registry.
    ///
    /// # Panics
    ///
    /// * If the `API_SOURCES` `RwLock` is poisoned
    pub fn register(id: impl Into<String>, display: impl Into<String>) -> Self {
        let id = id.into();
        let display = display.into();

        let api_source = Self { id, display };

        API_SOURCES.write().unwrap().insert(api_source.clone());

        api_source
    }

    /// Returns the library API source (same as [`library()`](Self::library)).
    pub fn register_library() -> Self {
        LIBRARY_API_SOURCE.clone()
    }

    /// Returns a clone of the library API source.
    #[must_use]
    pub fn library() -> Self {
        LIBRARY_API_SOURCE.clone()
    }

    /// Returns a static reference to the library API source.
    #[must_use]
    pub fn library_ref() -> &'static Self {
        &LIBRARY_API_SOURCE
    }

    /// Returns `true` if this API source is the library source.
    #[must_use]
    pub fn is_library(&self) -> bool {
        self == &*LIBRARY_API_SOURCE
    }

    /// Returns `true` if this API source's ID matches the given string.
    #[must_use]
    pub fn matches_str(&self, other: &str) -> bool {
        self.id == other
    }

    /// Returns the display name as a `String`.
    #[must_use]
    pub fn to_string_display(&self) -> String {
        self.as_display().to_string()
    }

    /// Returns the display name as a string slice.
    #[must_use]
    pub const fn as_display(&self) -> &str {
        self.display.as_str()
    }
}

impl Serialize for ApiSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.id)
    }
}

impl<'de> Deserialize<'de> for ApiSource {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: String = Deserialize::deserialize(deserializer)?;
        Ok(Self::from_str(&value).unwrap())
    }
}

impl AsRef<str> for ApiSource {
    fn as_ref(&self) -> &str {
        self.id.as_ref()
    }
}

impl Default for ApiSource {
    fn default() -> Self {
        LIBRARY_API_SOURCE.clone()
    }
}

impl TryFrom<&String> for ApiSource {
    type Error = FromStringApiSourceError;

    /// Attempts to convert a string reference to an `ApiSource`.
    ///
    /// # Errors
    ///
    /// * If the string doesn't match any registered API source
    fn try_from(value: &String) -> Result<Self, Self::Error> {
        value.clone().try_into()
    }
}

impl TryFrom<&str> for ApiSource {
    type Error = FromStringApiSourceError;

    /// Attempts to convert a string slice to an `ApiSource`.
    ///
    /// # Errors
    ///
    /// * If the string doesn't match any registered API source
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        value.to_string().try_into()
    }
}

impl TryFrom<String> for ApiSource {
    type Error = FromStringApiSourceError;

    /// Attempts to convert a string to an `ApiSource`.
    ///
    /// # Errors
    ///
    /// * If the string doesn't match any registered API source
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

/// Error returned when attempting to parse an unregistered API source.
#[derive(Debug, thiserror::Error)]
#[error("Invalid ApiSource: '{0}'")]
pub struct FromStringApiSourceError(String);

impl FromStr for ApiSource {
    type Err = FromStringApiSourceError;

    /// Parses an API source string.
    ///
    /// # Errors
    ///
    /// * If the string doesn't match any registered API source
    ///
    /// # Panics
    ///
    /// * If the `API_SOURCES` `RwLock` is poisoned
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        API_SOURCES
            .read()
            .unwrap()
            .iter()
            .find(|k| k.id == value)
            .cloned()
            .ok_or_else(|| FromStringApiSourceError(value.to_string()))
    }
}

impl From<ApiSource> for String {
    fn from(value: ApiSource) -> Self {
        value.id
    }
}

impl ApiSource {
    /// Returns an iterator over all registered API sources.
    ///
    /// # Panics
    ///
    /// * If the `API_SOURCES` `RwLock` is poisoned
    pub fn all() -> impl Iterator<Item = Self> {
        API_SOURCES
            .read()
            .unwrap()
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .into_iter()
    }
}

impl std::fmt::Display for ApiSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// The API source for a specific track, either local or from an external API.
#[derive(Default, Debug, Clone, Ord, PartialOrd, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TrackApiSource {
    /// Track is from local storage
    #[default]
    Local,
    /// Track is from an external API
    Api(ApiSource),
}

impl TrackApiSource {
    /// Returns a static slice containing all possible track API sources.
    #[must_use]
    pub fn all() -> &'static [Self] {
        static ALL: LazyLock<Vec<TrackApiSource>> = LazyLock::new(|| {
            #[allow(unused_mut)]
            let mut all = vec![TrackApiSource::Local];

            all.extend(ApiSource::all().map(TrackApiSource::Api));

            all
        });

        &ALL
    }

    /// Attempts to create a track API source from an API source identifier string.
    #[must_use]
    pub fn for_api_source(source: impl Into<String>) -> Option<Self> {
        ApiSource::try_from(source.into()).ok().map(Self::Api)
    }
}

impl Serialize for TrackApiSource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Local => "LOCAL".serialize(serializer),
            Self::Api(api_source) => format!("API:{api_source}").serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for TrackApiSource {
    /// Deserializes a `TrackApiSource` from a string value.
    ///
    /// # Panics
    ///
    /// * If the value is not a string
    /// * If the string doesn't match the expected format
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: Value = Value::deserialize(deserializer)?;

        Ok(value.as_str().map_or_else(
            || panic!("invalid type"),
            |value| value.try_into().unwrap_or_else(|_| panic!("invalid type")),
        ))
    }
}

impl std::fmt::Display for TrackApiSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local => f.write_str("LOCAL"),
            Self::Api(source) => f.write_fmt(format_args!("API:{source}")),
        }
    }
}

/// Error returned when attempting to parse an invalid track API source string.
#[derive(Debug, thiserror::Error)]
#[error("Invalid track api source: '{0}'")]
pub struct TryFromStringTrackApiSourceError(String);

impl TryFrom<&String> for TrackApiSource {
    type Error = TryFromStringTrackApiSourceError;

    /// Attempts to convert a string reference to a `TrackApiSource`.
    ///
    /// # Errors
    ///
    /// * If the string doesn't match the expected format
    fn try_from(value: &String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl TryFrom<String> for TrackApiSource {
    type Error = TryFromStringTrackApiSourceError;

    /// Attempts to convert a string to a `TrackApiSource`.
    ///
    /// # Errors
    ///
    /// * If the string doesn't match the expected format
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.as_str().try_into()
    }
}

impl TryFrom<&str> for TrackApiSource {
    type Error = TryFromStringTrackApiSourceError;

    /// Attempts to convert a string slice to a `TrackApiSource`.
    ///
    /// # Errors
    ///
    /// * If the string doesn't match the expected format ("LOCAL" or "API:source")
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(if let Some((case, value)) = value.split_once(':') {
            match case {
                "API" => Self::Api(value.try_into().map_err(|e: FromStringApiSourceError| {
                    TryFromStringTrackApiSourceError(e.0)
                })?),
                _ => return Err(TryFromStringTrackApiSourceError(value.into())),
            }
        } else {
            match value {
                "LOCAL" => Self::Local,
                _ => return Err(TryFromStringTrackApiSourceError(value.into())),
            }
        })
    }
}

impl FromStr for TrackApiSource {
    type Err = TryFromStringTrackApiSourceError;

    /// Parses a track API source string.
    ///
    /// # Errors
    ///
    /// * If the string doesn't match the expected format ("LOCAL" or "API:source")
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        value.try_into()
    }
}

impl From<&ApiSource> for TrackApiSource {
    fn from(value: &ApiSource) -> Self {
        value.clone().into()
    }
}

impl From<ApiSource> for TrackApiSource {
    fn from(value: ApiSource) -> Self {
        Self::Api(value)
    }
}

impl From<TrackApiSource> for String {
    fn from(value: TrackApiSource) -> Self {
        match value {
            TrackApiSource::Local => "LOCAL".to_string(),
            TrackApiSource::Api(source) => format!("API:{source}"),
        }
    }
}

impl From<TrackApiSource> for ApiSource {
    fn from(value: TrackApiSource) -> Self {
        match value {
            TrackApiSource::Local => LIBRARY_API_SOURCE.clone(),
            TrackApiSource::Api(source) => source,
        }
    }
}

impl From<AlbumSource> for TrackApiSource {
    fn from(value: AlbumSource) -> Self {
        match value {
            AlbumSource::Local => Self::Local,
            AlbumSource::Api(source) => Self::Api(source),
        }
    }
}

impl ToValueType<TrackApiSource> for &serde_json::Value {
    /// Converts a JSON value to a `TrackApiSource`.
    ///
    /// # Errors
    ///
    /// * If the value is not a string
    /// * If the string doesn't match the expected format
    fn to_value_type(self) -> Result<TrackApiSource, ParseError> {
        TrackApiSource::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("TrackApiSource".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("TrackApiSource".into()))
    }
}

/// Represents a music track with its metadata and audio properties.
#[derive(Default, Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    /// Unique identifier for the track
    pub id: Id,
    /// Track number within the album
    pub number: u32,
    /// Track title
    pub title: String,
    /// Track duration in seconds
    pub duration: f64,
    /// Album name
    pub album: String,
    /// Album identifier
    pub album_id: Id,
    /// Album type (LP, Live, etc.)
    pub album_type: AlbumType,
    /// Release date as ISO 8601 string
    pub date_released: Option<String>,
    /// Date added to library as ISO 8601 string
    pub date_added: Option<String>,
    /// Artist name
    pub artist: String,
    /// Artist identifier
    pub artist_id: Id,
    /// File path to the audio file
    pub file: Option<String>,
    /// Artwork URL
    pub artwork: Option<String>,
    /// Whether to blur the artwork
    pub blur: bool,
    /// File size in bytes
    pub bytes: u64,
    /// Audio format (FLAC, MP3, etc.)
    pub format: Option<AudioFormat>,
    /// Audio bit depth (16, 24, etc.)
    pub bit_depth: Option<u8>,
    /// Audio bitrate in bits per second
    pub audio_bitrate: Option<u32>,
    /// Overall bitrate including container overhead
    pub overall_bitrate: Option<u32>,
    /// Sample rate in Hz (44100, 48000, etc.)
    pub sample_rate: Option<u32>,
    /// Number of audio channels (1 = mono, 2 = stereo, etc.)
    pub channels: Option<u8>,
    /// Source of this track (Local or API)
    pub track_source: TrackApiSource,
    /// The primary API source for this track
    pub api_source: ApiSource,
    /// All API sources where this track is available
    pub sources: ApiSources,
}

#[derive(Default, Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
struct TrackInner {
    id: Id,
    number: u32,
    title: String,
    duration: f64,
    album: String,
    album_id: Id,
    album_type: AlbumType,
    date_released: Option<String>,
    date_added: Option<String>,
    artist: String,
    artist_id: Id,
    file: Option<String>,
    artwork: Option<String>,
    blur: bool,
    bytes: u64,
    format: Option<AudioFormat>,
    bit_depth: Option<u8>,
    audio_bitrate: Option<u32>,
    overall_bitrate: Option<u32>,
    sample_rate: Option<u32>,
    channels: Option<u8>,
    source: TrackApiSource,
    api_ids: Vec<ApiId>,
}

impl<'de> Deserialize<'de> for Track {
    /// Deserializes a `Track` from an internal representation.
    ///
    /// # Errors
    ///
    /// * If deserialization fails
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(TrackInner::deserialize(deserializer)?.into())
    }
}

impl From<TrackInner> for Track {
    fn from(value: TrackInner) -> Self {
        Self {
            id: value.id,
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
            sources: {
                let mut sources = ApiSources::default();
                for api_id in value.api_ids {
                    sources = sources.with_api_id(api_id);
                }
                sources
            },
        }
    }
}

impl Track {
    /// Returns the directory path containing this track's file.
    ///
    /// # Panics
    ///
    /// * If the parent file doesn't exist
    /// * If the parent file name cannot be converted to a `str`
    #[must_use]
    pub fn directory(&self) -> Option<String> {
        self.file
            .as_ref()
            .and_then(|f| PathBuf::from_str(f).ok())
            .map(|p| p.parent().unwrap().to_str().unwrap().to_string())
    }
}

/// Represents the audio quality characteristics of a specific album version.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct AlbumVersionQuality {
    /// Audio format (FLAC, MP3, etc.)
    pub format: Option<AudioFormat>,
    /// Audio bit depth (16, 24, etc.)
    pub bit_depth: Option<u8>,
    /// Sample rate in Hz (44100, 48000, etc.)
    pub sample_rate: Option<u32>,
    /// Number of audio channels (1 = mono, 2 = stereo, etc.)
    pub channels: Option<u8>,
    /// Source of this version (Local or API)
    pub source: TrackApiSource,
}

/// A collection of API source-ID pairs for tracking the same entity across multiple sources.
#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(transparent)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiSources(BTreeSet<ApiId>);

impl ApiSources {
    /// Adds a source-ID pair to this collection.
    pub fn add_source(&mut self, source: ApiSource, id: Id) {
        self.0.insert(ApiId { source, id });
    }

    /// Removes all entries for the specified source.
    pub fn remove_source(&mut self, source: &ApiSource) {
        self.0.retain(|x| &x.source != source);
    }

    /// Adds a source-ID pair if the ID is `Some`.
    pub fn add_source_opt(&mut self, source: ApiSource, id: Option<Id>) {
        if let Some(id) = id {
            self.0.insert(ApiId { source, id });
        }
    }

    /// Returns a new `ApiSources` with the given source-ID pair added.
    #[must_use]
    pub fn with_source(mut self, source: ApiSource, id: Id) -> Self {
        self.0.insert(ApiId { source, id });
        self
    }

    /// Returns a new `ApiSources` with the given source-ID pair added if the ID is `Some`.
    #[must_use]
    pub fn with_source_opt(mut self, source: ApiSource, id: Option<Id>) -> Self {
        if let Some(id) = id {
            self.0.insert(ApiId { source, id });
        }
        self
    }

    /// Returns a new `ApiSources` with the given API ID added.
    #[must_use]
    pub fn with_api_id(mut self, api_id: ApiId) -> Self {
        self.0.insert(api_id);
        self
    }

    /// Returns the ID for the given source, if present.
    #[must_use]
    pub fn get(&self, source: &ApiSource) -> Option<&Id> {
        self.iter().find_map(|x| {
            if &x.source == source {
                Some(&x.id)
            } else {
                None
            }
        })
    }
}

impl ApiSources {
    /// Returns an iterator over the API IDs in this collection.
    pub fn iter(&self) -> std::collections::btree_set::Iter<'_, ApiId> {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a ApiSources {
    type Item = &'a ApiId;
    type IntoIter = std::collections::btree_set::Iter<'a, ApiId>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl IntoIterator for ApiSources {
    type Item = ApiId;
    type IntoIter = std::collections::btree_set::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// The type/category of an album.
#[derive(
    Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumType {
    /// Studio album (LP)
    #[default]
    Lp,
    /// Live recording
    Live,
    /// Compilation album
    Compilations,
    /// EPs and singles
    EpsAndSingles,
    /// Other album types
    Other,
    /// Digital download release
    Download,
}

impl std::fmt::Display for AlbumType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Represents a music album with its metadata and available versions.
#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Album {
    /// Unique identifier for the album
    pub id: Id,
    /// Album title
    pub title: String,
    /// Artist name
    pub artist: String,
    /// Artist identifier
    pub artist_id: Id,
    /// Album type (LP, Live, etc.)
    pub album_type: AlbumType,
    /// Release date
    pub date_released: Option<NaiveDateTime>,
    /// Date added to library
    pub date_added: Option<NaiveDateTime>,
    /// Artwork URL
    pub artwork: Option<String>,
    /// Directory path containing album files
    pub directory: Option<String>,
    /// Whether to blur the artwork
    pub blur: bool,
    /// Available quality versions of this album
    pub versions: Vec<AlbumVersionQuality>,
    /// Source of this album (Local or API)
    pub album_source: AlbumSource,
    /// The primary API source for this album
    pub api_source: ApiSource,
    /// All API sources where the artist is available
    pub artist_sources: ApiSources,
    /// All API sources where this album is available
    pub album_sources: ApiSources,
}

impl TryFrom<&Track> for Album {
    type Error = chrono::ParseError;

    /// Attempts to convert a track reference to an album.
    ///
    /// # Errors
    ///
    /// * If date parsing fails
    fn try_from(value: &Track) -> Result<Self, Self::Error> {
        value.clone().try_into()
    }
}

impl TryFrom<Track> for Album {
    type Error = chrono::ParseError;

    /// Attempts to convert a track to an album.
    ///
    /// # Errors
    ///
    /// * If date parsing fails for `date_released` or `date_added` fields
    fn try_from(value: Track) -> Result<Self, Self::Error> {
        Ok(Self {
            directory: value.directory(),
            id: value.album_id,
            title: value.album,
            artist: value.artist,
            artist_id: value.artist_id,
            album_type: value.album_type,
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
            blur: value.blur,
            versions: vec![],
            album_source: value.track_source.into(),
            api_source: value.api_source,
            artist_sources: value.sources.clone(),
            album_sources: value.sources,
        })
    }
}

/// The API source for an album, either local or from an external API.
#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Default, AsRefStr)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumSource {
    /// Album is from local storage
    #[default]
    Local,
    /// Album is from an external API
    Api(ApiSource),
}

impl From<&ApiSource> for AlbumSource {
    fn from(value: &ApiSource) -> Self {
        value.clone().into()
    }
}

impl From<ApiSource> for AlbumSource {
    fn from(value: ApiSource) -> Self {
        Self::Api(value)
    }
}

impl From<AlbumSource> for ApiSource {
    fn from(value: AlbumSource) -> Self {
        match value {
            AlbumSource::Local => Self::library(),
            AlbumSource::Api(source) => source,
        }
    }
}

impl From<TrackApiSource> for AlbumSource {
    fn from(value: TrackApiSource) -> Self {
        match value {
            TrackApiSource::Local => Self::Local,
            TrackApiSource::Api(source) => Self::Api(source),
        }
    }
}

impl FromStr for AlbumSource {
    type Err = FromStringApiSourceError;

    /// Parses an album source string.
    ///
    /// # Errors
    ///
    /// * If parsing fails for a non-local source
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            source => Ok(Self::Api(source.try_into()?)),
        }
    }
}

impl std::fmt::Display for AlbumSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Sort order for album listings.
#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum AlbumSort {
    /// Sort by artist name in ascending order
    ArtistAsc,
    /// Sort by artist name in descending order
    ArtistDesc,
    /// Sort by album name in ascending order
    NameAsc,
    /// Sort by album name in descending order
    NameDesc,
    /// Sort by release date in ascending order
    ReleaseDateAsc,
    /// Sort by release date in descending order
    ReleaseDateDesc,
    /// Sort by date added in ascending order
    DateAddedAsc,
    /// Sort by date added in descending order
    DateAddedDesc,
}

impl std::fmt::Display for AlbumSort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ArtistAsc => f.write_str("artist"),
            Self::ArtistDesc => f.write_str("artist-desc"),
            Self::NameAsc => f.write_str("name"),
            Self::NameDesc => f.write_str("name-desc"),
            Self::ReleaseDateAsc => f.write_str("release-date"),
            Self::ReleaseDateDesc => f.write_str("release-date-desc"),
            Self::DateAddedAsc => f.write_str("date-added"),
            Self::DateAddedDesc => f.write_str("date-added-desc"),
        }
    }
}

impl FromStr for AlbumSort {
    type Err = ();

    /// Parses an album sort string.
    ///
    /// # Errors
    ///
    /// * If the input string doesn't match a known sort variant
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "artist-asc" | "artist" => Ok(Self::ArtistAsc),
            "artist-desc" => Ok(Self::ArtistDesc),
            "name-asc" | "name" => Ok(Self::NameAsc),
            "name-desc" => Ok(Self::NameDesc),
            "release-date-asc" | "release-date" => Ok(Self::ReleaseDateAsc),
            "release-date-desc" => Ok(Self::ReleaseDateDesc),
            "date-added-asc" | "date-added" => Ok(Self::DateAddedAsc),
            "date-added-desc" => Ok(Self::DateAddedDesc),
            _ => Err(()),
        }
    }
}

/// Audio format for tracks and albums.
#[derive(
    Copy, Debug, Clone, Serialize, Deserialize, EnumString, Default, AsRefStr, PartialEq, Eq,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AudioFormat {
    /// AAC audio format
    #[cfg(feature = "aac")]
    Aac,
    /// FLAC audio format
    #[cfg(feature = "flac")]
    Flac,
    /// MP3 audio format
    #[cfg(feature = "mp3")]
    Mp3,
    /// Opus audio format
    #[cfg(feature = "opus")]
    Opus,
    /// Use the source audio format without transcoding
    #[default]
    Source,
}

impl std::fmt::Display for AudioFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// Converts a file extension to an audio format.
///
/// Returns `None` if the extension is not recognized or if the corresponding
/// feature is not enabled.
#[must_use]
pub fn from_extension_to_audio_format(extension: &str) -> Option<AudioFormat> {
    #[allow(unreachable_code)]
    Some(match extension.to_lowercase().as_str() {
        #[cfg(feature = "flac")]
        "flac" => AudioFormat::Flac,
        #[cfg(feature = "mp3")]
        "mp3" => AudioFormat::Mp3,
        #[cfg(feature = "opus")]
        "opus" => AudioFormat::Opus,
        #[cfg(feature = "aac")]
        "m4a" | "mp4" => AudioFormat::Aac,
        _ => return None,
    })
}

/// Playback quality settings for audio.
#[derive(Copy, Clone, Default, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackQuality {
    /// Desired audio format for playback
    pub format: AudioFormat,
}

/// Represents the size of a track in a specific format.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct TrackSize {
    /// Unique identifier for this track size record
    pub id: u64,
    /// Identifier of the track
    pub track_id: u64,
    /// File size in bytes
    pub bytes: Option<u64>,
    /// Audio format identifier
    pub format: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_api_source_register_and_lookup() {
        let source = ApiSource::register("TestSource", "Test Source");
        assert_eq!(source.as_ref(), "TestSource");
        assert_eq!(source.as_display(), "Test Source");
        assert_eq!(source.to_string_display(), "Test Source");

        // Test lookup
        let found = ApiSource::from_str("TestSource").unwrap();
        assert_eq!(found, source);
    }

    #[test_log::test]
    fn test_api_source_library() {
        let library = ApiSource::library();
        assert!(library.is_library());
        assert!(library.matches_str("Library"));

        let library_ref = ApiSource::library_ref();
        assert_eq!(&library, library_ref);
    }

    #[test_log::test]
    fn test_api_source_from_str_error() {
        let result = ApiSource::from_str("NonExistentSource");
        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.to_string(), "Invalid ApiSource: 'NonExistentSource'");
        }
    }

    #[test_log::test]
    fn test_api_source_all() {
        ApiSource::register("TestSource1", "Test Source 1");
        ApiSource::register("TestSource2", "Test Source 2");

        let all: Vec<_> = ApiSource::all().collect();
        assert!(all.len() >= 3); // At least Library, TestSource1, TestSource2
        assert!(all.iter().any(|s| s.matches_str("Library")));
    }

    #[test_log::test]
    fn test_track_api_source_local_serialization() {
        let source = TrackApiSource::Local;
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(json, r#""LOCAL""#);

        let deserialized: TrackApiSource = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, TrackApiSource::Local);
    }

    #[test_log::test]
    fn test_track_api_source_api_serialization() {
        let api_source = ApiSource::register("Tidal", "Tidal");
        let source = TrackApiSource::Api(api_source.clone());
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(json, r#""API:Tidal""#);

        let deserialized: TrackApiSource = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, TrackApiSource::Api(api_source));
    }

    #[test_log::test]
    fn test_track_api_source_try_from_str() {
        // Test LOCAL
        let local: TrackApiSource = "LOCAL".try_into().unwrap();
        assert_eq!(local, TrackApiSource::Local);

        // Test API:source
        ApiSource::register("Qobuz", "Qobuz");
        let api: TrackApiSource = "API:Qobuz".try_into().unwrap();
        assert!(matches!(api, TrackApiSource::Api(_)));

        // Test invalid format
        let invalid: Result<TrackApiSource, _> = "INVALID".try_into();
        assert!(invalid.is_err());

        // Test invalid API source
        let invalid_api: Result<TrackApiSource, _> = "API:NonExistent".try_into();
        assert!(invalid_api.is_err());
    }

    #[test_log::test]
    fn test_track_api_source_for_api_source() {
        let _api_source = ApiSource::register("Spotify", "Spotify");
        let result = TrackApiSource::for_api_source("Spotify");
        assert!(result.is_some());

        let result = TrackApiSource::for_api_source("NonExistent");
        assert!(result.is_none());
    }

    #[test_log::test]
    fn test_album_source_from_str() {
        let local = AlbumSource::from_str("local").unwrap();
        assert_eq!(local, AlbumSource::Local);

        // Register with lowercase to match from_str behavior (it converts to lowercase)
        ApiSource::register("testapialbum", "testapialbum");
        let api = AlbumSource::from_str("TestAPIAlbum").unwrap();
        assert!(matches!(api, AlbumSource::Api(_)));

        let invalid = AlbumSource::from_str("NonExistentAPI");
        assert!(invalid.is_err());
    }

    #[test_log::test]
    fn test_album_sort_from_str() {
        assert_eq!(AlbumSort::from_str("artist").unwrap(), AlbumSort::ArtistAsc);
        assert_eq!(
            AlbumSort::from_str("artist-asc").unwrap(),
            AlbumSort::ArtistAsc
        );
        assert_eq!(
            AlbumSort::from_str("artist-desc").unwrap(),
            AlbumSort::ArtistDesc
        );
        assert_eq!(AlbumSort::from_str("name").unwrap(), AlbumSort::NameAsc);
        assert_eq!(
            AlbumSort::from_str("name-desc").unwrap(),
            AlbumSort::NameDesc
        );
        assert_eq!(
            AlbumSort::from_str("release-date").unwrap(),
            AlbumSort::ReleaseDateAsc
        );
        assert_eq!(
            AlbumSort::from_str("release-date-desc").unwrap(),
            AlbumSort::ReleaseDateDesc
        );
        assert_eq!(
            AlbumSort::from_str("date-added").unwrap(),
            AlbumSort::DateAddedAsc
        );
        assert_eq!(
            AlbumSort::from_str("date-added-desc").unwrap(),
            AlbumSort::DateAddedDesc
        );

        assert!(AlbumSort::from_str("invalid").is_err());
    }

    #[test_log::test]
    fn test_artist_sort_from_str() {
        assert_eq!(ArtistSort::from_str("name").unwrap(), ArtistSort::NameAsc);
        assert_eq!(
            ArtistSort::from_str("name-asc").unwrap(),
            ArtistSort::NameAsc
        );
        assert_eq!(
            ArtistSort::from_str("name-desc").unwrap(),
            ArtistSort::NameDesc
        );

        assert!(ArtistSort::from_str("invalid").is_err());
    }

    #[test_log::test]
    fn test_api_sources_operations() {
        let mut sources = ApiSources::default();
        let api1 = ApiSource::register("APISrc1", "APISrc1");
        let api2 = ApiSource::register("APISrc2", "APISrc2");

        // Test add_source
        sources.add_source(api1.clone(), Id::from(100));
        sources.add_source(api2.clone(), Id::from(200));

        // Test get
        assert_eq!(sources.get(&api1), Some(&Id::from(100)));
        assert_eq!(sources.get(&api2), Some(&Id::from(200)));

        // Test add_source_opt with Some (adds another entry, get returns first)
        sources.add_source_opt(api1.clone(), Some(Id::from(101)));
        // get() returns the first match found - either 100 or 101
        assert!(sources.get(&api1).is_some());

        // Test add_source_opt with None (should not modify)
        let api3 = ApiSource::register("APISrc3", "APISrc3");
        sources.add_source_opt(api3.clone(), None);
        assert_eq!(sources.get(&api3), None);

        // Test remove_source (removes all entries for that source)
        sources.remove_source(&api1);
        assert_eq!(sources.get(&api1), None);

        // Test with_source
        let sources2 = ApiSources::default().with_source(api1.clone(), Id::from(999));
        assert_eq!(sources2.get(&api1), Some(&Id::from(999)));

        // Test with_source_opt
        let sources3 = ApiSources::default()
            .with_source_opt(api1.clone(), Some(Id::from(888)))
            .with_source_opt(api2.clone(), None);
        assert_eq!(sources3.get(&api1), Some(&Id::from(888)));
        assert_eq!(sources3.get(&api2), None);
    }

    #[test_log::test]
    fn test_api_sources_iteration() {
        let api1 = ApiSource::register("IterAPI1", "IterAPI1");
        let api2 = ApiSource::register("IterAPI2", "IterAPI2");

        let sources = ApiSources::default()
            .with_source(api1, Id::from(1))
            .with_source(api2, Id::from(2));

        let count = sources.iter().count();
        assert_eq!(count, 2);

        // Test IntoIterator for reference
        let count_ref = (&sources).into_iter().count();
        assert_eq!(count_ref, 2);

        // Test IntoIterator for owned value
        let count_owned = sources.into_iter().count();
        assert_eq!(count_owned, 2);
    }

    #[test_log::test]
    fn test_track_directory() {
        let track = Track {
            file: Some("/path/to/music/album/track.flac".to_string()),
            ..Default::default()
        };

        let directory = track.directory();
        assert_eq!(directory, Some("/path/to/music/album".to_string()));

        // Test with no file
        let track_no_file = Track::default();
        assert_eq!(track_no_file.directory(), None);
    }

    #[test_log::test]
    fn test_track_to_album_conversion() {
        let track = Track {
            id: Id::from(1),
            album_id: Id::from(100),
            album: "Test Album".to_string(),
            artist: "Test Artist".to_string(),
            artist_id: Id::from(50),
            album_type: AlbumType::Lp,
            date_released: Some("2023-01-15T00:00:00Z".to_string()),
            date_added: Some("2024-01-01T12:00:00Z".to_string()),
            artwork: Some("artwork.jpg".to_string()),
            blur: false,
            file: Some("/music/album/track.flac".to_string()),
            track_source: TrackApiSource::Local,
            api_source: ApiSource::library(),
            sources: ApiSources::default(),
            ..Default::default()
        };

        let album: Album = track.try_into().unwrap();
        assert_eq!(album.id, Id::from(100));
        assert_eq!(album.title, "Test Album");
        assert_eq!(album.artist, "Test Artist");
        assert_eq!(album.artist_id, Id::from(50));
        assert_eq!(album.album_type, AlbumType::Lp);
        assert!(album.date_released.is_some());
        assert!(album.date_added.is_some());
        assert_eq!(album.artwork, Some("artwork.jpg".to_string()));
        assert!(!album.blur);
        assert_eq!(album.directory, Some("/music/album".to_string()));
    }

    #[test_log::test]
    fn test_track_to_album_conversion_invalid_date() {
        let track = Track {
            date_released: Some("invalid-date".to_string()),
            ..Default::default()
        };

        let result: Result<Album, _> = track.try_into();
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_from_extension_to_audio_format() {
        #[cfg(feature = "flac")]
        assert_eq!(
            from_extension_to_audio_format("flac"),
            Some(AudioFormat::Flac)
        );
        #[cfg(feature = "flac")]
        assert_eq!(
            from_extension_to_audio_format("FLAC"),
            Some(AudioFormat::Flac)
        );

        #[cfg(feature = "mp3")]
        assert_eq!(
            from_extension_to_audio_format("mp3"),
            Some(AudioFormat::Mp3)
        );

        #[cfg(feature = "opus")]
        assert_eq!(
            from_extension_to_audio_format("opus"),
            Some(AudioFormat::Opus)
        );

        #[cfg(feature = "aac")]
        assert_eq!(
            from_extension_to_audio_format("m4a"),
            Some(AudioFormat::Aac)
        );
        #[cfg(feature = "aac")]
        assert_eq!(
            from_extension_to_audio_format("mp4"),
            Some(AudioFormat::Aac)
        );

        assert_eq!(from_extension_to_audio_format("unknown"), None);
        assert_eq!(from_extension_to_audio_format("wav"), None);
    }

    #[test_log::test]
    fn test_album_type_display() {
        assert_eq!(AlbumType::Lp.to_string(), "LP");
        assert_eq!(AlbumType::Live.to_string(), "LIVE");
        assert_eq!(AlbumType::Compilations.to_string(), "COMPILATIONS");
        assert_eq!(AlbumType::EpsAndSingles.to_string(), "EPS_AND_SINGLES");
        assert_eq!(AlbumType::Other.to_string(), "OTHER");
        assert_eq!(AlbumType::Download.to_string(), "DOWNLOAD");
    }

    #[test_log::test]
    fn test_album_sort_display() {
        assert_eq!(AlbumSort::ArtistAsc.to_string(), "artist");
        assert_eq!(AlbumSort::ArtistDesc.to_string(), "artist-desc");
        assert_eq!(AlbumSort::NameAsc.to_string(), "name");
        assert_eq!(AlbumSort::NameDesc.to_string(), "name-desc");
        assert_eq!(AlbumSort::ReleaseDateAsc.to_string(), "release-date");
        assert_eq!(AlbumSort::ReleaseDateDesc.to_string(), "release-date-desc");
        assert_eq!(AlbumSort::DateAddedAsc.to_string(), "date-added");
        assert_eq!(AlbumSort::DateAddedDesc.to_string(), "date-added-desc");
    }

    #[test_log::test]
    fn test_track_api_source_conversions() {
        let api_source = ApiSource::register("ConversionAPI", "ConversionAPI");

        // Test From<ApiSource> for TrackApiSource
        let track_source = TrackApiSource::from(api_source.clone());
        assert!(matches!(track_source, TrackApiSource::Api(_)));

        // Test From<&ApiSource> for TrackApiSource
        let track_source_ref = TrackApiSource::from(&api_source);
        assert!(matches!(track_source_ref, TrackApiSource::Api(_)));

        // Test From<TrackApiSource> for ApiSource
        let back_to_api: ApiSource = track_source.into();
        assert_eq!(back_to_api, api_source);

        // Test Local conversion
        let local_api: ApiSource = TrackApiSource::Local.into();
        assert!(local_api.is_library());
    }

    #[test_log::test]
    fn test_album_source_conversions() {
        let api_source = ApiSource::register("AlbumAPI", "AlbumAPI");

        // Test From<ApiSource> for AlbumSource
        let album_source = AlbumSource::from(api_source.clone());
        assert!(matches!(album_source, AlbumSource::Api(_)));

        // Test From<AlbumSource> for ApiSource
        let back_to_api: ApiSource = album_source.into();
        assert_eq!(back_to_api, api_source);

        // Test From<TrackApiSource> for AlbumSource
        let track_source = TrackApiSource::Api(api_source);
        let album_from_track: AlbumSource = track_source.into();
        assert!(matches!(album_from_track, AlbumSource::Api(_)));

        let track_local = TrackApiSource::Local;
        let album_from_local: AlbumSource = track_local.into();
        assert_eq!(album_from_local, AlbumSource::Local);
    }
}
