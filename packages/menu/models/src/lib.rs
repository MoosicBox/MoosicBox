//! Data models for album versions in `MoosicBox` menu system.
//!
//! This crate provides data structures for representing album versions with
//! audio quality metadata, including format, bit depth, sample rate, and channel
//! information from various sources.
//!
//! # Main Types
//!
//! * [`AlbumVersion`] - Core domain model for an album version with tracks and quality info
//! * [`api::ApiAlbumVersion`] - Serializable API representation for HTTP requests/responses

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_music_models::{AudioFormat, Track, TrackApiSource};

#[cfg(feature = "api")]
pub mod api;

/// Represents a specific version of an album with audio quality metadata.
///
/// An album may have multiple versions with different audio formats, bit depths,
/// sample rates, and channel configurations from various sources.
#[derive(Debug, Clone)]
pub struct AlbumVersion {
    /// The tracks included in this album version.
    pub tracks: Vec<Track>,
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
