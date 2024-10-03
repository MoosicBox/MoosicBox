use std::{
    fmt::{Display, Formatter},
    num::ParseIntError,
    ops::Deref,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
};

use async_trait::async_trait;
use moosicbox_database::{AsId, Database, DatabaseValue};
use moosicbox_json_utils::{database::ToValue as _, MissingValue, ParseError, ToValueType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;

use crate::types::AudioFormat;

use super::db::DbError;

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
    async fn as_model_mapped_query(&self, db: Arc<Box<dyn Database>>) -> Result<Vec<T>, E>;
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

#[async_trait]
pub trait AsModelQuery<T> {
    async fn as_model_query(&self, db: Arc<Box<dyn Database>>) -> Result<T, DbError>;
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
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum TrackApiSource {
    #[default]
    Local,
    Tidal,
    Qobuz,
    Yt,
}

impl Display for TrackApiSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl From<TrackApiSource> for ApiSource {
    fn from(value: TrackApiSource) -> Self {
        match value {
            TrackApiSource::Local => ApiSource::Library,
            TrackApiSource::Tidal => ApiSource::Tidal,
            TrackApiSource::Qobuz => ApiSource::Qobuz,
            TrackApiSource::Yt => ApiSource::Yt,
        }
    }
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

#[derive(Default, Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: Id,
    pub number: u32,
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
            source: value.source,
            api_source: ApiSource::Library,
            sources: ApiSources::default()
                .with_source_opt(ApiSource::Tidal, value.tidal_id.map(|x| x.into()))
                .with_source_opt(ApiSource::Qobuz, value.qobuz_id.map(|x| x.into()))
                .with_source_opt(ApiSource::Yt, value.yt_id.map(|x| x.into())),
        }
    }
}

impl Track {
    pub fn directory(&self) -> Option<String> {
        self.file
            .as_ref()
            .and_then(|f| PathBuf::from_str(f).ok())
            .map(|p| p.parent().unwrap().to_str().unwrap().to_string())
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct Artist {
    pub id: Id,
    pub title: String,
    pub cover: Option<String>,
    pub source: ApiSource,
    pub sources: ApiSources,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ApiSources(Vec<(ApiSource, Id)>);

impl ApiSources {
    pub fn add_source(&mut self, source: ApiSource, id: Id) {
        self.0.push((source, id));
    }

    pub fn remove_source(&mut self, source: ApiSource) {
        self.0.retain_mut(|x| x.0 != source);
    }

    pub fn add_source_opt(&mut self, source: ApiSource, id: Option<Id>) {
        if let Some(id) = id {
            self.0.push((source, id));
        }
    }

    pub fn with_source(mut self, source: ApiSource, id: Id) -> Self {
        self.0.push((source, id));
        self
    }

    pub fn with_source_opt(mut self, source: ApiSource, id: Option<Id>) -> Self {
        if let Some(id) = id {
            self.0.push((source, id));
        }
        self
    }

    pub fn get(&self, source: ApiSource) -> Option<&Id> {
        self.deref()
            .iter()
            .find_map(|x| if x.0 == source { Some(&x.1) } else { None })
    }
}

impl Deref for ApiSources {
    type Target = [(ApiSource, Id)];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Default, Debug, Serialize, Deserialize, Clone, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
    pub source: AlbumSource,
    pub artist_sources: ApiSources,
    pub album_sources: ApiSources,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct ApiAlbumVersionQuality {
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackApiSource,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Eq, PartialEq, Default, AsRefStr)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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

#[derive(
    Default, Copy, Debug, Serialize, Deserialize, EnumString, AsRefStr, Eq, PartialEq, Clone, Hash,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum ApiSource {
    #[default]
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
    pub id: u64,
    pub track_id: u64,
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

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub enum IdType {
    Artist,
    Album,
    Track,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Id {
    String(String),
    Number(u64),
}

#[cfg(feature = "openapi")]
impl utoipa::__dev::SchemaReferences for Id {
    fn schemas(
        schemas: &mut Vec<(
            String,
            utoipa::openapi::RefOr<utoipa::openapi::schema::Schema>,
        )>,
    ) {
        use utoipa::PartialSchema as _;

        schemas.push(("Id".to_string(), String::schema()))
    }
}

#[cfg(feature = "openapi")]
impl utoipa::PartialSchema for Id {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        String::schema()
    }
}

#[cfg(feature = "openapi")]
impl utoipa::ToSchema for Id {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Id")
    }
}

#[derive(Debug, Error)]
pub enum IdFromStrError {
    #[error(transparent)]
    PopulateIndex(#[from] ParseIntError),
}

impl Id {
    pub fn from_str(value: &str, source: ApiSource, id_type: IdType) -> Self {
        Self::try_from_str(value, source, id_type).unwrap()
    }

    pub fn try_from_str(
        value: &str,
        source: ApiSource,
        id_type: IdType,
    ) -> Result<Self, IdFromStrError> {
        Ok(match id_type {
            IdType::Artist => match source {
                ApiSource::Library => Self::Number(value.parse::<u64>()?),
                ApiSource::Tidal => Self::Number(value.parse::<u64>()?),
                ApiSource::Qobuz => Self::Number(value.parse::<u64>()?),
                ApiSource::Yt => Self::String(value.to_owned()),
            },
            IdType::Album => match source {
                ApiSource::Library => Self::Number(value.parse::<u64>()?),
                ApiSource::Tidal => Self::Number(value.parse::<u64>()?),
                ApiSource::Qobuz => Self::String(value.to_owned()),
                ApiSource::Yt => Self::String(value.to_owned()),
            },
            IdType::Track => match source {
                ApiSource::Library => Self::Number(value.parse::<u64>()?),
                ApiSource::Tidal => Self::Number(value.parse::<u64>()?),
                ApiSource::Qobuz => Self::Number(value.parse::<u64>()?),
                ApiSource::Yt => Self::String(value.to_owned()),
            },
        })
    }

    pub fn default_value(source: ApiSource, id_type: IdType) -> Id {
        match id_type {
            IdType::Artist => match source {
                ApiSource::Library => Self::Number(0),
                ApiSource::Tidal => Self::Number(0),
                ApiSource::Qobuz => Self::Number(0),
                ApiSource::Yt => Self::String("".into()),
            },
            IdType::Album => match source {
                ApiSource::Library => Self::Number(0),
                ApiSource::Tidal => Self::Number(0),
                ApiSource::Qobuz => Self::String("".into()),
                ApiSource::Yt => Self::String("".into()),
            },
            IdType::Track => match source {
                ApiSource::Library => Self::Number(0),
                ApiSource::Tidal => Self::Number(0),
                ApiSource::Qobuz => Self::Number(0),
                ApiSource::Yt => Self::String("".into()),
            },
        }
    }

    pub fn is_number(&self) -> bool {
        match self {
            Id::String(_) => false,
            Id::Number(_) => true,
        }
    }

    pub fn is_string(&self) -> bool {
        match self {
            Id::String(_) => true,
            Id::Number(_) => false,
        }
    }
}

impl Serialize for Id {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Id::String(id) => id.serialize(serializer),
            Id::Number(id) => id.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Id {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value: Value = Value::deserialize(deserializer)?;

        if value.is_number() {
            Ok(Id::Number(value.as_u64().unwrap()))
        } else if value.is_string() {
            Ok(Id::String(value.as_str().unwrap().to_string()))
        } else {
            panic!("invalid type")
        }
    }
}

impl MissingValue<Id> for &moosicbox_database::Row {}
impl ToValueType<Id> for DatabaseValue {
    fn to_value_type(self) -> Result<Id, ParseError> {
        match self {
            DatabaseValue::String(x) | DatabaseValue::StringOpt(Some(x)) => Ok(Id::String(x)),
            DatabaseValue::Number(x) | DatabaseValue::NumberOpt(Some(x)) => {
                Ok(Id::Number(x as u64))
            }
            DatabaseValue::UNumber(x) | DatabaseValue::UNumberOpt(Some(x)) => Ok(Id::Number(x)),
            _ => Err(ParseError::ConvertType("Id".into())),
        }
    }
}

impl ToValueType<Id> for &serde_json::Value {
    fn to_value_type(self) -> Result<Id, ParseError> {
        if self.is_number() {
            return Ok(Id::Number(
                self.as_u64()
                    .ok_or_else(|| ParseError::ConvertType("Id".into()))?,
            ));
        }
        if self.is_string() {
            return Ok(Id::String(
                self.as_str()
                    .ok_or_else(|| ParseError::ConvertType("Id".into()))?
                    .to_string(),
            ));
        }
        Err(ParseError::ConvertType("Id".into()))
    }
}

#[cfg(feature = "tantivy")]
impl ToValueType<Id> for &tantivy::schema::OwnedValue {
    fn to_value_type(self) -> Result<Id, ParseError> {
        use tantivy::schema::Value;
        if let Some(id) = self.as_u64() {
            Ok(Id::Number(id))
        } else if let Some(id) = self.as_str() {
            Ok(Id::String(id.to_owned()))
        } else {
            Err(ParseError::ConvertType("Id".to_string()))
        }
    }
}

impl Default for Id {
    fn default() -> Self {
        Id::Number(0)
    }
}

impl From<Id> for DatabaseValue {
    fn from(val: Id) -> Self {
        match val {
            Id::String(x) => DatabaseValue::String(x),
            Id::Number(x) => DatabaseValue::UNumber(x),
        }
    }
}

impl From<&Id> for DatabaseValue {
    fn from(val: &Id) -> Self {
        match val {
            Id::String(x) => DatabaseValue::String(x.to_owned()),
            Id::Number(x) => DatabaseValue::UNumber(*x),
        }
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
