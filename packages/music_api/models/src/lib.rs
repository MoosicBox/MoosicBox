#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use moosicbox_music_models::{
    AlbumSort, AlbumSource, AlbumType, AudioFormat, TrackApiSource, id::Id,
};
use std::str::FromStr as _;

use moosicbox_database::DatabaseValue;
use moosicbox_json_utils::{MissingValue, ParseError, ToValueType};
use moosicbox_paging::PagingRequest;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumsRequest {
    pub sources: Option<Vec<AlbumSource>>,
    pub sort: Option<AlbumSort>,
    pub filters: Option<AlbumFilters>,
    pub page: Option<PagingRequest>,
}

#[derive(Default, Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumFilters {
    pub name: Option<String>,
    pub artist: Option<String>,
    pub search: Option<String>,
    pub album_type: Option<AlbumType>,
    pub artist_id: Option<Id>,
    pub tidal_artist_id: Option<Id>,
    pub qobuz_artist_id: Option<Id>,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ArtistOrder {
    DateAdded,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ArtistOrderDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AlbumOrder {
    DateAdded,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AlbumOrderDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackOrder {
    DateAdded,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackOrderDirection {
    Ascending,
    Descending,
}

#[derive(Clone, Debug)]
pub enum TrackSource {
    LocalFilePath {
        path: String,
        format: AudioFormat,
        track_id: Option<Id>,
        source: TrackApiSource,
    },
    RemoteUrl {
        url: String,
        format: AudioFormat,
        track_id: Option<Id>,
        source: TrackApiSource,
    },
}

impl TrackSource {
    #[must_use]
    pub const fn format(&self) -> AudioFormat {
        match self {
            Self::LocalFilePath { format, .. } | Self::RemoteUrl { format, .. } => *format,
        }
    }

    #[must_use]
    pub const fn track_id(&self) -> Option<&Id> {
        match self {
            Self::LocalFilePath { track_id, .. } | Self::RemoteUrl { track_id, .. } => {
                track_id.as_ref()
            }
        }
    }
}

#[derive(
    Debug, Default, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TrackAudioQuality {
    Low,          // MP3 320
    FlacLossless, // FLAC 16 bit 44.1kHz
    FlacHiRes,    // FLAC 24 bit <= 96kHz
    #[default]
    FlacHighestRes, // FLAC 24 bit > 96kHz <= 192kHz
}

impl MissingValue<TrackAudioQuality> for &moosicbox_database::Row {}
impl ToValueType<TrackAudioQuality> for DatabaseValue {
    fn to_value_type(self) -> Result<TrackAudioQuality, ParseError> {
        TrackAudioQuality::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("TrackAudioQuality".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("TrackAudioQuality".into()))
    }
}

impl ToValueType<TrackAudioQuality> for &serde_json::Value {
    fn to_value_type(self) -> Result<TrackAudioQuality, ParseError> {
        TrackAudioQuality::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("TrackAudioQuality".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("TrackAudioQuality".into()))
    }
}

#[derive(Debug, Clone)]
pub enum ImageCoverSource {
    LocalFilePath(String),
    RemoteUrl(String),
}

#[derive(Clone, Copy, Debug)]
pub enum ImageCoverSize {
    Max,
    Large,
    Medium,
    Small,
    Thumbnail,
}

impl std::fmt::Display for ImageCoverSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let num: u16 = (*self).into();
        f.write_str(&num.to_string())
    }
}

impl From<ImageCoverSize> for u16 {
    fn from(value: ImageCoverSize) -> Self {
        match value {
            ImageCoverSize::Max => 1280,
            ImageCoverSize::Large => 640,
            ImageCoverSize::Medium => 320,
            ImageCoverSize::Small => 160,
            ImageCoverSize::Thumbnail => 80,
        }
    }
}

impl From<u16> for ImageCoverSize {
    fn from(value: u16) -> Self {
        match value {
            0..=80 => Self::Thumbnail,
            81..=160 => Self::Small,
            161..=320 => Self::Medium,
            321..=640 => Self::Large,
            _ => Self::Max,
        }
    }
}

pub trait FromId {
    fn as_string(&self) -> String;
    fn into_id(str: &str) -> Self;
}

impl FromId for String {
    fn as_string(&self) -> String {
        self.to_string()
    }

    fn into_id(str: &str) -> Self {
        str.to_string()
    }
}

impl FromId for u64 {
    fn as_string(&self) -> String {
        self.to_string()
    }

    fn into_id(str: &str) -> Self {
        str.parse::<Self>().unwrap()
    }
}
