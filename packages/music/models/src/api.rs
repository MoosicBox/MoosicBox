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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn test_artist_to_api_artist() {
        let artist = Artist {
            id: Id::Number(1),
            title: "Test Artist".to_string(),
            cover: Some("cover.jpg".to_string()),
            api_source: ApiSource::library(),
            api_sources: ApiSources::default(),
        };

        let api_artist: ApiArtist = artist.clone().into();
        assert_eq!(api_artist.artist_id, Id::Number(1));
        assert_eq!(api_artist.title, "Test Artist");
        assert!(api_artist.contains_cover);
        assert!(api_artist.api_source.is_library());

        // Test without cover
        let artist_no_cover = Artist {
            cover: None,
            ..artist
        };
        let api_artist_no_cover: ApiArtist = artist_no_cover.into();
        assert!(!api_artist_no_cover.contains_cover);
    }

    #[test_log::test]
    fn test_api_artist_to_artist() {
        let api_artist = ApiArtist {
            artist_id: Id::Number(1),
            title: "Test Artist".to_string(),
            contains_cover: true,
            api_source: ApiSource::library(),
            api_sources: ApiSources::default(),
        };

        let artist: Artist = api_artist.clone().into();
        assert_eq!(artist.id, Id::Number(1));
        assert_eq!(artist.title, "Test Artist");
        assert_eq!(artist.cover, Some("1".to_string())); // ID becomes the cover

        // Test without cover
        let api_artist_no_cover = ApiArtist {
            contains_cover: false,
            ..api_artist
        };
        let artist_no_cover: Artist = api_artist_no_cover.into();
        assert_eq!(artist_no_cover.cover, None);
    }

    #[test_log::test]
    fn test_track_to_api_track() {
        let track = Track {
            id: Id::Number(1),
            number: 5,
            title: "Test Track".to_string(),
            duration: 180.5,
            album: "Test Album".to_string(),
            album_id: Id::Number(10),
            album_type: AlbumType::Lp,
            date_released: Some("2023-01-15T00:00:00Z".to_string()),
            date_added: Some("2024-01-01T12:00:00Z".to_string()),
            artist: "Test Artist".to_string(),
            artist_id: Id::Number(20),
            file: Some("/music/track.flac".to_string()),
            artwork: Some("artwork.jpg".to_string()),
            blur: false,
            bytes: 1024,
            format: Some(AudioFormat::Source),
            bit_depth: Some(24),
            audio_bitrate: Some(320_000),
            overall_bitrate: Some(350_000),
            sample_rate: Some(48_000),
            channels: Some(2),
            track_source: TrackApiSource::Local,
            api_source: ApiSource::library(),
            sources: ApiSources::default(),
        };

        let api_track: ApiTrack = track.into();
        assert_eq!(api_track.track_id, Id::Number(1));
        assert_eq!(api_track.number, 5);
        assert_eq!(api_track.title, "Test Track");
        assert!((api_track.duration - 180.5).abs() < f64::EPSILON);
        assert_eq!(api_track.album, "Test Album");
        assert!(api_track.contains_cover);
        assert_eq!(api_track.format, Some(AudioFormat::Source));
        assert_eq!(api_track.bit_depth, Some(24));
        assert_eq!(api_track.sample_rate, Some(48_000));
    }

    #[test_log::test]
    fn test_api_track_to_track() {
        let api_track = ApiTrack {
            track_id: Id::Number(1),
            number: 5,
            title: "Test Track".to_string(),
            duration: 180.5,
            album: "Test Album".to_string(),
            album_id: Id::Number(10),
            album_type: AlbumType::Lp,
            date_released: Some("2023-01-15T00:00:00Z".to_string()),
            date_added: Some("2024-01-01T12:00:00Z".to_string()),
            artist: "Test Artist".to_string(),
            artist_id: Id::Number(20),
            contains_cover: true,
            blur: false,
            format: Some(AudioFormat::Source),
            bit_depth: Some(24),
            audio_bitrate: Some(320_000),
            overall_bitrate: Some(350_000),
            sample_rate: Some(48_000),
            channels: Some(2),
            track_source: TrackApiSource::Local,
            api_source: ApiSource::library(),
            sources: ApiSources::default(),
        };

        let track: Track = api_track.into();
        assert_eq!(track.id, Id::Number(1));
        assert_eq!(track.number, 5);
        assert_eq!(track.title, "Test Track");
        assert_eq!(track.file, None); // API track has no file
        assert_eq!(track.artwork, Some("1".to_string())); // ID becomes artwork
        assert_eq!(track.bytes, 0); // API track has no bytes
    }

    #[test_log::test]
    fn test_album_version_quality_conversions() {
        let quality = AlbumVersionQuality {
            format: Some(AudioFormat::Source),
            bit_depth: Some(24),
            sample_rate: Some(48_000),
            channels: Some(2),
            source: TrackApiSource::Local,
        };

        let api_quality: ApiAlbumVersionQuality = quality.clone().into();
        assert_eq!(api_quality.format, Some(AudioFormat::Source));
        assert_eq!(api_quality.bit_depth, Some(24));
        assert_eq!(api_quality.sample_rate, Some(48_000));
        assert_eq!(api_quality.channels, Some(2));

        let back_to_quality: AlbumVersionQuality = api_quality.into();
        assert_eq!(back_to_quality, quality);
    }

    #[test_log::test]
    fn test_album_to_api_album() {
        use moosicbox_date_utils::chrono::NaiveDate;

        let date = NaiveDate::from_ymd_opt(2023, 1, 15)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();

        let album = Album {
            id: Id::Number(1),
            title: "Test Album".to_string(),
            artist: "Test Artist".to_string(),
            artist_id: Id::Number(10),
            album_type: AlbumType::Lp,
            date_released: Some(date),
            date_added: Some(date),
            artwork: Some("artwork.jpg".to_string()),
            directory: Some("/music/album".to_string()),
            blur: false,
            versions: vec![],
            album_source: AlbumSource::Local,
            api_source: ApiSource::library(),
            artist_sources: ApiSources::default(),
            album_sources: ApiSources::default(),
        };

        let api_album: ApiAlbum = album.into();
        assert_eq!(api_album.album_id, Id::Number(1));
        assert_eq!(api_album.title, "Test Album");
        assert!(api_album.contains_cover);
        assert!(api_album.date_released.is_some());
        assert!(api_album.date_added.is_some());
    }

    #[test_log::test]
    fn test_api_album_to_album() {
        let api_album = ApiAlbum {
            album_id: Id::Number(1),
            title: "Test Album".to_string(),
            artist: "Test Artist".to_string(),
            artist_id: Id::Number(10),
            album_type: AlbumType::Lp,
            date_released: Some("2023-01-15T00:00:00Z".to_string()),
            date_added: Some("2024-01-01T12:00:00Z".to_string()),
            contains_cover: true,
            blur: false,
            versions: vec![],
            album_source: AlbumSource::Local,
            api_source: ApiSource::library(),
            artist_sources: ApiSources::default(),
            album_sources: ApiSources::default(),
        };

        let album: Album = api_album.try_into().unwrap();
        assert_eq!(album.id, Id::Number(1));
        assert_eq!(album.title, "Test Album");
        assert_eq!(album.artwork, Some("1".to_string()));
        assert!(album.date_released.is_some());
        assert!(album.date_added.is_some());
        assert_eq!(album.directory, None); // API album has no directory
    }

    #[test_log::test]
    fn test_api_album_to_album_invalid_date() {
        let api_album = ApiAlbum {
            date_released: Some("invalid-date".to_string()),
            ..Default::default()
        };

        let result: Result<Album, _> = api_album.try_into();
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_api_track_to_api_album() {
        let api_track = ApiTrack {
            track_id: Id::Number(1),
            album_id: Id::Number(100),
            album: "Test Album".to_string(),
            artist: "Test Artist".to_string(),
            artist_id: Id::Number(50),
            album_type: AlbumType::Live,
            date_released: Some("2023-01-15T00:00:00Z".to_string()),
            date_added: Some("2024-01-01T12:00:00Z".to_string()),
            contains_cover: true,
            blur: false,
            track_source: TrackApiSource::Local,
            api_source: ApiSource::library(),
            sources: ApiSources::default(),
            ..Default::default()
        };

        let api_album: ApiAlbum = api_track.clone().into();
        assert_eq!(api_album.album_id, Id::Number(100));
        assert_eq!(api_album.title, "Test Album");
        assert_eq!(api_album.artist, "Test Artist");
        assert_eq!(api_album.album_type, AlbumType::Live);
        assert!(api_album.contains_cover);

        // Test reference conversion
        let api_album_ref: ApiAlbum = (&api_track).into();
        assert_eq!(api_album_ref.album_id, Id::Number(100));
    }

    #[test_log::test]
    fn test_track_to_api_album() {
        let track = Track {
            id: Id::Number(1),
            album_id: Id::Number(100),
            album: "Test Album".to_string(),
            artist: "Test Artist".to_string(),
            artist_id: Id::Number(50),
            album_type: AlbumType::EpsAndSingles,
            date_released: Some("2023-01-15T00:00:00Z".to_string()),
            date_added: Some("2024-01-01T12:00:00Z".to_string()),
            artwork: Some("artwork.jpg".to_string()),
            blur: true,
            track_source: TrackApiSource::Local,
            api_source: ApiSource::library(),
            sources: ApiSources::default(),
            ..Default::default()
        };

        let api_album: ApiAlbum = track.clone().into();
        assert_eq!(api_album.album_id, Id::Number(100));
        assert_eq!(api_album.title, "Test Album");
        assert_eq!(api_album.album_type, AlbumType::EpsAndSingles);
        assert!(api_album.contains_cover);
        assert!(api_album.blur);

        // Test reference conversion
        let api_album_ref: ApiAlbum = (&track).into();
        assert_eq!(api_album_ref.album_id, Id::Number(100));
    }
}
