#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{ops::Deref, path::PathBuf, str::FromStr, sync::LazyLock};

use id::Id;
use moosicbox_date_utils::chrono::{self, NaiveDateTime, parse_date_time};
use moosicbox_json_utils::{ParseError, ToValueType};
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

pub mod id;

#[cfg(feature = "api")]
pub mod api;

#[cfg(feature = "db")]
pub mod db;

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Artist {
    pub id: Id,
    pub title: String,
    pub cover: Option<String>,
    pub api_source: ApiSource,
    pub api_sources: ApiSources,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum ArtistSort {
    NameAsc,
    NameDesc,
}

impl FromStr for ArtistSort {
    type Err = ();

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "name-asc" | "name" => Ok(Self::NameAsc),
            "name-desc" => Ok(Self::NameDesc),
            _ => Err(()),
        }
    }
}

#[derive(
    Default, Copy, Debug, Serialize, Deserialize, EnumString, AsRefStr, Eq, PartialEq, Clone, Hash,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ApiSource {
    #[default]
    Library,
    #[cfg(feature = "tidal")]
    Tidal,
    #[cfg(feature = "qobuz")]
    Qobuz,
    #[cfg(feature = "yt")]
    Yt,
}

impl ApiSource {
    #[must_use]
    pub fn all() -> &'static [Self] {
        static ALL: LazyLock<Vec<ApiSource>> = LazyLock::new(|| {
            #[allow(unused_mut)]
            let mut all = vec![ApiSource::Library];

            #[cfg(feature = "tidal")]
            all.push(ApiSource::Tidal);
            #[cfg(feature = "qobuz")]
            all.push(ApiSource::Qobuz);
            #[cfg(feature = "yt")]
            all.push(ApiSource::Yt);

            all
        });

        &ALL
    }
}

impl TryFrom<&String> for ApiSource {
    type Error = strum::ParseError;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        Self::from_str(value.as_str())
    }
}

impl TryFrom<String> for ApiSource {
    type Error = strum::ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(value.as_str())
    }
}

impl std::fmt::Display for ApiSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

#[derive(
    Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, Eq, PartialEq, Clone, Copy,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TrackApiSource {
    #[default]
    Local,
    #[cfg(feature = "tidal")]
    Tidal,
    #[cfg(feature = "qobuz")]
    Qobuz,
    #[cfg(feature = "yt")]
    Yt,
}

impl TrackApiSource {
    #[must_use]
    pub fn all() -> &'static [Self] {
        static ALL: LazyLock<Vec<TrackApiSource>> = LazyLock::new(|| {
            #[allow(unused_mut)]
            let mut all = vec![TrackApiSource::Local];

            #[cfg(feature = "tidal")]
            all.push(TrackApiSource::Tidal);
            #[cfg(feature = "qobuz")]
            all.push(TrackApiSource::Qobuz);
            #[cfg(feature = "yt")]
            all.push(TrackApiSource::Yt);

            all
        });

        &ALL
    }
}

impl std::fmt::Display for TrackApiSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl TryFrom<&String> for TrackApiSource {
    type Error = strum::ParseError;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        Self::from_str(value.as_str())
    }
}

impl TryFrom<String> for TrackApiSource {
    type Error = strum::ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(value.as_str())
    }
}

impl From<TrackApiSource> for ApiSource {
    fn from(value: TrackApiSource) -> Self {
        match value {
            TrackApiSource::Local => Self::Library,
            #[cfg(feature = "tidal")]
            TrackApiSource::Tidal => Self::Tidal,
            #[cfg(feature = "qobuz")]
            TrackApiSource::Qobuz => Self::Qobuz,
            #[cfg(feature = "yt")]
            TrackApiSource::Yt => Self::Yt,
        }
    }
}

impl From<AlbumSource> for TrackApiSource {
    fn from(value: AlbumSource) -> Self {
        match value {
            AlbumSource::Local => Self::Local,
            #[cfg(feature = "tidal")]
            AlbumSource::Tidal => Self::Tidal,
            #[cfg(feature = "qobuz")]
            AlbumSource::Qobuz => Self::Qobuz,
            #[cfg(feature = "yt")]
            AlbumSource::Yt => Self::Yt,
        }
    }
}

impl ToValueType<TrackApiSource> for &serde_json::Value {
    fn to_value_type(self) -> Result<TrackApiSource, ParseError> {
        TrackApiSource::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("TrackApiSource".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("TrackApiSource".into()))
    }
}

#[derive(Default, Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: Id,
    pub number: u32,
    pub title: String,
    pub duration: f64,
    pub album: String,
    pub album_id: Id,
    pub album_type: AlbumType,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub artist: String,
    pub artist_id: Id,
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
    pub track_source: TrackApiSource,
    pub api_source: ApiSource,
    pub sources: ApiSources,
}

#[derive(Default, Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
struct TrackInner {
    id: Id,
    number: u32,
    title: String,
    duration: f64,
    album: String,
    album_id: Id,
    album_type: AlbumType,
    date_released: Option<String>,
    date_added: Option<String>,
    artist: String,
    artist_id: Id,
    file: Option<String>,
    artwork: Option<String>,
    blur: bool,
    bytes: u64,
    format: Option<AudioFormat>,
    bit_depth: Option<u8>,
    audio_bitrate: Option<u32>,
    overall_bitrate: Option<u32>,
    sample_rate: Option<u32>,
    channels: Option<u8>,
    source: TrackApiSource,
    qobuz_id: Option<u64>,
    tidal_id: Option<u64>,
    yt_id: Option<String>,
}

impl<'de> Deserialize<'de> for Track {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(TrackInner::deserialize(deserializer)?.into())
    }
}

impl From<TrackInner> for Track {
    fn from(value: TrackInner) -> Self {
        Self {
            id: value.id,
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

impl Track {
    /// # Panics
    ///
    /// * If the parent file doesn't exist
    /// * If the parent file name cannot be converted to a `str`
    #[must_use]
    pub fn directory(&self) -> Option<String> {
        self.file
            .as_ref()
            .and_then(|f| PathBuf::from_str(f).ok())
            .map(|p| p.parent().unwrap().to_str().unwrap().to_string())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct AlbumVersionQuality {
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackApiSource,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiSourceMapping {
    pub source: ApiSource,
    pub id: Id,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiSources(Vec<ApiSourceMapping>);

impl ApiSources {
    pub fn add_source(&mut self, source: ApiSource, id: Id) {
        self.0.push(ApiSourceMapping { source, id });
    }

    pub fn remove_source(&mut self, source: ApiSource) {
        self.0.retain_mut(|x| x.source != source);
    }

    pub fn add_source_opt(&mut self, source: ApiSource, id: Option<Id>) {
        if let Some(id) = id {
            self.0.push(ApiSourceMapping { source, id });
        }
    }

    #[must_use]
    pub fn with_source(mut self, source: ApiSource, id: Id) -> Self {
        self.0.push(ApiSourceMapping { source, id });
        self
    }

    #[must_use]
    pub fn with_source_opt(mut self, source: ApiSource, id: Option<Id>) -> Self {
        if let Some(id) = id {
            self.0.push(ApiSourceMapping { source, id });
        }
        self
    }

    #[must_use]
    pub fn get(&self, source: ApiSource) -> Option<&Id> {
        self.deref().iter().find_map(|x| {
            if x.source == source {
                Some(&x.id)
            } else {
                None
            }
        })
    }
}

impl Deref for ApiSources {
    type Target = [ApiSourceMapping];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(
    Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Eq, Clone, Copy,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumType {
    #[default]
    Lp,
    Live,
    Compilations,
    EpsAndSingles,
    Other,
    Download,
}

impl std::fmt::Display for AlbumType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Album {
    pub id: Id,
    pub title: String,
    pub artist: String,
    pub artist_id: Id,
    pub album_type: AlbumType,
    pub date_released: Option<NaiveDateTime>,
    pub date_added: Option<NaiveDateTime>,
    pub artwork: Option<String>,
    pub directory: Option<String>,
    pub blur: bool,
    pub versions: Vec<AlbumVersionQuality>,
    pub album_source: AlbumSource,
    pub api_source: ApiSource,
    pub artist_sources: ApiSources,
    pub album_sources: ApiSources,
}

impl TryFrom<&Track> for Album {
    type Error = chrono::ParseError;

    fn try_from(value: &Track) -> Result<Self, Self::Error> {
        value.clone().try_into()
    }
}

impl TryFrom<Track> for Album {
    type Error = chrono::ParseError;

    fn try_from(value: Track) -> Result<Self, Self::Error> {
        Ok(Self {
            directory: value.directory(),
            id: value.album_id,
            title: value.album,
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
            artwork: value.artwork,
            blur: value.blur,
            versions: vec![],
            album_source: value.track_source.into(),
            api_source: value.api_source,
            artist_sources: value.sources.clone(),
            album_sources: value.sources,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Eq, PartialEq, Default, AsRefStr)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AlbumSource {
    #[default]
    Local,
    #[cfg(feature = "tidal")]
    Tidal,
    #[cfg(feature = "qobuz")]
    Qobuz,
    #[cfg(feature = "yt")]
    Yt,
}

impl From<AlbumSource> for ApiSource {
    fn from(value: AlbumSource) -> Self {
        match value {
            AlbumSource::Local => Self::Library,
            #[cfg(feature = "tidal")]
            AlbumSource::Tidal => Self::Tidal,
            #[cfg(feature = "qobuz")]
            AlbumSource::Qobuz => Self::Qobuz,
            #[cfg(feature = "yt")]
            AlbumSource::Yt => Self::Yt,
        }
    }
}

impl From<TrackApiSource> for AlbumSource {
    fn from(value: TrackApiSource) -> Self {
        match value {
            TrackApiSource::Local => Self::Local,
            #[cfg(feature = "tidal")]
            TrackApiSource::Tidal => Self::Tidal,
            #[cfg(feature = "qobuz")]
            TrackApiSource::Qobuz => Self::Qobuz,
            #[cfg(feature = "yt")]
            TrackApiSource::Yt => Self::Yt,
        }
    }
}

impl FromStr for AlbumSource {
    type Err = ();

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            #[cfg(feature = "tidal")]
            "tidal" => Ok(Self::Tidal),
            #[cfg(feature = "qobuz")]
            "qobuz" => Ok(Self::Qobuz),
            #[cfg(feature = "yt")]
            "yt" => Ok(Self::Yt),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum AlbumSort {
    ArtistAsc,
    ArtistDesc,
    NameAsc,
    NameDesc,
    ReleaseDateAsc,
    ReleaseDateDesc,
    DateAddedAsc,
    DateAddedDesc,
}

impl std::fmt::Display for AlbumSort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ArtistAsc => f.write_str("artist"),
            Self::ArtistDesc => f.write_str("artist-desc"),
            Self::NameAsc => f.write_str("name"),
            Self::NameDesc => f.write_str("name-desc"),
            Self::ReleaseDateAsc => f.write_str("release-date"),
            Self::ReleaseDateDesc => f.write_str("release-date-desc"),
            Self::DateAddedAsc => f.write_str("date-added"),
            Self::DateAddedDesc => f.write_str("date-added-desc"),
        }
    }
}

impl FromStr for AlbumSort {
    type Err = ();

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.to_lowercase().as_str() {
            "artist-asc" | "artist" => Ok(Self::ArtistAsc),
            "artist-desc" => Ok(Self::ArtistDesc),
            "name-asc" | "name" => Ok(Self::NameAsc),
            "name-desc" => Ok(Self::NameDesc),
            "release-date-asc" | "release-date" => Ok(Self::ReleaseDateAsc),
            "release-date-desc" => Ok(Self::ReleaseDateDesc),
            "date-added-asc" | "date-added" => Ok(Self::DateAddedAsc),
            "date-added-desc" => Ok(Self::DateAddedDesc),
            _ => Err(()),
        }
    }
}

#[derive(
    Copy, Debug, Clone, Serialize, Deserialize, EnumString, Default, AsRefStr, PartialEq, Eq,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum AudioFormat {
    #[cfg(feature = "aac")]
    Aac,
    #[cfg(feature = "flac")]
    Flac,
    #[cfg(feature = "mp3")]
    Mp3,
    #[cfg(feature = "opus")]
    Opus,
    #[default]
    Source,
}

impl std::fmt::Display for AudioFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

#[must_use]
pub fn from_extension_to_audio_format(extension: &str) -> Option<AudioFormat> {
    #[allow(unreachable_code)]
    Some(match extension.to_lowercase().as_str() {
        #[cfg(feature = "flac")]
        "flac" => AudioFormat::Flac,
        #[cfg(feature = "mp3")]
        "mp3" => AudioFormat::Mp3,
        #[cfg(feature = "opus")]
        "opus" => AudioFormat::Opus,
        #[cfg(feature = "aac")]
        "m4a" | "mp4" => AudioFormat::Aac,
        _ => return None,
    })
}

#[derive(Copy, Clone, Default, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackQuality {
    pub format: AudioFormat,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Default)]
pub struct TrackSize {
    pub id: u64,
    pub track_id: u64,
    pub bytes: Option<u64>,
    pub format: String,
}
