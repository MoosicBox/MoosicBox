#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

#[cfg(feature = "db")]
pub mod db;

use std::{path::PathBuf, str::FromStr as _};

use moosicbox_core::{
    sqlite::models::{
        Album, AlbumSource, AlbumVersionQuality, ApiAlbum, ApiAlbumVersionQuality, ApiSource,
        ApiSources, ApiTrack, Artist, ToApi, Track, TrackApiSource,
    },
    types::AudioFormat,
};
use moosicbox_json_utils::{ParseError, ToValueType};
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

impl From<LibraryArtist> for moosicbox_core::sqlite::models::ApiArtist {
    fn from(value: LibraryArtist) -> Self {
        let artist: Artist = value.into();
        artist.into()
    }
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

impl From<moosicbox_core::sqlite::models::AlbumType> for LibraryAlbumType {
    fn from(value: moosicbox_core::sqlite::models::AlbumType) -> Self {
        match value {
            moosicbox_core::sqlite::models::AlbumType::Lp => Self::Lp,
            moosicbox_core::sqlite::models::AlbumType::Live => Self::Live,
            moosicbox_core::sqlite::models::AlbumType::Compilations => Self::Compilations,
            moosicbox_core::sqlite::models::AlbumType::EpsAndSingles => Self::EpsAndSingles,
            moosicbox_core::sqlite::models::AlbumType::Other
            | moosicbox_core::sqlite::models::AlbumType::Download => Self::Other,
        }
    }
}

impl From<LibraryAlbumType> for moosicbox_core::sqlite::models::AlbumType {
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
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

impl From<&LibraryAlbum> for ApiAlbum {
    fn from(value: &LibraryAlbum) -> Self {
        let album: Album = value.clone().into();
        album.into()
    }
}

impl From<LibraryAlbum> for ApiAlbum {
    fn from(value: LibraryAlbum) -> Self {
        let album: Album = value.into();
        album.into()
    }
}

impl From<LibraryAlbum> for Album {
    fn from(value: LibraryAlbum) -> Self {
        Self {
            id: value.id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            artwork: value.artwork,
            directory: value.directory,
            blur: value.blur,
            versions: value.versions,
            album_source: value.source,
            api_source: ApiSource::Library,
            artist_sources: value.artist_sources,
            album_sources: value.album_sources,
        }
    }
}

impl From<Album> for LibraryAlbum {
    fn from(value: Album) -> Self {
        Self {
            id: value.id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            artwork: value.artwork,
            directory: value.directory,
            blur: value.blur,
            versions: value.versions,
            source: AlbumSource::Local,
            album_sources: value.album_sources,
            artist_sources: value.artist_sources,
        }
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

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiLibraryAlbum {
    pub album_id: u64,
    pub title: String,
    pub artist: String,
    pub artist_id: u64,
    pub album_type: LibraryAlbumType,
    pub contains_cover: bool,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub source: AlbumSource,
    pub blur: bool,
    pub versions: Vec<ApiAlbumVersionQuality>,
    pub album_sources: ApiSources,
    pub artist_sources: ApiSources,
}

impl From<ApiLibraryAlbum> for moosicbox_core::sqlite::models::ApiAlbum {
    fn from(value: ApiLibraryAlbum) -> Self {
        Self {
            album_id: value.album_id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            contains_cover: value.contains_cover,
            blur: value.blur,
            versions: value
                .versions
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
            album_source: value.source,
            api_source: ApiSource::Library,
            album_sources: value.album_sources,
            artist_sources: value.artist_sources,
        }
    }
}

impl From<ApiLibraryAlbum> for Album {
    fn from(value: ApiLibraryAlbum) -> Self {
        Self {
            id: value.album_id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            album_type: value.album_type.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            artwork: if value.contains_cover {
                Some(value.album_id.to_string())
            } else {
                None
            },
            directory: None,
            blur: value.blur,
            versions: vec![],
            album_source: value.source,
            api_source: ApiSource::Library,
            album_sources: value.album_sources,
            artist_sources: value.artist_sources,
        }
    }
}

impl ToApi<ApiLibraryAlbum> for LibraryAlbum {
    fn to_api(self) -> ApiLibraryAlbum {
        ApiLibraryAlbum {
            album_id: self.id,
            title: self.title,
            artist: self.artist,
            artist_id: self.artist_id,
            album_type: self.album_type,
            contains_cover: self.artwork.is_some(),
            date_released: self.date_released,
            date_added: self.date_added,
            source: self.source,
            blur: self.blur,
            versions: self
                .versions
                .iter()
                .map(moosicbox_core::sqlite::models::ToApi::to_api)
                .collect(),
            album_sources: self.album_sources,
            artist_sources: self.artist_sources,
        }
    }
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

impl From<LibraryTrack> for ApiTrack {
    fn from(value: LibraryTrack) -> Self {
        let track: Track = value.into();
        track.into()
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiLibraryTrack {
    pub track_id: u64,
    pub number: u32,
    pub title: String,
    pub duration: f64,
    pub artist: String,
    pub artist_id: u64,
    pub album_type: LibraryAlbumType,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub album: String,
    pub album_id: u64,
    pub contains_cover: bool,
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
}

impl From<&ApiLibraryTrack> for LibraryTrack {
    fn from(value: &ApiLibraryTrack) -> Self {
        value.clone().into()
    }
}

impl From<ApiLibraryTrack> for LibraryTrack {
    fn from(value: ApiLibraryTrack) -> Self {
        Self {
            id: value.track_id,
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
            artwork: None,
            blur: value.blur,
            bytes: value.bytes,
            format: value.format,
            bit_depth: value.bit_depth,
            audio_bitrate: value.audio_bitrate,
            overall_bitrate: value.overall_bitrate,
            sample_rate: value.sample_rate,
            channels: value.channels,
            source: value.source,
            api_source: value.api_source,
            qobuz_id: None,
            tidal_id: None,
            yt_id: None,
        }
    }
}

impl From<ApiLibraryTrack> for Track {
    fn from(value: ApiLibraryTrack) -> Self {
        Self {
            id: value.track_id.into(),
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
            file: None,
            artwork: None,
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
            sources: ApiSources::default().with_source(ApiSource::Library, value.track_id.into()),
        }
    }
}
