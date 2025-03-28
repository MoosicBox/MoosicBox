#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "api")]
pub mod api;

#[cfg(feature = "db")]
pub mod db;

use std::{path::PathBuf, str::FromStr as _};

use moosicbox_date_utils::chrono::{self, parse_date_time};
use moosicbox_json_utils::{ParseError, ToValueType};
use moosicbox_music_models::{
    Album, AlbumSource, AlbumType, AlbumVersionQuality, ApiSource, ApiSources, Artist, AudioFormat,
    Track, TrackApiSource, id::TryFromIdError,
};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct LibraryArtist {
    pub id: u64,
    pub title: String,
    pub cover: Option<String>,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<u64>,
    pub yt_id: Option<u64>,
}

impl From<LibraryArtist> for Artist {
    fn from(value: LibraryArtist) -> Self {
        Self {
            id: value.id.into(),
            title: value.title,
            cover: value.cover,
            api_source: ApiSource::Library,
            api_sources: {
                #[allow(unused_mut)]
                let mut sources = ApiSources::default();
                #[cfg(feature = "tidal")]
                {
                    sources =
                        sources.with_source_opt(ApiSource::Tidal, value.tidal_id.map(Into::into));
                }
                #[cfg(feature = "qobuz")]
                {
                    sources =
                        sources.with_source_opt(ApiSource::Qobuz, value.qobuz_id.map(Into::into));
                }
                #[cfg(feature = "yt")]
                {
                    sources = sources.with_source_opt(ApiSource::Yt, value.yt_id.map(Into::into));
                }
                sources
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiLibraryArtist {
    pub artist_id: u64,
    pub title: String,
    pub contains_cover: bool,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<u64>,
    pub yt_id: Option<u64>,
}

#[derive(
    Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum LibraryAlbumType {
    #[default]
    Lp,
    Live,
    Compilations,
    EpsAndSingles,
    Other,
}

impl From<AlbumType> for LibraryAlbumType {
    fn from(value: AlbumType) -> Self {
        match value {
            AlbumType::Lp => Self::Lp,
            AlbumType::Live => Self::Live,
            AlbumType::Compilations => Self::Compilations,
            AlbumType::EpsAndSingles => Self::EpsAndSingles,
            AlbumType::Other | AlbumType::Download => Self::Other,
        }
    }
}

impl From<LibraryAlbumType> for AlbumType {
    fn from(value: LibraryAlbumType) -> Self {
        match value {
            LibraryAlbumType::Lp => Self::Lp,
            LibraryAlbumType::Live => Self::Live,
            LibraryAlbumType::Compilations => Self::Compilations,
            LibraryAlbumType::EpsAndSingles => Self::EpsAndSingles,
            LibraryAlbumType::Other => Self::Other,
        }
    }
}

impl ToValueType<LibraryAlbumType> for &serde_json::Value {
    fn to_value_type(self) -> Result<LibraryAlbumType, ParseError> {
        LibraryAlbumType::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("AlbumType".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("AlbumType".into()))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct LibraryAlbum {
    pub id: u64,
    pub title: String,
    pub artist: String,
    pub artist_id: u64,
    pub album_type: LibraryAlbumType,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub artwork: Option<String>,
    pub directory: Option<String>,
    pub source: AlbumSource,
    pub blur: bool,
    pub versions: Vec<AlbumVersionQuality>,
    pub album_sources: ApiSources,
    pub artist_sources: ApiSources,
}

impl TryFrom<LibraryAlbum> for Album {
    type Error = chrono::ParseError;

    fn try_from(value: LibraryAlbum) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
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
            artwork: value.artwork,
            directory: value.directory,
            blur: value.blur,
            versions: value.versions,
            album_source: value.source,
            api_source: ApiSource::Library,
            artist_sources: value.artist_sources,
            album_sources: value.album_sources,
        })
    }
}

impl TryFrom<Album> for LibraryAlbum {
    type Error = TryFromIdError;

    fn try_from(value: Album) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id.try_into()?,
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.try_into()?,
            album_type: value.album_type.into(),
            date_released: value.date_released.map(|x| x.and_utc().to_rfc3339()),
            date_added: value.date_added.map(|x| x.and_utc().to_rfc3339()),
            artwork: value.artwork,
            directory: value.directory,
            blur: value.blur,
            versions: value.versions,
            source: AlbumSource::Local,
            album_sources: value.album_sources,
            artist_sources: value.artist_sources,
        })
    }
}

#[must_use]
pub const fn track_source_to_u8(source: TrackApiSource) -> u8 {
    match source {
        TrackApiSource::Local => 1,
        #[cfg(feature = "tidal")]
        TrackApiSource::Tidal => 2,
        #[cfg(feature = "qobuz")]
        TrackApiSource::Qobuz => 3,
        #[cfg(feature = "yt")]
        TrackApiSource::Yt => 4,
    }
}

pub fn sort_album_versions(versions: &mut [AlbumVersionQuality]) {
    versions.sort_by(|a, b| {
        b.sample_rate
            .unwrap_or_default()
            .cmp(&a.sample_rate.unwrap_or_default())
    });
    versions.sort_by(|a, b| {
        b.bit_depth
            .unwrap_or_default()
            .cmp(&a.bit_depth.unwrap_or_default())
    });
    versions.sort_by(|a, b| track_source_to_u8(a.source).cmp(&track_source_to_u8(b.source)));
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct LibraryTrack {
    pub id: u64,
    pub number: u32,
    pub title: String,
    pub duration: f64,
    pub album: String,
    pub album_id: u64,
    pub album_type: LibraryAlbumType,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub artist: String,
    pub artist_id: u64,
    pub file: Option<String>,
    pub artwork: Option<String>,
    pub blur: bool,
    pub bytes: u64,
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub audio_bitrate: Option<u32>,
    pub overall_bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackApiSource,
    pub api_source: ApiSource,
    pub qobuz_id: Option<u64>,
    pub tidal_id: Option<u64>,
    pub yt_id: Option<u64>,
}

impl LibraryTrack {
    #[must_use]
    /// # Panics
    ///
    /// Will panic if directory doesn't exist
    pub fn directory(&self) -> Option<String> {
        self.file
            .as_ref()
            .and_then(|f| PathBuf::from_str(f).ok())
            .map(|p| p.parent().unwrap().to_str().unwrap().to_string())
    }
}

impl From<LibraryTrack> for Track {
    fn from(value: LibraryTrack) -> Self {
        Self {
            id: value.id.into(),
            number: value.number,
            title: value.title,
            duration: value.duration,
            album: value.album,
            album_id: value.album_id.into(),
            album_type: value.album_type.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            file: value.file,
            artwork: value.artwork,
            blur: value.blur,
            bytes: value.bytes,
            format: value.format,
            bit_depth: value.bit_depth,
            audio_bitrate: value.audio_bitrate,
            overall_bitrate: value.overall_bitrate,
            sample_rate: value.sample_rate,
            channels: value.channels,
            track_source: value.source,
            api_source: ApiSource::Library,
            sources: {
                #[allow(unused_mut)]
                let mut sources = ApiSources::default();
                #[cfg(feature = "tidal")]
                {
                    sources =
                        sources.with_source_opt(ApiSource::Tidal, value.tidal_id.map(Into::into));
                }
                #[cfg(feature = "qobuz")]
                {
                    sources =
                        sources.with_source_opt(ApiSource::Qobuz, value.qobuz_id.map(Into::into));
                }
                #[cfg(feature = "yt")]
                {
                    sources = sources.with_source_opt(ApiSource::Yt, value.yt_id.map(Into::into));
                }
                sources
            },
        }
    }
}
