#![allow(clippy::module_name_repetitions)]

use moosicbox_core::{
    sqlite::models::{ApiTrack, ToApi, TrackApiSource},
    types::AudioFormat,
};
use serde::{Deserialize, Serialize};

use crate::AlbumVersion;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiAlbumVersion {
    pub tracks: Vec<ApiTrack>,
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackApiSource,
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

impl ToApi<ApiAlbumVersion> for AlbumVersion {
    fn to_api(self) -> ApiAlbumVersion {
        self.into()
    }
}
