//! API-specific model types for serialization and network transfer.
//!
//! This module provides lightweight versions of core types optimized for API responses.
//! The main difference from the core types is the use of `contains_cover` boolean flags
//! instead of full cover URLs, reducing payload size for network transfers.

use moosicbox_date_utils::chrono::{self, parse_date_time};
use serde::{Deserialize, Serialize};

use crate::{
    Album, AlbumSource, AlbumType, AlbumVersionQuality, ApiSource, ApiSources, Artist, AudioFormat,
    Track, TrackApiSource, id::Id,
};

/// API-optimized representation of an artist.
///
/// Uses `contains_cover` boolean instead of full cover URL to reduce payload size.
#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiArtist {
    /// Unique identifier for the artist
    pub artist_id: Id,
    /// Artist name
    pub title: String,
    /// Whether cover artwork is available
    pub contains_cover: bool,
    /// The primary API source for this artist
    pub api_source: ApiSource,
    /// All API sources where this artist is available
    pub api_sources: ApiSources,
}

impl From<Artist> for ApiArtist {
    fn from(value: Artist) -> Self {
        Self {
            artist_id: value.id,
            title: value.title,
            contains_cover: value.cover.is_some(),
            api_source: value.api_source,
            api_sources: value.api_sources,
        }
    }
}

impl From<ApiArtist> for Artist {
    fn from(value: ApiArtist) -> Self {
        Self {
            id: value.artist_id.clone(),
            title: value.title,
            cover: if value.contains_cover {
                Some(value.artist_id.to_string())
            } else {
                None
            },
            api_source: value.api_source,
            api_sources: value.api_sources,
        }
    }
}

/// API-optimized representation of album version quality characteristics.
///
/// Identical to [`AlbumVersionQuality`] but provided for API consistency.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ApiAlbumVersionQuality {
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

impl From<ApiAlbumVersionQuality> for AlbumVersionQuality {
    fn from(value: ApiAlbumVersionQuality) -> Self {
        Self {
            format: value.format,
            bit_depth: value.bit_depth,
            sample_rate: value.sample_rate,
            channels: value.channels,
            source: value.source,
        }
    }
}

impl From<AlbumVersionQuality> for ApiAlbumVersionQuality {
    fn from(value: AlbumVersionQuality) -> Self {
        Self {
            format: value.format,
            bit_depth: value.bit_depth,
            sample_rate: value.sample_rate,
            channels: value.channels,
            source: value.source,
        }
    }
}

/// API-optimized representation of a music track.
///
/// Uses `contains_cover` boolean instead of full artwork URL to reduce payload size.
/// Does not include file path information which is only relevant server-side.
#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiTrack {
    /// Unique identifier for the track
    pub track_id: Id,
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
    /// Whether cover artwork is available
    pub contains_cover: bool,
    /// Whether to blur the artwork
    pub blur: bool,
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

impl From<Track> for ApiTrack {
    fn from(value: Track) -> Self {
        Self {
            track_id: value.id,
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
            contains_cover: value.artwork.is_some(),
            blur: value.blur,
            format: value.format,
            bit_depth: value.bit_depth,
            audio_bitrate: value.audio_bitrate,
            overall_bitrate: value.overall_bitrate,
            sample_rate: value.sample_rate,
            channels: value.channels,
            track_source: value.track_source,
            api_source: value.api_source,
            sources: value.sources,
        }
    }
}

impl From<ApiTrack> for Track {
    fn from(value: ApiTrack) -> Self {
        Self {
            id: value.track_id.clone(),
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
            artwork: if value.contains_cover {
                Some(value.track_id.to_string())
            } else {
                None
            },
            blur: value.blur,
            bytes: 0,
            format: value.format,
            bit_depth: value.bit_depth,
            audio_bitrate: value.audio_bitrate,
            overall_bitrate: value.overall_bitrate,
            sample_rate: value.sample_rate,
            channels: value.channels,
            track_source: value.track_source,
            api_source: value.api_source,
            sources: value.sources,
        }
    }
}

/// API-optimized representation of a music album.
///
/// Uses `contains_cover` boolean instead of full artwork URL to reduce payload size.
/// Does not include directory path information which is only relevant server-side.
#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ApiAlbum {
    /// Unique identifier for the album
    pub album_id: Id,
    /// Album title
    pub title: String,
    /// Artist name
    pub artist: String,
    /// Artist identifier
    pub artist_id: Id,
    /// Album type (LP, Live, etc.)
    pub album_type: AlbumType,
    /// Release date as ISO 8601 string
    pub date_released: Option<String>,
    /// Date added to library as ISO 8601 string
    pub date_added: Option<String>,
    /// Whether cover artwork is available
    pub contains_cover: bool,
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

impl From<Album> for ApiAlbum {
    fn from(value: Album) -> Self {
        Self {
            album_id: value.id,
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id,
            album_type: value.album_type,
            date_released: value.date_released.map(|x| x.and_utc().to_rfc3339()),
            date_added: value.date_added.map(|x| x.and_utc().to_rfc3339()),
            contains_cover: value.artwork.is_some(),
            blur: value.blur,
            versions: value.versions,
            album_source: value.album_source,
            api_source: value.api_source,
            artist_sources: value.artist_sources,
            album_sources: value.album_sources,
        }
    }
}

impl TryFrom<ApiAlbum> for Album {
    type Error = chrono::ParseError;

    /// Attempts to convert an API album to an album.
    ///
    /// # Errors
    ///
    /// * If date parsing fails for `date_released` or `date_added` fields
    fn try_from(value: ApiAlbum) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.album_id.clone(),
            title: value.title,
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
            artwork: if value.contains_cover {
                Some(value.album_id.to_string())
            } else {
                None
            },
            blur: value.blur,
            versions: value.versions,
            album_source: value.album_source,
            api_source: value.api_source,
            artist_sources: value.artist_sources,
            album_sources: value.album_sources,
            ..Default::default()
        })
    }
}

impl From<&ApiTrack> for ApiAlbum {
    fn from(value: &ApiTrack) -> Self {
        value.clone().into()
    }
}

impl From<ApiTrack> for ApiAlbum {
    fn from(value: ApiTrack) -> Self {
        Self {
            album_id: value.album_id,
            title: value.album,
            artist: value.artist,
            artist_id: value.artist_id,
            album_type: value.album_type,
            date_released: value.date_released,
            date_added: value.date_added,
            contains_cover: value.contains_cover,
            blur: value.blur,
            versions: vec![],
            album_source: value.track_source.into(),
            api_source: value.api_source,
            artist_sources: value.sources.clone(),
            album_sources: value.sources,
        }
    }
}

impl From<&Track> for ApiAlbum {
    fn from(value: &Track) -> Self {
        value.clone().into()
    }
}

impl From<Track> for ApiAlbum {
    fn from(value: Track) -> Self {
        Self {
            album_id: value.album_id,
            title: value.album,
            artist: value.artist,
            artist_id: value.artist_id,
            album_type: value.album_type,
            date_released: value.date_released,
            date_added: value.date_added,
            contains_cover: value.artwork.is_some(),
            blur: value.blur,
            versions: vec![],
            album_source: value.track_source.into(),
            api_source: value.api_source,
            artist_sources: value.sources.clone(),
            album_sources: value.sources,
        }
    }
}
