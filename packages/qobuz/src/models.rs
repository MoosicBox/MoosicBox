//! Data models for Qobuz API responses and internal representations.
//!
//! Contains types for albums, artists, tracks, images, genres, and search results,
//! along with conversions to standard MoosicBox music models.

use std::{fmt::Display, str::FromStr as _};

use chrono::{DateTime, Utc};
use moosicbox_date_utils::chrono::parse_date_time;
use moosicbox_json_utils::{
    ParseError, ToValueType,
    database::AsModelResult,
    serde_json::{ToNestedValue, ToValue},
};
use moosicbox_music_api::models::{
    ImageCoverSize,
    search::api::{
        ApiGlobalAlbumSearchResult, ApiGlobalArtistSearchResult, ApiGlobalSearchResult,
        ApiGlobalTrackSearchResult, ApiSearchResultsResponse,
    },
};
use moosicbox_music_models::{
    Album, ApiSources, Artist, Track,
    api::{ApiAlbum, ApiArtist},
    id::TryFromIdError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{API_SOURCE, QobuzAlbumReleaseType, format_title};

/// Represents image URLs at different sizes for Qobuz album and artist artwork.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzImage {
    /// Thumbnail size image URL (100x100).
    pub thumbnail: Option<String>,
    /// Small size image URL (300x300).
    pub small: Option<String>,
    /// Medium size image URL (600x600).
    pub medium: Option<String>,
    /// Large size image URL (1200x1200).
    pub large: Option<String>,
    /// Extra large size image URL (2400x2400).
    pub extralarge: Option<String>,
    /// Mega size image URL (4800x4800).
    pub mega: Option<String>,
}

/// Image size variants for Qobuz artwork, with pixel dimensions indicated in comments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QobuzImageSize {
    /// Mega size (4800x4800 pixels).
    Mega,
    /// Extra large size (2400x2400 pixels).
    ExtraLarge,
    /// Large size (1200x1200 pixels).
    Large,
    /// Medium size (600x600 pixels).
    Medium,
    /// Small size (300x300 pixels).
    Small,
    /// Thumbnail size (100x100 pixels).
    Thumbnail,
}

impl From<ImageCoverSize> for QobuzImageSize {
    fn from(value: ImageCoverSize) -> Self {
        match value {
            ImageCoverSize::Max => Self::Mega,
            ImageCoverSize::Large => Self::Large,
            ImageCoverSize::Medium => Self::Medium,
            ImageCoverSize::Small => Self::Small,
            ImageCoverSize::Thumbnail => Self::Thumbnail,
        }
    }
}

impl From<QobuzImageSize> for u16 {
    fn from(value: QobuzImageSize) -> Self {
        match value {
            QobuzImageSize::Mega => 4800,
            QobuzImageSize::ExtraLarge => 2400,
            QobuzImageSize::Large => 1200,
            QobuzImageSize::Medium => 600,
            QobuzImageSize::Small => 300,
            QobuzImageSize::Thumbnail => 100,
        }
    }
}

impl From<u16> for QobuzImageSize {
    fn from(value: u16) -> Self {
        match value {
            0..=100 => Self::Thumbnail,
            101..=300 => Self::Small,
            301..=600 => Self::Medium,
            601..=1200 => Self::Large,
            1201..=2400 => Self::ExtraLarge,
            _ => Self::Mega,
        }
    }
}

impl Display for QobuzImageSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{}", Into::<u16>::into(*self)))
    }
}

impl QobuzImage {
    /// Returns the highest quality available cover URL (Mega size preferred).
    ///
    /// Falls back to the next best available size if Mega is unavailable.
    #[must_use]
    pub fn cover_url(&self) -> Option<String> {
        self.cover_url_for_size(QobuzImageSize::Mega)
    }

    /// Returns a cover URL for the specified size, falling back to available alternatives.
    ///
    /// The fallback order prioritizes sizes closest to the requested size, preferring
    /// higher quality over lower quality when the exact size is unavailable.
    #[must_use]
    pub fn cover_url_for_size(&self, size: QobuzImageSize) -> Option<String> {
        match size {
            QobuzImageSize::Thumbnail => self
                .thumbnail
                .clone()
                .or_else(|| self.small.clone())
                .or_else(|| self.medium.clone())
                .or_else(|| self.large.clone())
                .or_else(|| self.extralarge.clone())
                .or_else(|| self.mega.clone()),

            QobuzImageSize::Small => self
                .small
                .clone()
                .or_else(|| self.medium.clone())
                .or_else(|| self.large.clone())
                .or_else(|| self.extralarge.clone())
                .or_else(|| self.mega.clone())
                .or_else(|| self.thumbnail.clone()),
            QobuzImageSize::Medium => self
                .medium
                .clone()
                .or_else(|| self.large.clone())
                .or_else(|| self.extralarge.clone())
                .or_else(|| self.mega.clone())
                .or_else(|| self.small.clone())
                .or_else(|| self.thumbnail.clone()),

            QobuzImageSize::Large => self
                .large
                .clone()
                .or_else(|| self.extralarge.clone())
                .or_else(|| self.mega.clone())
                .or_else(|| self.medium.clone())
                .or_else(|| self.small.clone())
                .or_else(|| self.thumbnail.clone()),

            QobuzImageSize::ExtraLarge => self
                .extralarge
                .clone()
                .or_else(|| self.mega.clone())
                .or_else(|| self.large.clone())
                .or_else(|| self.medium.clone())
                .or_else(|| self.small.clone())
                .or_else(|| self.thumbnail.clone()),

            QobuzImageSize::Mega => self
                .mega
                .clone()
                .or_else(|| self.extralarge.clone())
                .or_else(|| self.large.clone())
                .or_else(|| self.medium.clone())
                .or_else(|| self.small.clone())
                .or_else(|| self.thumbnail.clone()),
        }
    }
}

impl ToValueType<QobuzImage> for &Value {
    fn to_value_type(self) -> Result<QobuzImage, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzImage, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzImage, ParseError> {
        Ok(QobuzImage {
            thumbnail: self.to_value("thumbnail")?,
            small: self.to_value("small")?,
            medium: self.to_value("medium")?,
            large: self.to_value("large")?,
            extralarge: self.to_value("extralarge")?,
            mega: self.to_value("mega")?,
        })
    }
}

/// Represents a music genre in the Qobuz catalog.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzGenre {
    /// Unique genre identifier.
    pub id: u64,
    /// Human-readable genre name.
    pub name: String,
    /// URL-safe genre slug for routing.
    pub slug: String,
}

impl ToValueType<QobuzGenre> for &Value {
    fn to_value_type(self) -> Result<QobuzGenre, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzGenre, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzGenre, ParseError> {
        Ok(QobuzGenre {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
            slug: self.to_value("slug")?,
        })
    }
}

/// Represents an album in the Qobuz music catalog with full metadata.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzAlbum {
    /// Album identifier as string.
    pub id: String,
    /// Primary artist name.
    pub artist: String,
    /// Artist identifier.
    pub artist_id: u64,
    /// Release type (album, live, compilation, etc.).
    pub album_type: QobuzAlbumReleaseType,
    /// Maximum audio bit depth available (16 or 24).
    pub maximum_bit_depth: u16,
    /// Album artwork URLs at various sizes.
    pub image: Option<QobuzImage>,
    /// Album title.
    pub title: String,
    /// Album version or edition (e.g., "Deluxe Edition").
    pub version: Option<String>,
    /// Internal Qobuz numeric identifier.
    pub qobuz_id: u64,
    /// Unix timestamp of release date (milliseconds).
    pub released_at: i64,
    /// Original release date as ISO 8601 string.
    pub release_date_original: String,
    /// Total duration in seconds.
    pub duration: u32,
    /// Whether the album has explicit content.
    pub parental_warning: bool,
    /// Popularity score.
    pub popularity: u32,
    /// Number of tracks on the album.
    pub tracks_count: u32,
    /// Music genre metadata.
    pub genre: QobuzGenre,
    /// Maximum number of audio channels (typically 2 for stereo).
    pub maximum_channel_count: u16,
    /// Maximum sampling rate in kHz (e.g., 44.1, 96, 192).
    pub maximum_sampling_rate: f32,
}

impl TryFrom<QobuzAlbum> for Album {
    type Error = chrono::ParseError;

    fn try_from(value: QobuzAlbum) -> Result<Self, Self::Error> {
        let artwork = value.cover_url();
        Ok(Self {
            id: value.id.as_str().into(),
            title: format_title(value.title.as_str(), value.version.as_deref()),
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
            date_released: Some(parse_date_time(&value.release_date_original)?),
            date_added: None,
            artwork,
            directory: None,
            blur: false,
            versions: vec![],
            album_source: API_SOURCE.clone().into(),
            api_source: API_SOURCE.clone(),
            artist_sources: ApiSources::default()
                .with_source(API_SOURCE.clone(), value.artist_id.into()),
            album_sources: ApiSources::default().with_source(API_SOURCE.clone(), value.id.into()),
        })
    }
}

impl TryFrom<QobuzAlbum> for ApiAlbum {
    type Error = <QobuzAlbum as TryInto<Album>>::Error;

    fn try_from(value: QobuzAlbum) -> Result<Self, Self::Error> {
        let album: Album = value.try_into()?;
        Ok(album.into())
    }
}

impl From<QobuzAlbum> for ApiGlobalSearchResult {
    fn from(value: QobuzAlbum) -> Self {
        Self::Album(ApiGlobalAlbumSearchResult {
            artist_id: value.artist_id.into(),
            artist: value.artist,
            album_id: value.id.into(),
            title: format_title(&value.title, value.version.as_deref()),
            contains_cover: value.image.is_some(),
            blur: false,
            date_released: Some(value.release_date_original),
            date_added: None,
            versions: vec![],
            api_source: API_SOURCE.clone(),
        })
    }
}

impl TryFrom<Album> for QobuzAlbum {
    type Error = TryFromIdError;

    fn try_from(value: Album) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id.clone().try_into()?,
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.try_into()?,
            album_type: value.album_type.into(),
            maximum_bit_depth: 0,
            image: value.artwork.map(|x| QobuzImage {
                thumbnail: None,
                small: None,
                medium: None,
                large: None,
                extralarge: None,
                mega: Some(x),
            }),
            version: None,
            qobuz_id: 0,
            released_at: 0,
            release_date_original: value
                .date_released
                .map(|x| x.and_utc().to_rfc3339())
                .unwrap_or_default(),
            duration: 0,
            parental_warning: false,
            popularity: 0,
            tracks_count: 0,
            genre: QobuzGenre {
                id: 0,
                name: String::new(),
                slug: String::new(),
            },
            maximum_channel_count: 0,
            maximum_sampling_rate: 0.0,
        })
    }
}

impl QobuzAlbum {
    /// Returns the highest quality available album artwork URL.
    ///
    /// Returns `None` if no artwork is available for this album.
    #[must_use]
    pub fn cover_url(&self) -> Option<String> {
        self.image.as_ref().and_then(QobuzImage::cover_url)
    }
}

impl ToValueType<QobuzAlbum> for &Value {
    fn to_value_type(self) -> Result<QobuzAlbum, ParseError> {
        self.as_model()
    }
}

/// Determines the album release type based on track count and duration heuristics.
///
/// Used as a fallback when the Qobuz API does not provide explicit release type information.
#[must_use]
pub const fn magic_qobuz_album_release_type_determinizer(
    duration: u32,
    tracks_count: u32,
) -> QobuzAlbumReleaseType {
    match tracks_count {
        1 => QobuzAlbumReleaseType::Single,
        2..=6 => {
            if duration > 60 * 4 * 5 {
                QobuzAlbumReleaseType::Album
            } else {
                QobuzAlbumReleaseType::EpSingle
            }
        }
        _ => QobuzAlbumReleaseType::Album,
    }
}

impl AsModelResult<QobuzAlbum, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzAlbum, ParseError> {
        let album_type: Option<QobuzAlbumReleaseType> = self.to_value("release_type")?;
        let duration = self.to_value("duration")?;
        let tracks_count = self.to_value("tracks_count")?;
        Ok(QobuzAlbum {
            id: self.to_value("id")?,
            artist: self
                .to_nested_value::<String>(&["artist", "name"])
                .or_else(|_| self.to_nested_value(&["artist", "name", "display"]))?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            maximum_bit_depth: self
                .to_value("maximum_bit_depth")
                .or_else(|_| self.to_nested_value(&["audio_info", "maximum_bit_depth"]))?,
            image: self.to_value("image")?,
            title: self.to_value("title")?,
            version: self.to_value("version")?,
            qobuz_id: self.to_value("qobuz_id")?,
            album_type: album_type.unwrap_or_else(|| {
                magic_qobuz_album_release_type_determinizer(duration, tracks_count)
            }),
            released_at: self.to_value("released_at")?,
            release_date_original: self.to_value("release_date_original")?,
            duration,
            parental_warning: self.to_value("parental_warning")?,
            popularity: self.to_value("popularity")?,
            tracks_count,
            genre: self.to_value("genre")?,
            maximum_channel_count: self
                .to_value("maximum_channel_count")
                .or_else(|_| self.to_nested_value(&["audio_info", "maximum_channel_count"]))?,
            maximum_sampling_rate: self
                .to_value("maximum_sampling_rate")
                .or_else(|_| self.to_nested_value(&["audio_info", "maximum_sampling_rate"]))?,
        })
    }
}

/// Represents an album release in the Qobuz catalog, typically from artist album listings.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzRelease {
    /// Album identifier as string.
    pub id: String,
    /// Primary artist name.
    pub artist: String,
    /// Artist identifier.
    pub artist_id: u64,
    /// Release type (album, live, compilation, etc.).
    pub album_type: QobuzAlbumReleaseType,
    /// Maximum audio bit depth available (16 or 24).
    pub maximum_bit_depth: u16,
    /// Album artwork URLs at various sizes.
    pub image: Option<QobuzImage>,
    /// Album title.
    pub title: String,
    /// Album version or edition (e.g., "Deluxe Edition").
    pub version: Option<String>,
    /// Original release date as ISO 8601 string.
    pub release_date_original: String,
    /// Total duration in seconds.
    pub duration: u32,
    /// Whether the album has explicit content.
    pub parental_warning: bool,
    /// Number of tracks on the album.
    pub tracks_count: u32,
    /// Music genre name.
    pub genre: String,
    /// Maximum number of audio channels (typically 2 for stereo).
    pub maximum_channel_count: u16,
    /// Maximum sampling rate in kHz (e.g., 44.1, 96, 192).
    pub maximum_sampling_rate: f32,
}

impl QobuzRelease {
    /// Returns the highest quality available release artwork URL.
    ///
    /// Returns `None` if no artwork is available for this release.
    #[must_use]
    pub fn cover_url(&self) -> Option<String> {
        self.image.as_ref().and_then(QobuzImage::cover_url)
    }
}

impl ToValueType<QobuzRelease> for &Value {
    fn to_value_type(self) -> Result<QobuzRelease, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzRelease, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzRelease, ParseError> {
        let album_type: Option<QobuzAlbumReleaseType> = self.to_value("release_type")?;
        let duration = self.to_value("duration")?;
        let tracks_count = self.to_value("tracks_count")?;
        Ok(QobuzRelease {
            id: self.to_value("id")?,
            artist: self.to_nested_value(&["artist", "name", "display"])?,
            artist_id: self.to_nested_value(&["artist", "id"])?,
            album_type: album_type.unwrap_or_else(|| {
                magic_qobuz_album_release_type_determinizer(duration, tracks_count)
            }),
            maximum_bit_depth: self.to_nested_value(&["audio_info", "maximum_bit_depth"])?,
            image: self.to_value("image")?,
            title: self.to_value("title")?,
            version: self.to_value("version")?,
            release_date_original: self.to_nested_value(&["dates", "original"])?,
            duration,
            parental_warning: self.to_value("parental_warning")?,
            tracks_count,
            genre: self.to_nested_value(&["genre", "name"])?,
            maximum_channel_count: self
                .to_nested_value(&["audio_info", "maximum_channel_count"])?,
            maximum_sampling_rate: self
                .to_nested_value(&["audio_info", "maximum_sampling_rate"])?,
        })
    }
}

/// Represents a track in the Qobuz music catalog with metadata.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub struct QobuzTrack {
    /// Track identifier.
    pub id: u64,
    /// Track number on the album.
    pub track_number: u32,
    /// Artist name.
    pub artist: String,
    /// Artist identifier.
    pub artist_id: u64,
    /// Album title.
    pub album: String,
    /// Album identifier.
    pub album_id: String,
    /// Album release type.
    pub album_type: QobuzAlbumReleaseType,
    /// Album/track artwork URLs at various sizes.
    pub image: Option<QobuzImage>,
    /// Copyright notice.
    pub copyright: Option<String>,
    /// Track duration in seconds.
    pub duration: u32,
    /// Whether the track has explicit content.
    pub parental_warning: bool,
    /// International Standard Recording Code.
    pub isrc: String,
    /// Track title.
    pub title: String,
    /// Track version (e.g., "Radio Edit", "Remix").
    pub version: Option<String>,
}

impl From<QobuzTrack> for Track {
    fn from(value: QobuzTrack) -> Self {
        let artwork = value.cover_url();
        Self {
            id: value.id.into(),
            number: value.track_number,
            title: format_title(value.title.as_str(), value.version.as_deref()),
            duration: f64::from(value.duration),
            album: value.album,
            album_id: value.album_id.into(),
            album_type: value.album_type.into(),
            date_released: None,
            date_added: None,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            file: None,
            artwork,
            blur: false,
            bytes: 0,
            format: None,
            bit_depth: None,
            audio_bitrate: None,
            overall_bitrate: None,
            sample_rate: None,
            channels: None,
            track_source: API_SOURCE.clone().into(),
            api_source: API_SOURCE.clone(),
            sources: ApiSources::default().with_source(API_SOURCE.clone(), value.id.into()),
        }
    }
}

impl From<QobuzTrack> for ApiGlobalSearchResult {
    fn from(value: QobuzTrack) -> Self {
        Self::Track(ApiGlobalTrackSearchResult {
            artist_id: value.artist_id.into(),
            artist: value.artist,
            album_id: value.album_id.into(),
            album: value.album,
            title: value.title,
            contains_cover: value.image.is_some(),
            blur: false,
            date_released: None,
            date_added: None,
            track_id: value.id.into(),
            format: None,
            bit_depth: None,
            sample_rate: None,
            channels: None,
            source: API_SOURCE.clone().into(),
            api_source: API_SOURCE.clone(),
        })
    }
}

impl QobuzTrack {
    /// Returns the highest quality available track artwork URL.
    ///
    /// Returns `None` if no artwork is available for this track.
    #[must_use]
    pub fn cover_url(&self) -> Option<String> {
        self.image.as_ref().and_then(QobuzImage::cover_url)
    }
}

impl ToValueType<QobuzTrack> for &Value {
    fn to_value_type(self) -> Result<QobuzTrack, ParseError> {
        self.as_model()
    }
}

impl QobuzTrack {
    /// Constructs a `QobuzTrack` from a JSON value with additional album context.
    ///
    /// This method is used when parsing track data that lacks complete album information
    /// in the track object itself, requiring explicit album metadata to be provided.
    ///
    /// # Errors
    ///
    /// * If failed to parse the properties into a `QobuzTrack`
    #[allow(clippy::too_many_arguments)]
    pub fn from_value(
        value: &Value,
        artist: &str,
        artist_id: u64,
        album: &str,
        album_id: &str,
        album_type: QobuzAlbumReleaseType,
        album_version: Option<&str>,
        image: Option<QobuzImage>,
    ) -> Result<Self, ParseError> {
        Ok(Self {
            id: value.to_value("id")?,
            track_number: value.to_value("track_number")?,
            artist: artist.to_string(),
            artist_id,
            album_type,
            album: format_title(album, album_version),
            album_id: album_id.to_string(),
            image,
            copyright: value.to_value("copyright")?,
            duration: value.to_value("duration")?,
            parental_warning: value.to_value("parental_warning")?,
            isrc: value.to_value("isrc")?,
            title: value.to_value("title")?,
            version: value.to_value("version")?,
        })
    }
}

impl AsModelResult<QobuzTrack, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzTrack, ParseError> {
        let album_type: Option<QobuzAlbumReleaseType> =
            self.to_nested_value(&["album", "release_type"])?;
        let duration = self.to_nested_value(&["album", "duration"])?;
        let tracks_count = self.to_nested_value(&["album", "tracks_count"])?;
        Ok(QobuzTrack {
            id: self.to_value("id")?,
            track_number: self.to_value("track_number")?,
            album: self.to_nested_value(&["album", "title"])?,
            album_id: self.to_nested_value(&["album", "id"])?,
            artist: self.to_nested_value(&["album", "artist", "name"])?,
            artist_id: self.to_nested_value(&["album", "artist", "id"])?,
            album_type: album_type.unwrap_or_else(|| {
                magic_qobuz_album_release_type_determinizer(duration, tracks_count)
            }),
            image: self.to_value("image")?,
            copyright: self.to_value("copyright")?,
            duration,
            parental_warning: self.to_value("parental_warning")?,
            isrc: self.to_value("isrc")?,
            title: self.to_value("title")?,
            version: self.to_value("version")?,
        })
    }
}

/// Represents an artist in the Qobuz music catalog.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct QobuzArtist {
    /// Artist identifier.
    pub id: u64,
    /// Artist photo URLs at various sizes.
    pub image: Option<QobuzImage>,
    /// Artist name.
    pub name: String,
}

impl From<QobuzArtist> for Artist {
    fn from(value: QobuzArtist) -> Self {
        let cover = value.cover_url();
        Self {
            id: value.id.into(),
            title: value.name,
            cover,
            api_source: API_SOURCE.clone(),
            api_sources: ApiSources::default().with_source(API_SOURCE.clone(), value.id.into()),
        }
    }
}

impl From<QobuzArtist> for ApiArtist {
    fn from(value: QobuzArtist) -> Self {
        Self {
            artist_id: value.id.into(),
            title: value.name,
            contains_cover: value.image.is_some(),
            api_source: API_SOURCE.clone(),
            api_sources: ApiSources::default().with_source(API_SOURCE.clone(), value.id.into()),
        }
    }
}

impl From<QobuzArtist> for ApiGlobalSearchResult {
    fn from(value: QobuzArtist) -> Self {
        Self::Artist(ApiGlobalArtistSearchResult {
            artist_id: value.id.into(),
            title: value.name,
            contains_cover: value.image.is_some(),
            blur: false,
            api_source: API_SOURCE.clone(),
        })
    }
}

impl QobuzArtist {
    /// Returns the highest quality available artist photo URL.
    ///
    /// Returns `None` if no photo is available for this artist.
    #[must_use]
    pub fn cover_url(&self) -> Option<String> {
        self.image.as_ref().and_then(QobuzImage::cover_url)
    }
}

impl ToValueType<QobuzArtist> for &Value {
    fn to_value_type(self) -> Result<QobuzArtist, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzArtist, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzArtist, ParseError> {
        Ok(QobuzArtist {
            id: self.to_value("id")?,
            image: self.to_value("image")?,
            name: self.to_value("name")?,
        })
    }
}

impl From<QobuzRelease> for QobuzAlbum {
    fn from(value: QobuzRelease) -> Self {
        Self {
            id: value.id,
            artist: value.artist,
            artist_id: value.artist_id,
            maximum_bit_depth: value.maximum_bit_depth,
            image: value.image,
            title: value.title,
            version: value.version,
            released_at: chrono::DateTime::from_str(&value.release_date_original)
                .map(|x: DateTime<Utc>| x.timestamp_millis())
                .unwrap_or(0),
            release_date_original: value.release_date_original,
            duration: value.duration,
            parental_warning: value.parental_warning,
            tracks_count: value.tracks_count,
            maximum_channel_count: value.maximum_bit_depth,
            maximum_sampling_rate: value.maximum_sampling_rate,
            album_type: QobuzAlbumReleaseType::Album,
            qobuz_id: 0,
            popularity: 0,
            genre: QobuzGenre::default(),
        }
    }
}

impl TryFrom<QobuzRelease> for Album {
    type Error = <QobuzAlbum as TryInto<Self>>::Error;

    fn try_from(value: QobuzRelease) -> Result<Self, Self::Error> {
        let album: QobuzAlbum = value.into();
        album.try_into()
    }
}

impl TryFrom<QobuzRelease> for ApiAlbum {
    type Error = <QobuzRelease as TryInto<Album>>::Error;

    fn try_from(value: QobuzRelease) -> Result<Self, Self::Error> {
        let album: Album = value.try_into()?;
        Ok(album.into())
    }
}

/// Represents a paginated list of search results from Qobuz.
#[derive(Serialize, Deserialize)]
pub struct QobuzSearchResultList<T> {
    /// List of result items.
    pub items: Vec<T>,
    /// Starting offset of this page.
    pub offset: usize,
    /// Maximum number of items per page.
    pub limit: usize,
    /// Total number of matching results.
    pub total: usize,
}

impl<T> ToValueType<QobuzSearchResultList<T>> for &Value
where
    Value: AsModelResult<QobuzSearchResultList<T>, ParseError>,
{
    fn to_value_type(self) -> Result<QobuzSearchResultList<T>, ParseError> {
        self.as_model()
    }
}

impl<T> AsModelResult<QobuzSearchResultList<T>, ParseError> for Value
where
    for<'a> &'a Self: ToValueType<T>,
    for<'a> &'a Self: ToValueType<usize>,
{
    fn as_model(&self) -> Result<QobuzSearchResultList<T>, ParseError> {
        Ok(QobuzSearchResultList {
            items: self.to_value("items")?,
            offset: self.to_value("offset")?,
            limit: self.to_value("limit")?,
            total: self.to_value("total")?,
        })
    }
}

/// Contains search results across albums, artists, and tracks from Qobuz.
#[derive(Serialize, Deserialize)]
pub struct QobuzSearchResults {
    /// Paginated list of matching albums.
    pub albums: QobuzSearchResultList<QobuzAlbum>,
    /// Paginated list of matching artists.
    pub artists: QobuzSearchResultList<QobuzArtist>,
    /// Paginated list of matching tracks.
    pub tracks: QobuzSearchResultList<QobuzTrack>,
}

#[allow(clippy::fallible_impl_from)]
impl From<QobuzSearchResults> for ApiSearchResultsResponse {
    fn from(value: QobuzSearchResults) -> Self {
        let artists = value
            .artists
            .items
            .into_iter()
            .map(Into::into)
            .collect::<Vec<ApiGlobalSearchResult>>();
        let albums = value
            .albums
            .items
            .into_iter()
            .map(Into::into)
            .collect::<Vec<ApiGlobalSearchResult>>();
        let tracks = value
            .tracks
            .items
            .into_iter()
            .map(Into::into)
            .collect::<Vec<ApiGlobalSearchResult>>();

        let position = value.albums.offset + value.albums.limit;
        let position = if position > value.albums.total {
            value.albums.total
        } else {
            position
        };

        Self {
            position: u32::try_from(position).unwrap(),
            results: [artists, albums, tracks].concat(),
        }
    }
}

impl ToValueType<QobuzSearchResults> for &Value {
    fn to_value_type(self) -> Result<QobuzSearchResults, ParseError> {
        self.as_model()
    }
}

impl AsModelResult<QobuzSearchResults, ParseError> for Value {
    fn as_model(&self) -> Result<QobuzSearchResults, ParseError> {
        Ok(QobuzSearchResults {
            albums: self.to_value("albums")?,
            artists: self.to_value("artists")?,
            tracks: self.to_value("tracks")?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_magic_qobuz_album_release_type_determinizer_single_track() {
        let result = magic_qobuz_album_release_type_determinizer(180, 1);
        assert_eq!(result, QobuzAlbumReleaseType::Single);
    }

    #[test_log::test]
    fn test_magic_qobuz_album_release_type_determinizer_short_ep() {
        let result = magic_qobuz_album_release_type_determinizer(300, 3);
        assert_eq!(result, QobuzAlbumReleaseType::EpSingle);
    }

    #[test_log::test]
    fn test_magic_qobuz_album_release_type_determinizer_long_ep_becomes_album() {
        // 21 minutes (longer than 20 minutes threshold)
        let result = magic_qobuz_album_release_type_determinizer(1260, 6);
        assert_eq!(result, QobuzAlbumReleaseType::Album);
    }

    #[test_log::test]
    fn test_magic_qobuz_album_release_type_determinizer_many_tracks() {
        let result = magic_qobuz_album_release_type_determinizer(2400, 12);
        assert_eq!(result, QobuzAlbumReleaseType::Album);
    }

    #[test_log::test]
    fn test_magic_qobuz_album_release_type_determinizer_boundary_cases() {
        // Exactly at EP/Single boundary with short duration
        let result = magic_qobuz_album_release_type_determinizer(100, 2);
        assert_eq!(result, QobuzAlbumReleaseType::EpSingle);

        // Exactly at EP/Album boundary with 7 tracks
        let result = magic_qobuz_album_release_type_determinizer(1000, 7);
        assert_eq!(result, QobuzAlbumReleaseType::Album);
    }

    #[test_log::test]
    fn test_qobuz_image_size_from_u16() {
        assert_eq!(QobuzImageSize::from(50), QobuzImageSize::Thumbnail);
        assert_eq!(QobuzImageSize::from(100), QobuzImageSize::Thumbnail);
        assert_eq!(QobuzImageSize::from(150), QobuzImageSize::Small);
        assert_eq!(QobuzImageSize::from(300), QobuzImageSize::Small);
        assert_eq!(QobuzImageSize::from(400), QobuzImageSize::Medium);
        assert_eq!(QobuzImageSize::from(600), QobuzImageSize::Medium);
        assert_eq!(QobuzImageSize::from(800), QobuzImageSize::Large);
        assert_eq!(QobuzImageSize::from(1200), QobuzImageSize::Large);
        assert_eq!(QobuzImageSize::from(1500), QobuzImageSize::ExtraLarge);
        assert_eq!(QobuzImageSize::from(2400), QobuzImageSize::ExtraLarge);
        assert_eq!(QobuzImageSize::from(3000), QobuzImageSize::Mega);
        assert_eq!(QobuzImageSize::from(5000), QobuzImageSize::Mega);
    }

    #[test_log::test]
    fn test_qobuz_image_size_to_u16() {
        assert_eq!(u16::from(QobuzImageSize::Thumbnail), 100);
        assert_eq!(u16::from(QobuzImageSize::Small), 300);
        assert_eq!(u16::from(QobuzImageSize::Medium), 600);
        assert_eq!(u16::from(QobuzImageSize::Large), 1200);
        assert_eq!(u16::from(QobuzImageSize::ExtraLarge), 2400);
        assert_eq!(u16::from(QobuzImageSize::Mega), 4800);
    }

    #[test_log::test]
    fn test_qobuz_image_size_from_image_cover_size() {
        assert_eq!(
            QobuzImageSize::from(ImageCoverSize::Thumbnail),
            QobuzImageSize::Thumbnail
        );
        assert_eq!(
            QobuzImageSize::from(ImageCoverSize::Small),
            QobuzImageSize::Small
        );
        assert_eq!(
            QobuzImageSize::from(ImageCoverSize::Medium),
            QobuzImageSize::Medium
        );
        assert_eq!(
            QobuzImageSize::from(ImageCoverSize::Large),
            QobuzImageSize::Large
        );
        assert_eq!(
            QobuzImageSize::from(ImageCoverSize::Max),
            QobuzImageSize::Mega
        );
    }

    #[test_log::test]
    fn test_qobuz_image_cover_url_all_available() {
        let image = QobuzImage {
            thumbnail: Some("thumb.jpg".to_string()),
            small: Some("small.jpg".to_string()),
            medium: Some("medium.jpg".to_string()),
            large: Some("large.jpg".to_string()),
            extralarge: Some("xl.jpg".to_string()),
            mega: Some("mega.jpg".to_string()),
        };

        assert_eq!(image.cover_url(), Some("mega.jpg".to_string()));
    }

    #[test_log::test]
    fn test_qobuz_image_cover_url_for_size_with_fallback() {
        let image = QobuzImage {
            thumbnail: None,
            small: Some("small.jpg".to_string()),
            medium: None,
            large: Some("large.jpg".to_string()),
            extralarge: None,
            mega: None,
        };

        // Request mega, should fall back to large
        assert_eq!(
            image.cover_url_for_size(QobuzImageSize::Mega),
            Some("large.jpg".to_string())
        );

        // Request medium, should fall back to large (higher quality preferred)
        assert_eq!(
            image.cover_url_for_size(QobuzImageSize::Medium),
            Some("large.jpg".to_string())
        );

        // Request thumbnail, should fall back to small
        assert_eq!(
            image.cover_url_for_size(QobuzImageSize::Thumbnail),
            Some("small.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_qobuz_image_cover_url_for_size_prefers_higher_quality() {
        let image = QobuzImage {
            thumbnail: Some("thumb.jpg".to_string()),
            small: Some("small.jpg".to_string()),
            medium: None,
            large: None,
            extralarge: None,
            mega: None,
        };

        // Request medium when only smaller sizes available
        // Should prefer small over thumbnail
        assert_eq!(
            image.cover_url_for_size(QobuzImageSize::Medium),
            Some("small.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_qobuz_image_cover_url_for_size_no_images() {
        let image = QobuzImage::default();

        assert_eq!(image.cover_url_for_size(QobuzImageSize::Mega), None);
        assert_eq!(image.cover_url_for_size(QobuzImageSize::Thumbnail), None);
    }

    #[test_log::test]
    fn test_qobuz_image_cover_url_for_size_exact_match() {
        let image = QobuzImage {
            thumbnail: None,
            small: None,
            medium: Some("medium.jpg".to_string()),
            large: None,
            extralarge: None,
            mega: None,
        };

        assert_eq!(
            image.cover_url_for_size(QobuzImageSize::Medium),
            Some("medium.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_qobuz_image_cover_url_for_size_thumbnail_fallback_chain() {
        let image = QobuzImage {
            thumbnail: None,
            small: None,
            medium: None,
            large: None,
            extralarge: None,
            mega: Some("mega.jpg".to_string()),
        };

        // Thumbnail should eventually fall back to mega as last resort
        assert_eq!(
            image.cover_url_for_size(QobuzImageSize::Thumbnail),
            Some("mega.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_qobuz_image_display() {
        assert_eq!(format!("{}", QobuzImageSize::Thumbnail), "100");
        assert_eq!(format!("{}", QobuzImageSize::Small), "300");
        assert_eq!(format!("{}", QobuzImageSize::Medium), "600");
        assert_eq!(format!("{}", QobuzImageSize::Large), "1200");
        assert_eq!(format!("{}", QobuzImageSize::ExtraLarge), "2400");
        assert_eq!(format!("{}", QobuzImageSize::Mega), "4800");
    }

    #[test_log::test]
    fn test_search_results_to_api_response_position_calculation() {
        // Test position when offset + limit is less than total
        let results = QobuzSearchResults {
            albums: QobuzSearchResultList {
                items: vec![],
                offset: 0,
                limit: 10,
                total: 100,
            },
            artists: QobuzSearchResultList {
                items: vec![],
                offset: 0,
                limit: 10,
                total: 50,
            },
            tracks: QobuzSearchResultList {
                items: vec![],
                offset: 0,
                limit: 10,
                total: 75,
            },
        };
        let response: ApiSearchResultsResponse = results.into();
        assert_eq!(response.position, 10); // offset (0) + limit (10) = 10 < total (100)
    }

    #[test_log::test]
    fn test_search_results_to_api_response_position_capped_at_total() {
        // Test position is capped at total when offset + limit exceeds total
        let results = QobuzSearchResults {
            albums: QobuzSearchResultList {
                items: vec![],
                offset: 95,
                limit: 10,
                total: 100,
            },
            artists: QobuzSearchResultList {
                items: vec![],
                offset: 0,
                limit: 10,
                total: 50,
            },
            tracks: QobuzSearchResultList {
                items: vec![],
                offset: 0,
                limit: 10,
                total: 75,
            },
        };
        let response: ApiSearchResultsResponse = results.into();
        // offset (95) + limit (10) = 105, but total is 100, so position should be 100
        assert_eq!(response.position, 100);
    }

    #[test_log::test]
    fn test_search_results_to_api_response_position_exactly_at_total() {
        // Test when offset + limit equals total exactly
        let results = QobuzSearchResults {
            albums: QobuzSearchResultList {
                items: vec![],
                offset: 90,
                limit: 10,
                total: 100,
            },
            artists: QobuzSearchResultList {
                items: vec![],
                offset: 0,
                limit: 10,
                total: 50,
            },
            tracks: QobuzSearchResultList {
                items: vec![],
                offset: 0,
                limit: 10,
                total: 75,
            },
        };
        let response: ApiSearchResultsResponse = results.into();
        assert_eq!(response.position, 100); // offset (90) + limit (10) = 100 = total
    }

    #[test_log::test]
    fn test_qobuz_release_to_qobuz_album_conversion() {
        let release = QobuzRelease {
            id: "album123".to_string(),
            artist: "Test Artist".to_string(),
            artist_id: 42,
            album_type: crate::QobuzAlbumReleaseType::Live,
            maximum_bit_depth: 24,
            image: Some(QobuzImage {
                thumbnail: Some("thumb.jpg".to_string()),
                small: None,
                medium: None,
                large: None,
                extralarge: None,
                mega: None,
            }),
            title: "Test Album".to_string(),
            version: Some("Deluxe".to_string()),
            release_date_original: "2023-06-15T00:00:00Z".to_string(),
            duration: 3600,
            parental_warning: true,
            tracks_count: 12,
            genre: "Rock".to_string(),
            maximum_channel_count: 2,
            maximum_sampling_rate: 96.0,
        };

        let album: QobuzAlbum = release.into();

        assert_eq!(album.id, "album123");
        assert_eq!(album.artist, "Test Artist");
        assert_eq!(album.artist_id, 42);
        assert_eq!(album.maximum_bit_depth, 24);
        assert_eq!(album.title, "Test Album");
        assert_eq!(album.version, Some("Deluxe".to_string()));
        assert_eq!(album.duration, 3600);
        assert!(album.parental_warning);
        assert_eq!(album.tracks_count, 12);
        assert!((album.maximum_sampling_rate - 96.0).abs() < f32::EPSILON);
        // released_at should be a valid timestamp from the ISO 8601 date
        assert!(album.released_at > 0);
        // album_type is always set to Album in this conversion
        assert_eq!(album.album_type, crate::QobuzAlbumReleaseType::Album);
        // qobuz_id and popularity are set to 0
        assert_eq!(album.qobuz_id, 0);
        assert_eq!(album.popularity, 0);
    }

    #[test_log::test]
    fn test_qobuz_release_to_qobuz_album_with_invalid_date() {
        // Test that invalid date results in released_at = 0
        let release = QobuzRelease {
            id: "album456".to_string(),
            artist: "Artist".to_string(),
            artist_id: 1,
            album_type: crate::QobuzAlbumReleaseType::Album,
            maximum_bit_depth: 16,
            image: None,
            title: "Album".to_string(),
            version: None,
            release_date_original: "not-a-valid-date".to_string(),
            duration: 1000,
            parental_warning: false,
            tracks_count: 10,
            genre: "Pop".to_string(),
            maximum_channel_count: 2,
            maximum_sampling_rate: 44.1,
        };

        let album: QobuzAlbum = release.into();

        // Invalid date should result in released_at = 0
        assert_eq!(album.released_at, 0);
    }

    #[test_log::test]
    fn test_qobuz_album_cover_url_with_image() {
        let album = QobuzAlbum {
            id: "123".to_string(),
            image: Some(QobuzImage {
                thumbnail: None,
                small: None,
                medium: Some("medium.jpg".to_string()),
                large: None,
                extralarge: None,
                mega: None,
            }),
            ..Default::default()
        };

        assert_eq!(album.cover_url(), Some("medium.jpg".to_string()));
    }

    #[test_log::test]
    fn test_qobuz_album_cover_url_without_image() {
        let album = QobuzAlbum {
            id: "123".to_string(),
            image: None,
            ..Default::default()
        };

        assert_eq!(album.cover_url(), None);
    }

    #[test_log::test]
    fn test_qobuz_release_cover_url_with_image() {
        let release = QobuzRelease {
            id: "456".to_string(),
            image: Some(QobuzImage {
                thumbnail: None,
                small: None,
                medium: None,
                large: Some("large.jpg".to_string()),
                extralarge: None,
                mega: None,
            }),
            ..Default::default()
        };

        assert_eq!(release.cover_url(), Some("large.jpg".to_string()));
    }

    #[test_log::test]
    fn test_qobuz_track_cover_url() {
        let track = QobuzTrack {
            id: 789,
            image: Some(QobuzImage {
                thumbnail: Some("track_thumb.jpg".to_string()),
                small: None,
                medium: None,
                large: None,
                extralarge: None,
                mega: None,
            }),
            ..Default::default()
        };

        assert_eq!(track.cover_url(), Some("track_thumb.jpg".to_string()));
    }

    #[test_log::test]
    fn test_qobuz_artist_cover_url() {
        let artist = QobuzArtist {
            id: 111,
            name: "Test Artist".to_string(),
            image: Some(QobuzImage {
                thumbnail: None,
                small: None,
                medium: None,
                large: None,
                extralarge: Some("artist_xl.jpg".to_string()),
                mega: None,
            }),
        };

        assert_eq!(artist.cover_url(), Some("artist_xl.jpg".to_string()));
    }

    #[test_log::test]
    fn test_qobuz_image_cover_url_for_size_extralarge() {
        // Test the ExtraLarge fallback path
        let image = QobuzImage {
            thumbnail: Some("thumb.jpg".to_string()),
            small: None,
            medium: None,
            large: None,
            extralarge: Some("xl.jpg".to_string()),
            mega: None,
        };

        // ExtraLarge should return its own value
        assert_eq!(
            image.cover_url_for_size(QobuzImageSize::ExtraLarge),
            Some("xl.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_qobuz_image_cover_url_for_size_small() {
        // Test the Small fallback path
        let image = QobuzImage {
            thumbnail: Some("thumb.jpg".to_string()),
            small: Some("small.jpg".to_string()),
            medium: None,
            large: None,
            extralarge: None,
            mega: None,
        };

        // Small should return its own value first
        assert_eq!(
            image.cover_url_for_size(QobuzImageSize::Small),
            Some("small.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_qobuz_image_cover_url_for_size_large() {
        // Test the Large fallback path
        let image = QobuzImage {
            thumbnail: Some("thumb.jpg".to_string()),
            small: None,
            medium: None,
            large: Some("large.jpg".to_string()),
            extralarge: None,
            mega: None,
        };

        // Large should return its own value first
        assert_eq!(
            image.cover_url_for_size(QobuzImageSize::Large),
            Some("large.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_qobuz_image_cover_url_for_size_small_fallback_to_thumbnail() {
        // Test Small falling back to thumbnail when no larger sizes available
        let image = QobuzImage {
            thumbnail: Some("thumb.jpg".to_string()),
            small: None,
            medium: None,
            large: None,
            extralarge: None,
            mega: None,
        };

        // Small with only thumbnail available should fall back to thumbnail
        assert_eq!(
            image.cover_url_for_size(QobuzImageSize::Small),
            Some("thumb.jpg".to_string())
        );
    }

    #[test_log::test]
    fn test_qobuz_track_from_value_parses_all_fields() {
        let json = serde_json::json!({
            "id": 12345,
            "track_number": 3,
            "copyright": "2023 Test Records",
            "duration": 240,
            "parental_warning": true,
            "isrc": "USRC12345678",
            "title": "Test Track",
            "version": "Live Version"
        });

        let image = QobuzImage {
            thumbnail: Some("thumb.jpg".to_string()),
            small: None,
            medium: None,
            large: None,
            extralarge: None,
            mega: None,
        };

        let track = QobuzTrack::from_value(
            &json,
            "Test Artist",
            42,
            "Test Album",
            "album123",
            crate::QobuzAlbumReleaseType::Album,
            Some("Deluxe"),
            Some(image.clone()),
        )
        .expect("Failed to parse track");

        assert_eq!(track.id, 12345);
        assert_eq!(track.track_number, 3);
        assert_eq!(track.artist, "Test Artist");
        assert_eq!(track.artist_id, 42);
        // Album title should include the version passed
        assert_eq!(track.album, "Test Album - Deluxe");
        assert_eq!(track.album_id, "album123");
        assert_eq!(track.album_type, crate::QobuzAlbumReleaseType::Album);
        assert_eq!(track.copyright, Some("2023 Test Records".to_string()));
        assert_eq!(track.duration, 240);
        assert!(track.parental_warning);
        assert_eq!(track.isrc, "USRC12345678");
        assert_eq!(track.title, "Test Track");
        assert_eq!(track.version, Some("Live Version".to_string()));
        assert_eq!(track.image, Some(image));
    }

    #[test_log::test]
    fn test_qobuz_track_from_value_without_optional_fields() {
        let json = serde_json::json!({
            "id": 99,
            "track_number": 1,
            "duration": 180,
            "parental_warning": false,
            "isrc": "TEST12345",
            "title": "Simple Track"
        });

        let track = QobuzTrack::from_value(
            &json,
            "Artist",
            1,
            "Album",
            "album1",
            crate::QobuzAlbumReleaseType::Single,
            None,
            None,
        )
        .expect("Failed to parse track");

        assert_eq!(track.id, 99);
        assert_eq!(track.track_number, 1);
        assert_eq!(track.album, "Album");
        assert_eq!(track.copyright, None);
        assert_eq!(track.version, None);
        assert_eq!(track.image, None);
    }

    #[test_log::test]
    fn test_qobuz_artist_to_artist_conversion() {
        let qobuz_artist = QobuzArtist {
            id: 12345,
            name: "Test Artist Name".to_string(),
            image: Some(QobuzImage {
                thumbnail: None,
                small: None,
                medium: None,
                large: Some("large_artist.jpg".to_string()),
                extralarge: None,
                mega: None,
            }),
        };

        let artist: moosicbox_music_models::Artist = qobuz_artist.into();

        assert_eq!(artist.id, 12345.into());
        assert_eq!(artist.title, "Test Artist Name");
        assert_eq!(artist.cover, Some("large_artist.jpg".to_string()));
    }

    #[test_log::test]
    fn test_qobuz_artist_to_artist_without_image() {
        let qobuz_artist = QobuzArtist {
            id: 999,
            name: "No Image Artist".to_string(),
            image: None,
        };

        let artist: moosicbox_music_models::Artist = qobuz_artist.into();

        assert_eq!(artist.id, 999.into());
        assert_eq!(artist.title, "No Image Artist");
        assert_eq!(artist.cover, None);
    }

    #[test_log::test]
    fn test_qobuz_artist_to_api_artist_conversion() {
        let qobuz_artist = QobuzArtist {
            id: 5555,
            name: "API Test Artist".to_string(),
            image: Some(QobuzImage {
                thumbnail: Some("thumb.jpg".to_string()),
                small: None,
                medium: None,
                large: None,
                extralarge: None,
                mega: None,
            }),
        };

        let api_artist: moosicbox_music_models::api::ApiArtist = qobuz_artist.into();

        assert_eq!(api_artist.artist_id, 5555.into());
        assert_eq!(api_artist.title, "API Test Artist");
        assert!(api_artist.contains_cover);
    }

    #[test_log::test]
    fn test_qobuz_artist_to_api_artist_without_image() {
        let qobuz_artist = QobuzArtist {
            id: 6666,
            name: "No Cover Artist".to_string(),
            image: None,
        };

        let api_artist: moosicbox_music_models::api::ApiArtist = qobuz_artist.into();

        assert_eq!(api_artist.artist_id, 6666.into());
        assert!(!api_artist.contains_cover);
    }

    #[test_log::test]
    fn test_qobuz_track_to_track_conversion() {
        let qobuz_track = QobuzTrack {
            id: 11111,
            track_number: 5,
            artist: "Track Artist".to_string(),
            artist_id: 222,
            album: "Track Album".to_string(),
            album_id: "album_track_123".to_string(),
            album_type: crate::QobuzAlbumReleaseType::Live,
            image: Some(QobuzImage {
                thumbnail: None,
                small: Some("small_cover.jpg".to_string()),
                medium: None,
                large: None,
                extralarge: None,
                mega: None,
            }),
            copyright: Some("Copyright Info".to_string()),
            duration: 300,
            parental_warning: true,
            isrc: "ISRC12345".to_string(),
            title: "Converted Track".to_string(),
            version: Some("Extended Mix".to_string()),
        };

        let track: moosicbox_music_models::Track = qobuz_track.into();

        assert_eq!(track.id, 11111.into());
        assert_eq!(track.number, 5);
        // Title should include version
        assert_eq!(track.title, "Converted Track - Extended Mix");
        assert!((track.duration - 300.0).abs() < f64::EPSILON);
        assert_eq!(track.album, "Track Album");
        assert_eq!(track.album_id, "album_track_123".into());
        assert_eq!(track.album_type, moosicbox_music_models::AlbumType::Live);
        assert_eq!(track.artist, "Track Artist");
        assert_eq!(track.artist_id, 222.into());
        assert_eq!(track.artwork, Some("small_cover.jpg".to_string()));
        assert!(!track.blur);
    }

    #[test_log::test]
    fn test_qobuz_track_to_track_without_optional_fields() {
        let qobuz_track = QobuzTrack {
            id: 22222,
            track_number: 1,
            artist: "Simple Artist".to_string(),
            artist_id: 100,
            album: "Simple Album".to_string(),
            album_id: "simple_album".to_string(),
            album_type: crate::QobuzAlbumReleaseType::Single,
            image: None,
            copyright: None,
            duration: 180,
            parental_warning: false,
            isrc: "SIMPLE123".to_string(),
            title: "Simple Track".to_string(),
            version: None,
        };

        let track: moosicbox_music_models::Track = qobuz_track.into();

        // Title without version should be just the title
        assert_eq!(track.title, "Simple Track");
        assert_eq!(track.artwork, None);
    }

    #[test_log::test]
    fn test_qobuz_album_to_album_conversion_success() {
        let qobuz_album = QobuzAlbum {
            id: "album_999".to_string(),
            artist: "Album Artist".to_string(),
            artist_id: 555,
            album_type: crate::QobuzAlbumReleaseType::Compilation,
            maximum_bit_depth: 24,
            image: Some(QobuzImage {
                thumbnail: None,
                small: None,
                medium: Some("medium_album.jpg".to_string()),
                large: None,
                extralarge: None,
                mega: None,
            }),
            title: "Converted Album".to_string(),
            version: Some("Remastered".to_string()),
            qobuz_id: 99999,
            released_at: 1_686_787_200_000,
            release_date_original: "2023-06-15".to_string(),
            duration: 3600,
            parental_warning: false,
            popularity: 100,
            tracks_count: 12,
            genre: QobuzGenre {
                id: 1,
                name: "Rock".to_string(),
                slug: "rock".to_string(),
            },
            maximum_channel_count: 2,
            maximum_sampling_rate: 96.0,
        };

        let album: moosicbox_music_models::Album = qobuz_album
            .try_into()
            .expect("Failed to convert QobuzAlbum to Album");

        assert_eq!(album.id, "album_999".into());
        // Title should include version
        assert_eq!(album.title, "Converted Album - Remastered");
        assert_eq!(album.artist, "Album Artist");
        assert_eq!(album.artist_id, 555.into());
        assert_eq!(
            album.album_type,
            moosicbox_music_models::AlbumType::Compilations
        );
        assert_eq!(album.artwork, Some("medium_album.jpg".to_string()));
        assert!(!album.blur);
    }

    #[test_log::test]
    fn test_qobuz_album_to_album_with_invalid_date() {
        let qobuz_album = QobuzAlbum {
            id: "album_bad_date".to_string(),
            artist: "Artist".to_string(),
            artist_id: 1,
            album_type: crate::QobuzAlbumReleaseType::Album,
            maximum_bit_depth: 16,
            image: None,
            title: "Album".to_string(),
            version: None,
            qobuz_id: 1,
            released_at: 0,
            release_date_original: "not-a-valid-date".to_string(),
            duration: 100,
            parental_warning: false,
            popularity: 0,
            tracks_count: 1,
            genre: QobuzGenre::default(),
            maximum_channel_count: 2,
            maximum_sampling_rate: 44.1,
        };

        // Converting with invalid date should fail
        let result: Result<moosicbox_music_models::Album, _> = qobuz_album.try_into();
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_qobuz_artist_to_api_global_search_result() {
        let qobuz_artist = QobuzArtist {
            id: 7777,
            name: "Search Artist".to_string(),
            image: Some(QobuzImage {
                thumbnail: Some("search_thumb.jpg".to_string()),
                small: None,
                medium: None,
                large: None,
                extralarge: None,
                mega: None,
            }),
        };

        let search_result: ApiGlobalSearchResult = qobuz_artist.into();

        match search_result {
            ApiGlobalSearchResult::Artist(result) => {
                assert_eq!(result.artist_id, 7777.into());
                assert_eq!(result.title, "Search Artist");
                assert!(result.contains_cover);
                assert!(!result.blur);
            }
            _ => panic!("Expected Artist search result"),
        }
    }

    #[test_log::test]
    fn test_qobuz_album_to_api_global_search_result() {
        let qobuz_album = QobuzAlbum {
            id: "search_album_123".to_string(),
            artist: "Search Album Artist".to_string(),
            artist_id: 888,
            album_type: crate::QobuzAlbumReleaseType::Album,
            maximum_bit_depth: 16,
            image: Some(QobuzImage {
                thumbnail: None,
                small: None,
                medium: Some("album_medium.jpg".to_string()),
                large: None,
                extralarge: None,
                mega: None,
            }),
            title: "Search Album Title".to_string(),
            version: Some("Deluxe".to_string()),
            qobuz_id: 12345,
            released_at: 0,
            release_date_original: "2024-01-01".to_string(),
            duration: 3000,
            parental_warning: false,
            popularity: 50,
            tracks_count: 10,
            genre: QobuzGenre::default(),
            maximum_channel_count: 2,
            maximum_sampling_rate: 48.0,
        };

        let search_result: ApiGlobalSearchResult = qobuz_album.into();

        match search_result {
            ApiGlobalSearchResult::Album(result) => {
                assert_eq!(result.artist_id, 888.into());
                assert_eq!(result.artist, "Search Album Artist");
                assert_eq!(result.album_id, "search_album_123".into());
                // Title should include version
                assert_eq!(result.title, "Search Album Title - Deluxe");
                assert!(result.contains_cover);
                assert!(!result.blur);
                assert_eq!(result.date_released, Some("2024-01-01".to_string()));
            }
            _ => panic!("Expected Album search result"),
        }
    }

    #[test_log::test]
    fn test_qobuz_track_to_api_global_search_result() {
        let qobuz_track = QobuzTrack {
            id: 33333,
            track_number: 7,
            artist: "Search Track Artist".to_string(),
            artist_id: 444,
            album: "Search Track Album".to_string(),
            album_id: "search_track_album_id".to_string(),
            album_type: crate::QobuzAlbumReleaseType::EpSingle,
            image: Some(QobuzImage {
                thumbnail: Some("track_search_thumb.jpg".to_string()),
                small: None,
                medium: None,
                large: None,
                extralarge: None,
                mega: None,
            }),
            copyright: None,
            duration: 210,
            parental_warning: false,
            isrc: "SEARCH123".to_string(),
            title: "Search Track Title".to_string(),
            version: None,
        };

        let search_result: ApiGlobalSearchResult = qobuz_track.into();

        match search_result {
            ApiGlobalSearchResult::Track(result) => {
                assert_eq!(result.track_id, 33333.into());
                assert_eq!(result.artist_id, 444.into());
                assert_eq!(result.artist, "Search Track Artist");
                assert_eq!(result.album_id, "search_track_album_id".into());
                assert_eq!(result.album, "Search Track Album");
                assert_eq!(result.title, "Search Track Title");
                assert!(result.contains_cover);
                assert!(!result.blur);
            }
            _ => panic!("Expected Track search result"),
        }
    }

    #[test_log::test]
    fn test_qobuz_search_results_concatenates_all_result_types() {
        // Create search results with items in each category
        let results = QobuzSearchResults {
            albums: QobuzSearchResultList {
                items: vec![QobuzAlbum {
                    id: "album1".to_string(),
                    artist: "Artist1".to_string(),
                    artist_id: 1,
                    album_type: crate::QobuzAlbumReleaseType::Album,
                    maximum_bit_depth: 16,
                    image: None,
                    title: "Album1".to_string(),
                    version: None,
                    qobuz_id: 1,
                    released_at: 0,
                    release_date_original: "2023-01-01".to_string(),
                    duration: 1000,
                    parental_warning: false,
                    popularity: 10,
                    tracks_count: 10,
                    genre: QobuzGenre::default(),
                    maximum_channel_count: 2,
                    maximum_sampling_rate: 44.1,
                }],
                offset: 0,
                limit: 10,
                total: 1,
            },
            artists: QobuzSearchResultList {
                items: vec![QobuzArtist {
                    id: 100,
                    name: "Artist100".to_string(),
                    image: None,
                }],
                offset: 0,
                limit: 10,
                total: 1,
            },
            tracks: QobuzSearchResultList {
                items: vec![QobuzTrack {
                    id: 500,
                    track_number: 1,
                    artist: "Track Artist".to_string(),
                    artist_id: 50,
                    album: "Track Album".to_string(),
                    album_id: "track_album".to_string(),
                    album_type: crate::QobuzAlbumReleaseType::Single,
                    image: None,
                    copyright: None,
                    duration: 200,
                    parental_warning: false,
                    isrc: "TRACK123".to_string(),
                    title: "Track Title".to_string(),
                    version: None,
                }],
                offset: 0,
                limit: 10,
                total: 1,
            },
        };

        let response: ApiSearchResultsResponse = results.into();

        // Should have 3 results: 1 artist + 1 album + 1 track
        assert_eq!(response.results.len(), 3);

        // Verify we have one of each type (order is artists, albums, tracks)
        let has_artist = response
            .results
            .iter()
            .any(|r| matches!(r, ApiGlobalSearchResult::Artist(_)));
        let has_album = response
            .results
            .iter()
            .any(|r| matches!(r, ApiGlobalSearchResult::Album(_)));
        let has_track = response
            .results
            .iter()
            .any(|r| matches!(r, ApiGlobalSearchResult::Track(_)));

        assert!(has_artist);
        assert!(has_album);
        assert!(has_track);
    }
}
