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
