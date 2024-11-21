#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use moosicbox_core::{
    sqlite::models::{Track, TrackApiSource},
    types::AudioFormat,
};

#[cfg(feature = "api")]
pub mod api;

#[derive(Clone)]
pub struct AlbumVersion {
    pub tracks: Vec<Track>,
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackApiSource,
}
