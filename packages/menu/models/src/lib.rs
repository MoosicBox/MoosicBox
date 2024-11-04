#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

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
