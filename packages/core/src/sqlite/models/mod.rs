use std::{
    fmt::{Display, Formatter},
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};

use async_trait::async_trait;
use moosicbox_database::{Database, DatabaseValue};
use moosicbox_json_utils::{database::ToValue as _, MissingValue, ParseError, ToValueType};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

use crate::types::AudioFormat;

use super::db::{
    get_album_version_qualities, get_players, get_session_active_players, get_session_playlist,
    get_session_playlist_tracks, get_tracks, DbError,
};

pub mod qobuz;

pub trait AsModel<T> {
    fn as_model(&self) -> T;
}

pub trait AsModelResult<T, E> {
    fn as_model(&self) -> Result<T, E>;
}

pub trait AsModelResultMapped<T, E> {
    fn as_model_mapped(&self) -> Result<Vec<T>, E>;
}

pub trait AsModelResultMappedMut<T, E> {
    fn as_model_mapped_mut(&mut self) -> Result<Vec<T>, E>;
}

#[async_trait]
pub trait AsModelResultMappedQuery<T, E> {
    async fn as_model_mapped_query(&self, db: &dyn Database) -> Result<Vec<T>, E>;
}

pub trait AsModelResultMut<T, E> {
    fn as_model_mut<'a>(&'a mut self) -> Result<Vec<T>, E>
    where
        for<'b> &'b moosicbox_database::Row: ToValueType<T>;
}

impl<T, E> AsModelResultMut<T, E> for Vec<moosicbox_database::Row>
where
    E: From<DbError>,
{
    fn as_model_mut<'a>(&'a mut self) -> Result<Vec<T>, E>
    where
        for<'b> &'b moosicbox_database::Row: ToValueType<T>,
    {
        let mut values = vec![];

        for row in self {
            match row.to_value_type() {
                Ok(value) => values.push(value),
                Err(err) => {
                    if log::log_enabled!(log::Level::Debug) {
                        log::error!("Row error: {err:?} ({row:?})");
                    } else {
                        log::error!("Row error: {err:?}");
                    }
                }
            }
        }

        Ok(values)
    }
}

pub trait AsId {
    fn as_id(&self) -> DatabaseValue;
}

#[async_trait]
pub trait AsModelQuery<T> {
    async fn as_model_query(&self, db: &dyn Database) -> Result<T, DbError>;
}

pub trait ToApi<T> {
    fn to_api(self) -> T;
}

impl<T, X> ToApi<T> for Arc<X>
where
    X: ToApi<T> + Clone,
{
    fn to_api(self) -> T {
        self.as_ref().clone().to_api()
    }
}

impl<'a, T, X> ToApi<T> for &'a X
where
    X: ToApi<T> + Clone,
{
    fn to_api(self) -> T {
        self.clone().to_api()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct NumberId {
    pub id: i32,
}

impl AsModel<NumberId> for &moosicbox_database::Row {
    fn as_model(&self) -> NumberId {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<NumberId, ParseError> for &moosicbox_database::Row {
    fn as_model(&self) -> Result<NumberId, ParseError> {
        Ok(NumberId {
            id: self.to_value("id")?,
        })
    }
}

impl AsId for NumberId {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct StringId {
    pub id: String,
}

impl AsModel<StringId> for &moosicbox_database::Row {
    fn as_model(&self) -> StringId {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<StringId, ParseError> for &moosicbox_database::Row {
    fn as_model(&self) -> Result<StringId, ParseError> {
        Ok(StringId {
            id: self.to_value("id")?,
        })
    }
}

impl AsId for StringId {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::String(self.id.clone())
    }
}

#[derive(
    Default, Debug, Serialize, Deserialize, EnumString, AsRefStr, Eq, PartialEq, Clone, Copy,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackApiSource {
    #[default]
    Local,
    Tidal,
    Qobuz,
    Yt,
}

impl From<AlbumSource> for TrackApiSource {
    fn from(value: AlbumSource) -> Self {
        match value {
            AlbumSource::Local => Self::Local,
            AlbumSource::Tidal => Self::Tidal,
            AlbumSource::Qobuz => Self::Qobuz,
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

impl MissingValue<TrackApiSource> for &moosicbox_database::Row {}
impl ToValueType<TrackApiSource> for rusqlite::types::Value {
    fn to_value_type(self) -> Result<TrackApiSource, ParseError> {
        match self {
            rusqlite::types::Value::Text(str) => Ok(TrackApiSource::from_str(&str)
                .map_err(|_| ParseError::ConvertType("TrackApiSource".into()))?),
            _ => Err(ParseError::ConvertType("TrackApiSource".into())),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: Id,
    pub number: i32,
    pub title: String,
    pub duration: f64,
    pub album: String,
    pub album_id: Id,
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
    pub source: TrackApiSource,
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
            source: value.source,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct LibraryTrack {
    pub id: i32,
    pub number: i32,
    pub title: String,
    pub duration: f64,
    pub album: String,
    pub album_id: i32,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub artist: String,
    pub artist_id: i32,
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
    pub qobuz_id: Option<u64>,
    pub tidal_id: Option<u64>,
    pub yt_id: Option<u64>,
}

impl LibraryTrack {
    pub fn directory(&self) -> Option<String> {
        self.file
            .as_ref()
            .and_then(|f| PathBuf::from_str(f).ok())
            .map(|p| p.parent().unwrap().to_str().unwrap().to_string())
    }
}

impl AsModel<LibraryTrack> for &moosicbox_database::Row {
    fn as_model(&self) -> LibraryTrack {
        AsModelResult::as_model(self).unwrap()
    }
}

impl ToValueType<LibraryTrack> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<LibraryTrack, ParseError> {
        Ok(LibraryTrack {
            id: self.to_value("id")?,
            number: self.to_value("number")?,
            title: self.to_value("title")?,
            duration: self.to_value("duration")?,
            album: self.to_value("album").unwrap_or_default(),
            album_id: self.to_value("album_id")?,
            date_released: self.to_value("date_released").unwrap_or_default(),
            date_added: self.to_value("date_added").unwrap_or_default(),
            artist: self.to_value("artist").unwrap_or_default(),
            artist_id: self.to_value("artist_id").unwrap_or_default(),
            file: self.to_value("file")?,
            artwork: self.to_value("artwork").unwrap_or_default(),
            blur: self.to_value("blur").unwrap_or_default(),
            bytes: self.to_value("bytes").unwrap_or_default(),
            format: self
                .to_value::<Option<String>>("format")
                .unwrap_or(None)
                .map(|s| {
                    AudioFormat::from_str(&s)
                        .map_err(|_e| ParseError::ConvertType(format!("Invalid format: {s}")))
                })
                .transpose()?,
            bit_depth: self.to_value("bit_depth").unwrap_or_default(),
            audio_bitrate: self.to_value("audio_bitrate").unwrap_or_default(),
            overall_bitrate: self.to_value("overall_bitrate").unwrap_or_default(),
            sample_rate: self.to_value("sample_rate").unwrap_or_default(),
            channels: self.to_value("channels").unwrap_or_default(),
            source: TrackApiSource::from_str(&self.to_value::<String>("source")?)
                .expect("Missing source"),
            qobuz_id: self.to_value("qobuz_id")?,
            tidal_id: self.to_value("tidal_id")?,
            yt_id: self.to_value("yt_id")?,
        })
    }
}

impl AsModelResult<LibraryTrack, ParseError> for &moosicbox_database::Row {
    fn as_model(&self) -> Result<LibraryTrack, ParseError> {
        Ok(LibraryTrack {
            id: self.to_value("id")?,
            number: self.to_value("number")?,
            title: self.to_value("title")?,
            duration: self.to_value("duration")?,
            album: self.to_value("album").unwrap_or_default(),
            album_id: self.to_value("album_id")?,
            date_released: self.to_value("date_released").unwrap_or_default(),
            date_added: self.to_value("date_added").unwrap_or_default(),
            artist: self.to_value("artist").unwrap_or_default(),
            artist_id: self.to_value("artist_id").unwrap_or_default(),
            file: self.to_value("file")?,
            artwork: self.to_value("artwork").unwrap_or_default(),
            blur: self.to_value("blur").unwrap_or_default(),
            bytes: self.to_value("bytes").unwrap_or_default(),
            format: self
                .to_value::<Option<String>>("format")
                .unwrap_or(None)
                .map(|s| {
                    AudioFormat::from_str(&s)
                        .map_err(|_e| ParseError::ConvertType(format!("Invalid format: {s}")))
                })
                .transpose()?,
            bit_depth: self.to_value("bit_depth").unwrap_or_default(),
            audio_bitrate: self.to_value("audio_bitrate").unwrap_or_default(),
            overall_bitrate: self.to_value("overall_bitrate").unwrap_or_default(),
            sample_rate: self.to_value("sample_rate").unwrap_or_default(),
            channels: self.to_value("channels").unwrap_or_default(),
            source: TrackApiSource::from_str(&self.to_value::<String>("source")?)
                .expect("Missing source"),
            qobuz_id: self.to_value("qobuz_id")?,
            tidal_id: self.to_value("tidal_id")?,
            yt_id: self.to_value("yt_id")?,
        })
    }
}

impl AsId for LibraryTrack {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
enum ApiTrackInner {
    Library(ApiLibraryTrack),
    Tidal(serde_json::Value),
    Qobuz(serde_json::Value),
    Yt(serde_json::Value),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ApiTrack {
    Library {
        track_id: u64,
        data: ApiLibraryTrack,
    },
    Tidal {
        track_id: u64,
        data: serde_json::Value,
    },
    Qobuz {
        track_id: u64,
        data: serde_json::Value,
    },
    Yt {
        track_id: String,
        data: serde_json::Value,
    },
}

impl Serialize for ApiTrack {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            ApiTrack::Library { data, .. } => {
                ApiTrackInner::Library(data.clone()).serialize(serializer)
            }
            ApiTrack::Tidal { data, .. } => {
                ApiTrackInner::Tidal(data.clone()).serialize(serializer)
            }
            ApiTrack::Qobuz { data, .. } => {
                ApiTrackInner::Qobuz(data.clone()).serialize(serializer)
            }
            ApiTrack::Yt { data, .. } => ApiTrackInner::Yt(data.clone()).serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ApiTrack {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(match ApiTrackInner::deserialize(deserializer)? {
            ApiTrackInner::Library(track) => ApiTrack::Library {
                track_id: track.track_id.try_into().unwrap(),
                data: track,
            },
            ApiTrackInner::Tidal(data) => ApiTrack::Tidal {
                track_id: data
                    .get("id")
                    .expect("Failed to get tidal track id")
                    .as_u64()
                    .unwrap(),
                data,
            },
            ApiTrackInner::Qobuz(data) => ApiTrack::Qobuz {
                track_id: data
                    .get("id")
                    .expect("Failed to get qobuz track id")
                    .as_u64()
                    .unwrap(),
                data,
            },
            ApiTrackInner::Yt(data) => ApiTrack::Yt {
                track_id: data
                    .get("id")
                    .expect("Failed to get yt track id")
                    .as_str()
                    .unwrap()
                    .to_string(),
                data,
            },
        })
    }
}

impl ApiTrack {
    pub fn api_source(&self) -> ApiSource {
        match self {
            ApiTrack::Library { .. } => ApiSource::Library,
            ApiTrack::Tidal { .. } => ApiSource::Tidal,
            ApiTrack::Qobuz { .. } => ApiSource::Qobuz,
            ApiTrack::Yt { .. } => ApiSource::Yt,
        }
    }

    pub fn data(&self) -> serde_json::Value {
        match self {
            ApiTrack::Library { data, .. } => serde_json::to_value(data).unwrap(),
            ApiTrack::Tidal { data, .. } => data.clone(),
            ApiTrack::Qobuz { data, .. } => data.clone(),
            ApiTrack::Yt { data, .. } => data.clone(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiLibraryTrack {
    pub track_id: i32,
    pub number: i32,
    pub title: String,
    pub duration: f64,
    pub artist: String,
    pub artist_id: i32,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub album: String,
    pub album_id: i32,
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
}

impl ToApi<ApiTrack> for LibraryTrack {
    fn to_api(self) -> ApiTrack {
        ApiTrack::Library {
            track_id: self.id as u64,
            data: ApiLibraryTrack {
                track_id: self.id,
                number: self.number,
                title: self.title.clone(),
                duration: self.duration,
                artist: self.artist.clone(),
                artist_id: self.artist_id,
                date_released: self.date_released.clone(),
                date_added: self.date_added.clone(),
                album: self.album.clone(),
                album_id: self.album_id,
                contains_cover: self.artwork.is_some(),
                blur: self.blur,
                bytes: self.bytes,
                format: self.format,
                bit_depth: self.bit_depth,
                audio_bitrate: self.audio_bitrate,
                overall_bitrate: self.overall_bitrate,
                sample_rate: self.sample_rate,
                channels: self.channels,
                source: self.source,
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Artist {
    pub id: Id,
    pub title: String,
    pub cover: Option<String>,
}

impl From<LibraryArtist> for Artist {
    fn from(value: LibraryArtist) -> Self {
        Self {
            id: value.id.into(),
            title: value.title,
            cover: value.cover,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct LibraryArtist {
    pub id: i32,
    pub title: String,
    pub cover: Option<String>,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<u64>,
    pub yt_id: Option<u64>,
}

impl AsModel<LibraryArtist> for &moosicbox_database::Row {
    fn as_model(&self) -> LibraryArtist {
        AsModelResult::as_model(self).unwrap()
    }
}

impl ToValueType<LibraryArtist> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<LibraryArtist, ParseError> {
        Ok(LibraryArtist {
            id: self.to_value("id")?,
            title: self.to_value("title")?,
            cover: self.to_value("cover")?,
            tidal_id: self.to_value("tidal_id")?,
            qobuz_id: self.to_value("qobuz_id")?,
            yt_id: self.to_value("yt_id")?,
        })
    }
}

impl AsModelResult<LibraryArtist, ParseError> for &moosicbox_database::Row {
    fn as_model(&self) -> Result<LibraryArtist, ParseError> {
        Ok(LibraryArtist {
            id: self.to_value("id")?,
            title: self.to_value("title")?,
            cover: self.to_value("cover")?,
            tidal_id: self.to_value("tidal_id")?,
            qobuz_id: self.to_value("qobuz_id")?,
            yt_id: self.to_value("yt_id")?,
        })
    }
}

impl AsId for LibraryArtist {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum ArtistSort {
    NameAsc,
    NameDesc,
}

impl FromStr for ArtistSort {
    type Err = ();

    fn from_str(input: &str) -> Result<ArtistSort, Self::Err> {
        match input.to_lowercase().as_str() {
            "name-asc" | "name" => Ok(ArtistSort::NameAsc),
            "name-desc" => Ok(ArtistSort::NameDesc),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiLibraryArtist {
    pub artist_id: i32,
    pub title: String,
    pub contains_cover: bool,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<u64>,
    pub yt_id: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiArtist {
    Library(ApiLibraryArtist),
}

impl ToApi<ApiArtist> for LibraryArtist {
    fn to_api(self) -> ApiArtist {
        ApiArtist::Library(ApiLibraryArtist {
            artist_id: self.id,
            title: self.title.clone(),
            contains_cover: self.cover.is_some(),
            tidal_id: self.tidal_id,
            qobuz_id: self.qobuz_id,
            yt_id: self.yt_id,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct AlbumVersionQuality {
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackApiSource,
}

impl ToApi<ApiAlbumVersionQuality> for AlbumVersionQuality {
    fn to_api(self) -> ApiAlbumVersionQuality {
        ApiAlbumVersionQuality {
            format: self.format,
            bit_depth: self.bit_depth,
            sample_rate: self.sample_rate,
            channels: self.channels,
            source: self.source,
        }
    }
}

impl AsModel<AlbumVersionQuality> for &moosicbox_database::Row {
    fn as_model(&self) -> AlbumVersionQuality {
        AsModelResult::as_model(self).unwrap()
    }
}

impl MissingValue<AlbumVersionQuality> for &moosicbox_database::Row {}
impl ToValueType<AlbumVersionQuality> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<AlbumVersionQuality, ParseError> {
        Ok(AlbumVersionQuality {
            format: self
                .to_value::<Option<String>>("format")
                .unwrap_or(None)
                .map(|s| {
                    AudioFormat::from_str(&s)
                        .map_err(|_e| ParseError::ConvertType(format!("Invalid format: {s}")))
                })
                .transpose()?,
            bit_depth: self.to_value("bit_depth").unwrap_or_default(),
            sample_rate: self.to_value("sample_rate")?,
            channels: self.to_value("channels")?,
            source: TrackApiSource::from_str(&self.to_value::<String>("source")?)
                .map_err(|e| ParseError::ConvertType(format!("Invalid source: {e:?}")))?,
        })
    }
}

impl AsModelResult<AlbumVersionQuality, ParseError> for &moosicbox_database::Row {
    fn as_model(&self) -> Result<AlbumVersionQuality, ParseError> {
        Ok(AlbumVersionQuality {
            format: self
                .to_value::<Option<String>>("format")
                .unwrap_or(None)
                .map(|s| {
                    AudioFormat::from_str(&s)
                        .map_err(|_e| ParseError::ConvertType(format!("Invalid format: {s}")))
                })
                .transpose()?,
            bit_depth: self.to_value("bit_depth").unwrap_or_default(),
            sample_rate: self.to_value("sample_rate")?,
            channels: self.to_value("channels")?,
            source: TrackApiSource::from_str(&self.to_value::<String>("source")?)
                .map_err(|e| ParseError::ConvertType(format!("Invalid source: {e:?}")))?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Album {
    pub id: Id,
    pub title: String,
    pub artist: String,
    pub artist_id: Id,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub artwork: Option<String>,
    pub directory: Option<String>,
    pub blur: bool,
    pub versions: Vec<AlbumVersionQuality>,
}

impl From<LibraryAlbum> for Album {
    fn from(value: LibraryAlbum) -> Self {
        Self {
            id: value.id.into(),
            title: value.title,
            artist: value.artist,
            artist_id: value.artist_id.into(),
            date_released: value.date_released,
            date_added: value.date_added,
            artwork: value.artwork,
            directory: value.directory,
            blur: value.blur,
            versions: value.versions,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct LibraryAlbum {
    pub id: i32,
    pub title: String,
    pub artist: String,
    pub artist_id: i32,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub artwork: Option<String>,
    pub directory: Option<String>,
    pub source: AlbumSource,
    pub blur: bool,
    pub versions: Vec<AlbumVersionQuality>,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<String>,
    pub yt_id: Option<u64>,
    pub tidal_artist_id: Option<u64>,
    pub qobuz_artist_id: Option<u64>,
    pub yt_artist_id: Option<u64>,
}

impl AsModel<LibraryAlbum> for &moosicbox_database::Row {
    fn as_model(&self) -> LibraryAlbum {
        AsModelResult::as_model(self).unwrap()
    }
}

impl MissingValue<LibraryAlbum> for &moosicbox_database::Row {}
impl ToValueType<LibraryAlbum> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<LibraryAlbum, ParseError> {
        Ok(LibraryAlbum {
            id: self.to_value("id")?,
            artist: self.to_value("artist").unwrap_or_default(),
            artist_id: self.to_value("artist_id")?,
            title: self.to_value("title")?,
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            artwork: self.to_value("artwork")?,
            directory: self.to_value("directory")?,
            source: AlbumSource::Local,
            blur: self.to_value("blur")?,
            versions: vec![],
            tidal_id: self.to_value("tidal_id")?,
            qobuz_id: self.to_value("qobuz_id")?,
            yt_id: self.to_value("yt_id")?,
            tidal_artist_id: self.to_value("tidal_artist_id")?,
            qobuz_artist_id: self.to_value("qobuz_artist_id")?,
            yt_artist_id: self.to_value("yt_artist_id")?,
        })
    }
}

impl AsModelResult<LibraryAlbum, ParseError> for &moosicbox_database::Row {
    fn as_model(&self) -> Result<LibraryAlbum, ParseError> {
        Ok(LibraryAlbum {
            id: self.to_value("id")?,
            artist: self.to_value("artist").unwrap_or_default(),
            artist_id: self.to_value("artist_id")?,
            title: self.to_value("title")?,
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            artwork: self.to_value("artwork")?,
            directory: self.to_value("directory")?,
            source: AlbumSource::Local,
            blur: self.to_value("blur")?,
            versions: vec![],
            tidal_id: self.to_value("tidal_id")?,
            qobuz_id: self.to_value("qobuz_id")?,
            yt_id: self.to_value("yt_id")?,
            tidal_artist_id: self.to_value("tidal_artist_id")?,
            qobuz_artist_id: self.to_value("qobuz_artist_id")?,
            yt_artist_id: self.to_value("yt_artist_id")?,
        })
    }
}

pub fn track_source_to_u8(source: TrackApiSource) -> u8 {
    match source {
        TrackApiSource::Local => 1,
        TrackApiSource::Tidal => 2,
        TrackApiSource::Qobuz => 3,
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

impl AsModelResultMapped<LibraryAlbum, DbError> for Vec<moosicbox_database::Row> {
    fn as_model_mapped(&self) -> Result<Vec<LibraryAlbum>, DbError> {
        let mut results: Vec<LibraryAlbum> = vec![];
        let mut last_album_id = 0;

        for row in self {
            let album_id: i32 = row
                .get("album_id")
                .ok_or(DbError::InvalidRequest)?
                .try_into()
                .map_err(|_| DbError::InvalidRequest)?;

            if album_id != last_album_id {
                if let Some(ref mut album) = results.last_mut() {
                    log::trace!(
                        "Sorting versions for album id={} count={}",
                        album.id,
                        album.versions.len()
                    );
                    sort_album_versions(&mut album.versions);
                }
                match row.to_value_type() {
                    Ok(album) => {
                        results.push(album);
                    }
                    Err(err) => {
                        log::error!("Failed to parse Album for album id={}: {err:?}", album_id);
                        continue;
                    }
                }
                last_album_id = album_id;
            }

            if let Some(album) = results.last_mut() {
                if let Some(_source) = row.get("source") {
                    match row.to_value_type() {
                        Ok(version) => {
                            album.versions.push(version);
                            log::trace!(
                                "Added version to album id={} count={}",
                                album.id,
                                album.versions.len()
                            );
                        }
                        Err(err) => {
                            log::error!(
                                "Failed to parse AlbumVersionQuality for album id={}: {err:?}",
                                album.id
                            );
                        }
                    }
                } else {
                    if album.tidal_id.is_some() {
                        album.versions.push(AlbumVersionQuality {
                            format: None,
                            bit_depth: None,
                            sample_rate: None,
                            channels: None,
                            source: TrackApiSource::Tidal,
                        });
                        log::trace!(
                            "Added Tidal version to album id={} count={}",
                            album.id,
                            album.versions.len()
                        );
                    }
                    if album.qobuz_id.is_some() {
                        album.versions.push(AlbumVersionQuality {
                            format: None,
                            bit_depth: None,
                            sample_rate: None,
                            channels: None,
                            source: TrackApiSource::Qobuz,
                        });
                        log::trace!(
                            "Added Qobuz version to album id={} count={}",
                            album.id,
                            album.versions.len()
                        );
                    }
                    if album.yt_id.is_some() {
                        album.versions.push(AlbumVersionQuality {
                            format: None,
                            bit_depth: None,
                            sample_rate: None,
                            channels: None,
                            source: TrackApiSource::Yt,
                        });
                        log::trace!(
                            "Added Yt version to album id={} count={}",
                            album.id,
                            album.versions.len()
                        );
                    }
                }
            }
        }

        if let Some(ref mut album) = results.last_mut() {
            log::trace!(
                "Sorting versions for last album id={} count={}",
                album.id,
                album.versions.len()
            );
            sort_album_versions(&mut album.versions);
        }

        Ok(results)
    }
}

#[async_trait]
impl AsModelQuery<LibraryAlbum> for &moosicbox_database::Row {
    async fn as_model_query(&self, db: &dyn Database) -> Result<LibraryAlbum, DbError> {
        let id = self.to_value("id")?;

        Ok(LibraryAlbum {
            id,
            artist: self
                .to_value::<Option<String>>("artist")?
                .unwrap_or_default(),
            artist_id: self.to_value("artist_id")?,
            title: self.to_value("title")?,
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            artwork: self.to_value("artwork")?,
            directory: self.to_value("directory")?,
            source: AlbumSource::Local,
            blur: self.to_value("blur")?,
            versions: get_album_version_qualities(db, id).await?,
            tidal_id: self.to_value("tidal_id")?,
            qobuz_id: self.to_value("qobuz_id")?,
            yt_id: self.to_value("yt_id")?,
            tidal_artist_id: self.to_value("tidal_artist_id")?,
            qobuz_artist_id: self.to_value("qobuz_artist_id")?,
            yt_artist_id: self.to_value("yt_artist_id")?,
        })
    }
}

impl AsId for LibraryAlbum {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiAlbum {
    Library(ApiLibraryAlbum),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiAlbumVersionQuality {
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackApiSource,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiLibraryAlbum {
    pub album_id: i32,
    pub title: String,
    pub artist: String,
    pub artist_id: i32,
    pub contains_cover: bool,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub source: AlbumSource,
    pub blur: bool,
    pub versions: Vec<ApiAlbumVersionQuality>,
    pub tidal_id: Option<u64>,
    pub qobuz_id: Option<String>,
    pub yt_id: Option<u64>,
}

impl ToApi<ApiAlbum> for LibraryAlbum {
    fn to_api(self) -> ApiAlbum {
        ApiAlbum::Library(ApiLibraryAlbum {
            album_id: self.id,
            title: self.title,
            artist: self.artist,
            artist_id: self.artist_id,
            contains_cover: self.artwork.is_some(),
            date_released: self.date_released,
            date_added: self.date_added,
            source: self.source,
            blur: self.blur,
            versions: self.versions.iter().map(|v| v.to_api()).collect(),
            tidal_id: self.tidal_id,
            qobuz_id: self.qobuz_id,
            yt_id: self.yt_id,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Eq, PartialEq, Default)]
pub enum AlbumSource {
    #[default]
    Local,
    Tidal,
    Qobuz,
    Yt,
}

impl From<TrackApiSource> for AlbumSource {
    fn from(value: TrackApiSource) -> Self {
        match value {
            TrackApiSource::Local => Self::Local,
            TrackApiSource::Tidal => Self::Tidal,
            TrackApiSource::Qobuz => Self::Qobuz,
            TrackApiSource::Yt => Self::Yt,
        }
    }
}

impl FromStr for AlbumSource {
    type Err = ();

    fn from_str(input: &str) -> Result<AlbumSource, Self::Err> {
        match input.to_lowercase().as_str() {
            "local" => Ok(AlbumSource::Local),
            "tidal" => Ok(AlbumSource::Tidal),
            "qobuz" => Ok(AlbumSource::Qobuz),
            "yt" => Ok(AlbumSource::Yt),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
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

impl FromStr for AlbumSort {
    type Err = ();

    fn from_str(input: &str) -> Result<AlbumSort, Self::Err> {
        match input.to_lowercase().as_str() {
            "artist-asc" | "artist" => Ok(AlbumSort::ArtistAsc),
            "artist-desc" => Ok(AlbumSort::ArtistDesc),
            "name-asc" | "name" => Ok(AlbumSort::NameAsc),
            "name-desc" => Ok(AlbumSort::NameDesc),
            "release-date-asc" | "release-date" => Ok(AlbumSort::ReleaseDateAsc),
            "release-date-desc" => Ok(AlbumSort::ReleaseDateDesc),
            "date-added-asc" | "date-added" => Ok(AlbumSort::DateAddedAsc),
            "date-added-desc" => Ok(AlbumSort::DateAddedDesc),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SetSessionActivePlayers {
    pub session_id: i32,
    pub players: Vec<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreateSession {
    pub name: String,
    pub active_players: Vec<i32>,
    pub playlist: CreateSessionPlaylist,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct CreateSessionPlaylist {
    pub tracks: Vec<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSession {
    pub session_id: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seek: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist: Option<UpdateSessionPlaylist>,
}

impl UpdateSession {
    pub fn playback_updated(&self) -> bool {
        self.play.is_some()
            || self.stop.is_some()
            || self.active.is_some()
            || self.playing.is_some()
            || self.position.is_some()
            || self.volume.is_some()
            || self.seek.is_some()
            || self.playlist.is_some()
    }
}

impl ToApi<ApiUpdateSession> for UpdateSession {
    fn to_api(self) -> ApiUpdateSession {
        ApiUpdateSession {
            session_id: self.session_id,
            play: self.play,
            stop: self.stop,
            name: self.name,
            active: self.active,
            playing: self.playing,
            position: self.position,
            seek: self.seek,
            volume: self.volume,
            playlist: self.playlist.as_ref().map(|p| p.to_api()),
        }
    }
}

#[derive(Copy, Debug, Serialize, Deserialize, EnumString, AsRefStr, Eq, PartialEq, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ApiSource {
    Library,
    Tidal,
    Qobuz,
    Yt,
}

impl Display for ApiSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl MissingValue<ApiSource> for &moosicbox_database::Row {}
impl ToValueType<ApiSource> for DatabaseValue {
    fn to_value_type(self) -> Result<ApiSource, ParseError> {
        ApiSource::from_str(
            self.as_str()
                .ok_or_else(|| ParseError::ConvertType("ApiSource".into()))?,
        )
        .map_err(|_| ParseError::ConvertType("ApiSource".into()))
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionPlaylist {
    pub session_playlist_id: i32,
    pub tracks: Vec<UpdateSessionPlaylistTrack>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSessionPlaylistTrack {
    pub id: String,
    pub r#type: ApiSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl From<UpdateSessionPlaylistTrack> for SessionPlaylistTrack {
    fn from(value: UpdateSessionPlaylistTrack) -> Self {
        SessionPlaylistTrack {
            id: value.id,
            r#type: value.r#type,
            data: value.data,
        }
    }
}

impl ToApi<ApiTrack> for SessionPlaylistTrack {
    fn to_api(self) -> ApiTrack {
        match self.r#type {
            ApiSource::Library => {
                let id = self.id.parse::<u64>().expect("Invalid Library Track ID");
                ApiTrack::Library {
                    track_id: id,
                    data: ApiLibraryTrack {
                        track_id: id as i32,
                        ..Default::default()
                    },
                }
            }
            ApiSource::Tidal => {
                let id = self.id.parse::<u64>().expect("Invalid Tidal Track ID");
                match &self.data {
                    Some(data) => ApiTrack::Tidal {
                        track_id: id,
                        data: serde_json::from_str(data)
                            .expect("Failed to parse UpdateSessionPlaylistTrack data"),
                    },
                    None => ApiTrack::Tidal {
                        track_id: id,
                        data: serde_json::json!({
                            "id": id,
                            "type": self.r#type,
                        }),
                    },
                }
            }
            ApiSource::Qobuz => {
                let id = self.id.parse::<u64>().expect("Invalid Qobuz Track ID");
                match &self.data {
                    Some(data) => ApiTrack::Qobuz {
                        track_id: id,
                        data: serde_json::from_str(data)
                            .expect("Failed to parse UpdateSessionPlaylistTrack data"),
                    },
                    None => ApiTrack::Qobuz {
                        track_id: id,
                        data: serde_json::json!({
                            "id": id,
                            "type": self.r#type,
                        }),
                    },
                }
            }
            ApiSource::Yt => match &self.data {
                Some(data) => ApiTrack::Yt {
                    track_id: self.id,
                    data: serde_json::from_str(data)
                        .expect("Failed to parse UpdateSessionPlaylistTrack data"),
                },
                None => ApiTrack::Yt {
                    track_id: self.id.clone(),
                    data: serde_json::json!({
                        "id": self.id,
                        "type": self.r#type,
                    }),
                },
            },
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiUpdateSessionPlaylistTrack {
    pub id: String,
    pub r#type: ApiSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl ToApi<ApiUpdateSessionPlaylistTrack> for UpdateSessionPlaylistTrack {
    fn to_api(self) -> ApiUpdateSessionPlaylistTrack {
        ApiUpdateSessionPlaylistTrack {
            id: self.id,
            r#type: self.r#type,
            data: self.data,
        }
    }
}

impl ToApi<ApiUpdateSessionPlaylist> for UpdateSessionPlaylist {
    fn to_api(self) -> ApiUpdateSessionPlaylist {
        ApiUpdateSessionPlaylist {
            session_playlist_id: self.session_playlist_id,
            tracks: self
                .tracks
                .into_iter()
                .map(From::<UpdateSessionPlaylistTrack>::from)
                .map(|track: SessionPlaylistTrack| track.to_api())
                .collect(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiUpdateSession {
    pub session_id: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub play: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seek: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlist: Option<ApiUpdateSessionPlaylist>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiUpdateSessionPlaylist {
    pub session_playlist_id: i32,
    pub tracks: Vec<ApiTrack>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSession {
    pub session_id: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: i32,
    pub name: String,
    pub active: bool,
    pub playing: bool,
    pub position: Option<i32>,
    pub seek: Option<i32>,
    pub volume: Option<f64>,
    pub active_players: Vec<Player>,
    pub playlist: SessionPlaylist,
}

impl ToValueType<Session> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<Session, ParseError> {
        Ok(Session {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
            active: self.to_value("active")?,
            playing: self.to_value("playing")?,
            position: self.to_value("position")?,
            seek: self.to_value("seek")?,
            volume: self.to_value("volume")?,
            ..Default::default()
        })
    }
}

#[async_trait]
impl AsModelQuery<Session> for &moosicbox_database::Row {
    async fn as_model_query(&self, db: &dyn Database) -> Result<Session, DbError> {
        let id = self.to_value("id")?;
        match get_session_playlist(db, id).await? {
            Some(playlist) => Ok(Session {
                id,
                name: self.to_value("name")?,
                active: self.to_value("active")?,
                playing: self.to_value("playing")?,
                position: self.to_value("position")?,
                seek: self.to_value("seek")?,
                volume: self.to_value("volume")?,
                active_players: get_session_active_players(db, id).await?,
                playlist,
            }),
            None => Err(DbError::InvalidRequest),
        }
    }
}

impl AsId for Session {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiSession {
    pub session_id: i32,
    pub name: String,
    pub active: bool,
    pub playing: bool,
    pub position: Option<i32>,
    pub seek: Option<i32>,
    pub volume: Option<f64>,
    pub active_players: Vec<ApiPlayer>,
    pub playlist: ApiSessionPlaylist,
}

impl ToApi<ApiSession> for Session {
    fn to_api(self) -> ApiSession {
        ApiSession {
            session_id: self.id,
            name: self.name,
            active: self.active,
            playing: self.playing,
            position: self.position,
            seek: self.seek,
            volume: self.volume,
            active_players: self.active_players.iter().map(|p| p.to_api()).collect(),
            playlist: self.playlist.to_api(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SessionPlaylist {
    pub id: i32,
    pub tracks: Vec<ApiTrack>,
}

impl ToValueType<SessionPlaylist> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<SessionPlaylist, ParseError> {
        Ok(SessionPlaylist {
            id: self.to_value("id")?,
            ..Default::default()
        })
    }
}

#[async_trait]
impl AsModelResultMappedQuery<ApiTrack, DbError> for Vec<SessionPlaylistTrack> {
    async fn as_model_mapped_query(&self, db: &dyn Database) -> Result<Vec<ApiTrack>, DbError> {
        let tracks = self;
        log::trace!("Mapping tracks to ApiTracks: {tracks:?}");

        let library_track_ids = tracks
            .iter()
            .filter(|t| t.r#type == ApiSource::Library)
            .filter_map(|t| t.id.parse::<u64>().ok())
            .collect::<Vec<_>>();

        log::trace!("Fetching tracks by ids: {library_track_ids:?}");
        let library_tracks = get_tracks(db, Some(&library_track_ids)).await?;

        Ok(tracks
            .iter()
            .map(|t| {
                Ok(match t.r#type {
                    ApiSource::Library => library_tracks
                        .iter()
                        .find(|lib| lib.id.to_string() == t.id)
                        .ok_or(DbError::Unknown)?
                        .to_api(),
                    ApiSource::Tidal => t.to_api(),
                    ApiSource::Qobuz => t.to_api(),
                    ApiSource::Yt => t.to_api(),
                })
            })
            .collect::<Result<Vec<_>, DbError>>()?)
    }
}

#[async_trait]
impl AsModelQuery<SessionPlaylist> for &moosicbox_database::Row {
    async fn as_model_query(&self, db: &dyn Database) -> Result<SessionPlaylist, DbError> {
        let id = self.to_value("id")?;
        let tracks = get_session_playlist_tracks(db, id)
            .await?
            .as_model_mapped_query(db)
            .await?;
        log::trace!("Got SessionPlaylistTracks for session_playlist {id}: {tracks:?}");

        Ok(SessionPlaylist { id, tracks })
    }
}

impl AsId for SessionPlaylist {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionPlaylistTrack {
    pub id: String,
    pub r#type: ApiSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl ToValueType<SessionPlaylistTrack> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<SessionPlaylistTrack, ParseError> {
        Ok(SessionPlaylistTrack {
            id: self.to_value("track_id")?,
            r#type: self.to_value("type")?,
            data: self.to_value("data")?,
        })
    }
}

impl AsModelResult<SessionPlaylistTrack, ParseError> for &moosicbox_database::Row {
    fn as_model(&self) -> Result<SessionPlaylistTrack, ParseError> {
        Ok(SessionPlaylistTrack {
            id: self.to_value("track_id")?,
            r#type: self.to_value("type")?,
            data: self.to_value("data")?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiSessionPlaylistTrack {
    pub id: String,
    pub r#type: ApiSource,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

impl ToApi<ApiSessionPlaylistTrack> for SessionPlaylistTrack {
    fn to_api(self) -> ApiSessionPlaylistTrack {
        ApiSessionPlaylistTrack {
            id: self.id,
            r#type: self.r#type,
            data: self.data,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApiSessionPlaylist {
    pub session_playlist_id: i32,
    pub tracks: Vec<ApiTrack>,
}

impl ToApi<ApiSessionPlaylist> for SessionPlaylist {
    fn to_api(self) -> ApiSessionPlaylist {
        ApiSessionPlaylist {
            session_playlist_id: self.id,
            tracks: self.tracks,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RegisterConnection {
    pub connection_id: String,
    pub name: String,
    pub players: Vec<RegisterPlayer>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Connection {
    pub id: String,
    pub name: String,
    pub created: String,
    pub updated: String,
    pub players: Vec<Player>,
}

impl ToValueType<Connection> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<Connection, ParseError> {
        Ok(Connection {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
            ..Default::default()
        })
    }
}

#[async_trait]
impl AsModelQuery<Connection> for &moosicbox_database::Row {
    async fn as_model_query(&self, db: &dyn Database) -> Result<Connection, DbError> {
        let id = self.to_value::<String>("id")?;
        let players = get_players(db, &id).await?;
        Ok(Connection {
            id,
            name: self.to_value("name")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
            players,
        })
    }
}

impl AsId for Connection {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::String(self.id.clone())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiConnection {
    pub connection_id: String,
    pub name: String,
    pub alive: bool,
    pub players: Vec<ApiPlayer>,
}

impl ToApi<ApiConnection> for Connection {
    fn to_api(self) -> ApiConnection {
        ApiConnection {
            connection_id: self.id,
            name: self.name,
            alive: false,
            players: self.players.iter().map(|p| p.to_api()).collect(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RegisterPlayer {
    pub name: String,
    pub r#type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default, EnumString)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum PlayerType {
    Symphonia,
    Howler,
    #[default]
    Unknown,
}

impl MissingValue<PlayerType> for &moosicbox_database::Row {}
impl ToValueType<PlayerType> for DatabaseValue {
    fn to_value_type(self) -> Result<PlayerType, ParseError> {
        match self {
            DatabaseValue::String(str) | DatabaseValue::StringOpt(Some(str)) => {
                Ok(PlayerType::from_str(&str).unwrap_or(PlayerType::Unknown))
            }
            _ => Err(ParseError::ConvertType("PlayerType".into())),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub id: i32,
    pub name: String,
    pub r#type: PlayerType,
    pub playing: bool,
    pub created: String,
    pub updated: String,
}

impl MissingValue<Player> for &moosicbox_database::Row {}
impl ToValueType<Player> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<Player, ParseError> {
        Ok(Player {
            id: self.to_value("id")?,
            name: self.to_value("name")?,
            r#type: self.to_value("type")?,
            playing: self.to_value("playing")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsId for Player {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ActivePlayer {
    pub id: i32,
    pub session_id: i32,
    pub player_id: i32,
    pub created: String,
    pub updated: String,
}

impl MissingValue<ActivePlayer> for &moosicbox_database::Row {}
impl ToValueType<ActivePlayer> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<ActivePlayer, ParseError> {
        Ok(ActivePlayer {
            id: self.to_value("id")?,
            session_id: self.to_value("session_id")?,
            player_id: self.to_value("player_id")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsModelResult<ActivePlayer, ParseError> for &moosicbox_database::Row {
    fn as_model(&self) -> Result<ActivePlayer, ParseError> {
        self.to_value_type()
    }
}

impl AsModel<ActivePlayer> for &moosicbox_database::Row {
    fn as_model(&self) -> ActivePlayer {
        self.to_value_type().unwrap()
    }
}

impl AsId for ActivePlayer {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(self.id as i64)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ApiPlayer {
    pub player_id: i32,
    pub name: String,
    pub r#type: PlayerType,
    pub playing: bool,
}

impl ToApi<ApiPlayer> for Player {
    fn to_api(self) -> ApiPlayer {
        ApiPlayer {
            player_id: self.id,
            name: self.name.clone(),
            r#type: self.r#type.clone(),
            playing: self.playing,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SetSeek {
    pub session_id: i32,
    pub seek: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ClientAccessToken {
    pub token: String,
    pub client_id: String,
    pub created: String,
    pub updated: String,
}

impl AsModel<ClientAccessToken> for &moosicbox_database::Row {
    fn as_model(&self) -> ClientAccessToken {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<ClientAccessToken, ParseError> for &moosicbox_database::Row {
    fn as_model(&self) -> Result<ClientAccessToken, ParseError> {
        Ok(ClientAccessToken {
            token: self.to_value("token")?,
            client_id: self.to_value("client_id")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsId for ClientAccessToken {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::String(self.token.clone())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MagicToken {
    pub magic_token: String,
    pub client_id: String,
    pub access_token: String,
    pub created: String,
    pub updated: String,
}

impl AsModel<MagicToken> for &moosicbox_database::Row {
    fn as_model(&self) -> MagicToken {
        AsModelResult::as_model(self).unwrap()
    }
}

impl AsModelResult<MagicToken, ParseError> for &moosicbox_database::Row {
    fn as_model(&self) -> Result<MagicToken, ParseError> {
        Ok(MagicToken {
            magic_token: self.to_value("magic_token")?,
            client_id: self.to_value("client_id")?,
            access_token: self.to_value("access_token")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

impl AsId for MagicToken {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::String(self.magic_token.clone())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct TrackSize {
    pub id: i32,
    pub track_id: i32,
    pub bytes: Option<u64>,
    pub format: String,
}

impl AsModel<TrackSize> for &moosicbox_database::Row {
    fn as_model(&self) -> TrackSize {
        AsModelResult::as_model(self).unwrap()
    }
}

impl ToValueType<TrackSize> for &moosicbox_database::Row {
    fn to_value_type(self) -> Result<TrackSize, ParseError> {
        Ok(TrackSize {
            id: self.to_value("id")?,
            track_id: self.to_value("track_id")?,
            bytes: self.to_value("bytes")?,
            format: self.to_value("format")?,
        })
    }
}

impl AsModelResult<TrackSize, ParseError> for &moosicbox_database::Row {
    fn as_model(&self) -> Result<TrackSize, ParseError> {
        Ok(TrackSize {
            id: self.to_value("id")?,
            track_id: self.to_value("track_id")?,
            bytes: self.to_value("bytes")?,
            format: self.to_value("format")?,
        })
    }
}

impl AsId for TrackSize {
    fn as_id(&self) -> DatabaseValue {
        DatabaseValue::Number(self.id as i64)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Id {
    String(String),
    Number(u64),
}

impl Default for Id {
    fn default() -> Self {
        Id::Number(0)
    }
}

impl From<&String> for Id {
    fn from(value: &String) -> Self {
        Id::String(value.clone())
    }
}

impl From<String> for Id {
    fn from(value: String) -> Self {
        Id::String(value)
    }
}

impl From<Id> for String {
    fn from(value: Id) -> Self {
        if let Id::String(string) = value {
            string
        } else {
            panic!("Not String Id type");
        }
    }
}

impl From<&Id> for String {
    fn from(value: &Id) -> Self {
        if let Id::String(string) = value {
            string.to_string()
        } else {
            panic!("Not String Id type");
        }
    }
}

impl<'a> From<&'a Id> for &'a str {
    fn from(value: &'a Id) -> Self {
        if let Id::String(string) = value {
            string
        } else {
            panic!("Not String Id type");
        }
    }
}

impl From<&str> for Id {
    fn from(value: &str) -> Self {
        Id::String(value.to_string())
    }
}

impl From<i32> for Id {
    fn from(value: i32) -> Self {
        Id::Number(value as u64)
    }
}

impl From<&i32> for Id {
    fn from(value: &i32) -> Self {
        Id::Number(*value as u64)
    }
}

impl From<u64> for Id {
    fn from(value: u64) -> Self {
        Id::Number(value)
    }
}

impl From<Id> for u64 {
    fn from(value: Id) -> Self {
        if let Id::Number(number) = value {
            number
        } else {
            panic!("Not u64 Id type");
        }
    }
}

impl From<Id> for i32 {
    fn from(value: Id) -> Self {
        if let Id::Number(number) = value {
            number as i32
        } else {
            panic!("Not i32 Id type");
        }
    }
}

impl From<&Id> for i32 {
    fn from(value: &Id) -> Self {
        if let Id::Number(number) = value {
            *number as i32
        } else {
            panic!("Not i32 Id type");
        }
    }
}

impl From<&Id> for u64 {
    fn from(value: &Id) -> Self {
        if let Id::Number(number) = value {
            *number
        } else {
            panic!("Not u64 Id type");
        }
    }
}

impl From<&u64> for Id {
    fn from(value: &u64) -> Self {
        Id::Number(*value)
    }
}

impl Display for Id {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Id::String(string) => f.write_str(string),
            Id::Number(number) => f.write_fmt(format_args!("{number}")),
        }
    }
}
