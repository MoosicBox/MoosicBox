#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{collections::HashMap, str::FromStr as _, sync::Arc};

use async_trait::async_trait;
use moosicbox_core::{
    sqlite::models::{Album, AlbumSort, AlbumSource, ApiSource, Artist, Id, Track},
    types::{AudioFormat, PlaybackQuality},
};
use moosicbox_database::DatabaseValue;
use moosicbox_json_utils::{MissingValue, ParseError, ToValueType};
use moosicbox_paging::{PagingRequest, PagingResult};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;

#[derive(Clone)]
pub struct MusicApis(Arc<HashMap<ApiSource, Arc<Box<dyn MusicApi>>>>);

#[derive(Debug, Error)]
pub enum MusicApisError {
    #[error("Music API for source not found: {0}")]
    NotFound(ApiSource),
}

impl MusicApis {
    pub fn get(&self, source: ApiSource) -> Result<Arc<Box<dyn MusicApi>>, MusicApisError> {
        let api = self
            .0
            .get(&source)
            .ok_or(MusicApisError::NotFound(source))?;

        Ok(api.clone())
    }
}

#[derive(Clone)]
pub struct MusicApiState {
    pub apis: MusicApis,
}

impl MusicApiState {
    pub fn new(apis: HashMap<ApiSource, Box<dyn MusicApi>>) -> Self {
        Self {
            apis: MusicApis(Arc::new(
                apis.into_iter()
                    .map(|(source, api)| (source, Arc::new(api)))
                    .collect(),
            )),
        }
    }
}

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
    pub artist_id: Option<Id>,
    pub tidal_artist_id: Option<Id>,
    pub qobuz_artist_id: Option<Id>,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ArtistOrder {
    DateAdded,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum ArtistOrderDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AlbumOrder {
    DateAdded,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AlbumOrderDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackOrder {
    DateAdded,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackOrderDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum AlbumType {
    All,
    Lp,
    Live,
    Compilations,
    EpsAndSingles,
    Other,
    Download,
}

#[derive(Clone, Debug)]
pub enum TrackSource {
    LocalFilePath {
        path: String,
        format: AudioFormat,
        track_id: Option<Id>,
    },
    RemoteUrl {
        url: String,
        format: AudioFormat,
        track_id: Option<Id>,
    },
}

impl TrackSource {
    pub fn format(&self) -> AudioFormat {
        match self {
            TrackSource::LocalFilePath { format, .. } => *format,
            TrackSource::RemoteUrl { format, .. } => *format,
        }
    }

    pub fn track_id(&self) -> Option<&Id> {
        match self {
            TrackSource::LocalFilePath { track_id, .. } => track_id.as_ref(),
            TrackSource::RemoteUrl { track_id, .. } => track_id.as_ref(),
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, EnumString, AsRefStr, PartialEq, Clone, Copy)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
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

#[cfg(feature = "db")]
impl MissingValue<TrackAudioQuality> for &rusqlite::Row<'_> {}
#[cfg(feature = "db")]
impl ToValueType<TrackAudioQuality> for rusqlite::types::Value {
    fn to_value_type(self) -> Result<TrackAudioQuality, ParseError> {
        match self {
            rusqlite::types::Value::Text(str) => Ok(TrackAudioQuality::from_str(&str)
                .map_err(|_| ParseError::ConvertType("TrackAudioQuality".into()))?),
            _ => Err(ParseError::ConvertType("TrackAudioQuality".into())),
        }
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
            0..=80 => ImageCoverSize::Thumbnail,
            81..=160 => ImageCoverSize::Small,
            161..=320 => ImageCoverSize::Medium,
            321..=640 => ImageCoverSize::Large,
            _ => ImageCoverSize::Max,
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
        str.parse::<u64>().unwrap()
    }
}

#[derive(Debug, Error)]
pub enum ArtistsError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum ArtistError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum AddArtistError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum RemoveArtistError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum AlbumsError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum AlbumError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum ArtistAlbumsError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum LibraryAlbumError {
    #[cfg(not(feature = "db"))]
    #[error("No DB")]
    NoDb,
    #[cfg(feature = "db")]
    #[error(transparent)]
    Db(#[from] moosicbox_core::sqlite::db::DbError),
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum AddAlbumError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum RemoveAlbumError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum TracksError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum TrackError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum AddTrackError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[derive(Debug, Error)]
pub enum RemoveTrackError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

#[async_trait]
pub trait MusicApi: Send + Sync {
    fn source(&self) -> ApiSource;

    async fn artists(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<ArtistOrder>,
        order_direction: Option<ArtistOrderDirection>,
    ) -> PagingResult<Artist, ArtistsError>;

    async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, ArtistError>;

    async fn add_artist(&self, artist_id: &Id) -> Result<(), AddArtistError>;

    async fn remove_artist(&self, artist_id: &Id) -> Result<(), RemoveArtistError>;

    async fn album_artist(&self, album_id: &Id) -> Result<Option<Artist>, ArtistError> {
        let album = if let Some(album) = self
            .album(album_id)
            .await
            .map_err(|e| ArtistError::Other(e.into()))?
        {
            album
        } else {
            return Ok(None);
        };

        self.artist(&album.artist_id).await
    }

    async fn artist_cover_source(
        &self,
        artist: &Artist,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, ArtistError> {
        Ok(artist
            .cover
            .as_ref()
            .cloned()
            .map(ImageCoverSource::RemoteUrl))
    }

    async fn albums(&self, request: &AlbumsRequest) -> PagingResult<Album, AlbumsError>;

    async fn album(&self, album_id: &Id) -> Result<Option<Album>, AlbumError>;

    #[allow(clippy::too_many_arguments)]
    async fn artist_albums(
        &self,
        artist_id: &Id,
        album_type: AlbumType,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<AlbumOrder>,
        order_direction: Option<AlbumOrderDirection>,
    ) -> PagingResult<Album, ArtistAlbumsError>;

    async fn add_album(&self, album_id: &Id) -> Result<(), AddAlbumError>;

    async fn remove_album(&self, album_id: &Id) -> Result<(), RemoveAlbumError>;

    async fn album_cover_source(
        &self,
        album: &Album,
        _size: ImageCoverSize,
    ) -> Result<Option<ImageCoverSource>, AlbumError> {
        Ok(album
            .artwork
            .as_ref()
            .cloned()
            .map(ImageCoverSource::RemoteUrl))
    }

    async fn tracks(
        &self,
        track_ids: Option<&[Id]>,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, TracksError>;

    async fn track(&self, track_id: &Id) -> Result<Option<Track>, TrackError>;

    async fn album_tracks(
        &self,
        album_id: &Id,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, TracksError>;

    async fn add_track(&self, track_id: &Id) -> Result<(), AddTrackError>;

    async fn remove_track(&self, track_id: &Id) -> Result<(), RemoveTrackError>;

    async fn track_source(
        &self,
        track: &Track,
        quality: TrackAudioQuality,
    ) -> Result<Option<TrackSource>, TrackError>;

    async fn track_size(
        &self,
        track_id: &Id,
        source: &TrackSource,
        quality: PlaybackQuality,
    ) -> Result<Option<u64>, TrackError>;
}
