//! API-compatible serializable types for album version data.
//!
//! This module provides types that can be serialized/deserialized for use in
//! HTTP APIs, with conversions to and from the core domain models.

#![allow(clippy::module_name_repetitions)]

use moosicbox_music_models::{AlbumVersionQuality, AudioFormat, TrackApiSource, api::ApiTrack};
use serde::{Deserialize, Serialize};

use crate::AlbumVersion;

/// API representation of an album version with audio quality metadata.
///
/// This is the serializable version of [`AlbumVersion`] used for API responses
/// and requests. It contains the same quality information but uses [`ApiTrack`]
/// for the tracks collection.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiAlbumVersion {
    /// The tracks included in this album version.
    pub tracks: Vec<ApiTrack>,
    /// The audio format (e.g., FLAC, MP3).
    pub format: Option<AudioFormat>,
    /// The bit depth in bits (e.g., 16, 24).
    pub bit_depth: Option<u8>,
    /// The sample rate in Hz (e.g., 44100, 96000).
    pub sample_rate: Option<u32>,
    /// The number of audio channels (e.g., 2 for stereo).
    pub channels: Option<u8>,
    /// The API source this version comes from.
    pub source: TrackApiSource,
}

/// Converts a reference to an API album version into quality metadata.
///
/// This extracts only the audio quality information (format, bit depth, sample rate,
/// channels, and source) from an [`ApiAlbumVersion`], discarding the track list.
impl From<&ApiAlbumVersion> for AlbumVersionQuality {
    fn from(value: &ApiAlbumVersion) -> Self {
        Self {
            format: value.format,
            bit_depth: value.bit_depth,
            sample_rate: value.sample_rate,
            channels: value.channels,
            source: value.source.clone(),
        }
    }
}

/// Converts an API album version into quality metadata.
///
/// This extracts only the audio quality information (format, bit depth, sample rate,
/// channels, and source) from an [`ApiAlbumVersion`], discarding the track list.
impl From<ApiAlbumVersion> for AlbumVersionQuality {
    fn from(value: ApiAlbumVersion) -> Self {
        Self {
            format: value.format,
            bit_depth: value.bit_depth,
            sample_rate: value.sample_rate,
            channels: value.channels,
            source: value.source,
        }
    }
}

/// Converts an API album version into a domain model album version.
///
/// This conversion transforms the API representation (with [`ApiTrack`]s) into the
/// internal domain model (with `Track`s). All quality metadata is preserved.
impl From<ApiAlbumVersion> for AlbumVersion {
    fn from(value: ApiAlbumVersion) -> Self {
        Self {
            tracks: value.tracks.into_iter().map(Into::into).collect(),
            format: value.format,
            bit_depth: value.bit_depth,
            sample_rate: value.sample_rate,
            channels: value.channels,
            source: value.source,
        }
    }
}

/// Converts a domain model album version into an API album version.
///
/// This conversion transforms the internal domain model (with `Track`s) into the
/// API representation (with [`ApiTrack`]s). All quality metadata is preserved.
impl From<AlbumVersion> for ApiAlbumVersion {
    fn from(value: AlbumVersion) -> Self {
        Self {
            tracks: value.tracks.into_iter().map(Into::into).collect(),
            format: value.format,
            bit_depth: value.bit_depth,
            sample_rate: value.sample_rate,
            channels: value.channels,
            source: value.source,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]

    use super::*;
    use moosicbox_music_models::{AudioFormat, Track, TrackApiSource, id::Id};

    /// Creates a test `ApiAlbumVersion` with all quality fields populated.
    fn create_test_api_album_version() -> ApiAlbumVersion {
        ApiAlbumVersion {
            tracks: vec![
                ApiTrack {
                    track_id: Id::Number(1),
                    title: "Test Track 1".to_string(),
                    ..Default::default()
                },
                ApiTrack {
                    track_id: Id::Number(2),
                    title: "Test Track 2".to_string(),
                    ..Default::default()
                },
            ],
            format: Some(AudioFormat::Flac),
            bit_depth: Some(24),
            sample_rate: Some(96000),
            channels: Some(2),
            source: TrackApiSource::Local,
        }
    }

    /// Creates a test `ApiAlbumVersion` with no quality fields populated.
    fn create_test_api_album_version_no_quality() -> ApiAlbumVersion {
        ApiAlbumVersion {
            tracks: vec![ApiTrack {
                track_id: Id::Number(1),
                title: "Test Track".to_string(),
                ..Default::default()
            }],
            format: None,
            bit_depth: None,
            sample_rate: None,
            channels: None,
            source: TrackApiSource::Local,
        }
    }

    /// Creates a test `AlbumVersion` with all quality fields populated.
    fn create_test_album_version() -> AlbumVersion {
        AlbumVersion {
            tracks: vec![
                Track {
                    id: Id::Number(1),
                    title: "Test Track 1".to_string(),
                    ..Default::default()
                },
                Track {
                    id: Id::Number(2),
                    title: "Test Track 2".to_string(),
                    ..Default::default()
                },
            ],
            format: Some(AudioFormat::Flac),
            bit_depth: Some(24),
            sample_rate: Some(96000),
            channels: Some(2),
            source: TrackApiSource::Local,
        }
    }

    #[test_log::test]
    fn test_api_album_version_ref_to_quality_extracts_all_fields() {
        let api_version = create_test_api_album_version();
        let quality: AlbumVersionQuality = (&api_version).into();

        assert_eq!(quality.format, Some(AudioFormat::Flac));
        assert_eq!(quality.bit_depth, Some(24));
        assert_eq!(quality.sample_rate, Some(96000));
        assert_eq!(quality.channels, Some(2));
        assert_eq!(quality.source, TrackApiSource::Local);
    }

    #[test_log::test]
    fn test_api_album_version_ref_to_quality_with_no_quality_fields() {
        let api_version = create_test_api_album_version_no_quality();
        let quality: AlbumVersionQuality = (&api_version).into();

        assert_eq!(quality.format, None);
        assert_eq!(quality.bit_depth, None);
        assert_eq!(quality.sample_rate, None);
        assert_eq!(quality.channels, None);
        assert_eq!(quality.source, TrackApiSource::Local);
    }

    #[test_log::test]
    fn test_api_album_version_owned_to_quality_extracts_all_fields() {
        let api_version = create_test_api_album_version();
        let quality: AlbumVersionQuality = api_version.into();

        assert_eq!(quality.format, Some(AudioFormat::Flac));
        assert_eq!(quality.bit_depth, Some(24));
        assert_eq!(quality.sample_rate, Some(96000));
        assert_eq!(quality.channels, Some(2));
        assert_eq!(quality.source, TrackApiSource::Local);
    }

    #[test_log::test]
    fn test_api_album_version_to_album_version_preserves_all_fields() {
        let api_version = create_test_api_album_version();
        let num_tracks = api_version.tracks.len();

        let domain_version: AlbumVersion = api_version.into();

        assert_eq!(domain_version.tracks.len(), num_tracks);
        assert_eq!(domain_version.format, Some(AudioFormat::Flac));
        assert_eq!(domain_version.bit_depth, Some(24));
        assert_eq!(domain_version.sample_rate, Some(96000));
        assert_eq!(domain_version.channels, Some(2));
        assert_eq!(domain_version.source, TrackApiSource::Local);
    }

    #[test_log::test]
    fn test_api_album_version_to_album_version_transforms_tracks() {
        let api_version = ApiAlbumVersion {
            tracks: vec![
                ApiTrack {
                    track_id: Id::Number(1),
                    title: "Track 1".to_string(),
                    number: 1,
                    duration: 180.5,
                    ..Default::default()
                },
                ApiTrack {
                    track_id: Id::Number(2),
                    title: "Track 2".to_string(),
                    number: 2,
                    duration: 240.0,
                    ..Default::default()
                },
            ],
            format: Some(AudioFormat::Flac),
            bit_depth: Some(16),
            sample_rate: Some(44100),
            channels: Some(2),
            source: TrackApiSource::Local,
        };

        let domain_version: AlbumVersion = api_version.into();

        assert_eq!(domain_version.tracks.len(), 2);
        assert_eq!(domain_version.tracks[0].id, Id::Number(1));
        assert_eq!(domain_version.tracks[0].title, "Track 1");
        assert_eq!(domain_version.tracks[0].number, 1);
        assert_eq!(domain_version.tracks[0].duration, 180.5);
        assert_eq!(domain_version.tracks[1].id, Id::Number(2));
        assert_eq!(domain_version.tracks[1].title, "Track 2");
        assert_eq!(domain_version.tracks[1].number, 2);
        assert_eq!(domain_version.tracks[1].duration, 240.0);
    }

    #[test_log::test]
    fn test_album_version_to_api_album_version_preserves_all_fields() {
        let domain_version = create_test_album_version();
        let num_tracks = domain_version.tracks.len();

        let api_version: ApiAlbumVersion = domain_version.into();

        assert_eq!(api_version.tracks.len(), num_tracks);
        assert_eq!(api_version.format, Some(AudioFormat::Flac));
        assert_eq!(api_version.bit_depth, Some(24));
        assert_eq!(api_version.sample_rate, Some(96000));
        assert_eq!(api_version.channels, Some(2));
        assert_eq!(api_version.source, TrackApiSource::Local);
    }

    #[test_log::test]
    fn test_album_version_to_api_album_version_transforms_tracks() {
        let domain_version = AlbumVersion {
            tracks: vec![
                Track {
                    id: Id::Number(1),
                    title: "Track 1".to_string(),
                    number: 1,
                    duration: 180.5,
                    ..Default::default()
                },
                Track {
                    id: Id::Number(2),
                    title: "Track 2".to_string(),
                    number: 2,
                    duration: 240.0,
                    ..Default::default()
                },
            ],
            format: Some(AudioFormat::Flac),
            bit_depth: Some(16),
            sample_rate: Some(44100),
            channels: Some(2),
            source: TrackApiSource::Local,
        };

        let api_version: ApiAlbumVersion = domain_version.into();

        assert_eq!(api_version.tracks.len(), 2);
        assert_eq!(api_version.tracks[0].track_id, Id::Number(1));
        assert_eq!(api_version.tracks[0].title, "Track 1");
        assert_eq!(api_version.tracks[0].number, 1);
        assert_eq!(api_version.tracks[0].duration, 180.5);
        assert_eq!(api_version.tracks[1].track_id, Id::Number(2));
        assert_eq!(api_version.tracks[1].title, "Track 2");
        assert_eq!(api_version.tracks[1].number, 2);
        assert_eq!(api_version.tracks[1].duration, 240.0);
    }

    #[test_log::test]
    fn test_roundtrip_album_version_to_api_and_back() {
        let original = AlbumVersion {
            tracks: vec![Track {
                id: Id::Number(1),
                title: "Test Track".to_string(),
                number: 1,
                duration: 200.0,
                ..Default::default()
            }],
            format: Some(AudioFormat::Flac),
            bit_depth: Some(24),
            sample_rate: Some(96000),
            channels: Some(2),
            source: TrackApiSource::Local,
        };

        let api_version: ApiAlbumVersion = original.clone().into();
        let roundtrip: AlbumVersion = api_version.into();

        // Verify quality fields are preserved
        assert_eq!(roundtrip.format, original.format);
        assert_eq!(roundtrip.bit_depth, original.bit_depth);
        assert_eq!(roundtrip.sample_rate, original.sample_rate);
        assert_eq!(roundtrip.channels, original.channels);
        assert_eq!(roundtrip.source, original.source);

        // Verify track data is preserved
        assert_eq!(roundtrip.tracks.len(), original.tracks.len());
        assert_eq!(roundtrip.tracks[0].id, original.tracks[0].id);
        assert_eq!(roundtrip.tracks[0].title, original.tracks[0].title);
        assert_eq!(roundtrip.tracks[0].number, original.tracks[0].number);
        assert_eq!(roundtrip.tracks[0].duration, original.tracks[0].duration);
    }

    #[test_log::test]
    fn test_conversion_with_empty_tracks() {
        let api_version = ApiAlbumVersion {
            tracks: vec![],
            format: Some(AudioFormat::Flac),
            bit_depth: Some(16),
            sample_rate: Some(44100),
            channels: Some(2),
            source: TrackApiSource::Local,
        };

        let domain_version: AlbumVersion = api_version.into();

        assert_eq!(domain_version.tracks.len(), 0);
        assert_eq!(domain_version.format, Some(AudioFormat::Flac));
    }
}
