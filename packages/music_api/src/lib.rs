#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::fmt::{Display, Formatter};

use async_trait::async_trait;
use moosicbox_core::sqlite::models::{
    Album, AlbumId, ApiSource, Artist, ArtistId, LibraryAlbum, Track,
};
use moosicbox_paging::PagingResult;
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;

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
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Error)]
pub enum ArtistError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Error)]
pub enum AddArtistError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Error)]
pub enum RemoveArtistError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Error)]
pub enum AlbumsError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Error)]
pub enum AlbumError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Error)]
pub enum ArtistAlbumsError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
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
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Error)]
pub enum AddAlbumError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Error)]
pub enum RemoveAlbumError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Error)]
pub enum TracksError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Error)]
pub enum TrackError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Error)]
pub enum AddTrackError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Error)]
pub enum RemoveTrackError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Id {
    String(String),
    Number(u64),
}

impl From<ArtistId> for Id {
    fn from(value: ArtistId) -> Self {
        match value {
            ArtistId::Library(value) => Id::Number(value as u64),
            ArtistId::Tidal(value) => Id::Number(value),
            ArtistId::Qobuz(value) => Id::Number(value),
        }
    }
}

impl From<&ArtistId> for Id {
    fn from(value: &ArtistId) -> Self {
        match value {
            ArtistId::Library(value) => Id::Number(*value as u64),
            ArtistId::Tidal(value) => Id::Number(*value),
            ArtistId::Qobuz(value) => Id::Number(*value),
        }
    }
}

impl From<AlbumId> for Id {
    fn from(value: AlbumId) -> Self {
        match value {
            AlbumId::Library(value) => Id::Number(value as u64),
            AlbumId::Tidal(value) => Id::Number(value),
            AlbumId::Qobuz(value) => Id::String(value.clone()),
        }
    }
}

impl From<&AlbumId> for Id {
    fn from(value: &AlbumId) -> Self {
        match value {
            AlbumId::Library(value) => Id::Number(*value as u64),
            AlbumId::Tidal(value) => Id::Number(*value),
            AlbumId::Qobuz(value) => Id::String(value.clone()),
        }
    }
}

impl From<Artist> for Id {
    fn from(value: Artist) -> Self {
        match value {
            Artist::Library(value) => Id::Number(value.id as u64),
            Artist::Tidal(value) => Id::Number(value.id),
            Artist::Qobuz(value) => Id::Number(value.id),
        }
    }
}

impl From<Album> for Id {
    fn from(value: Album) -> Self {
        match value {
            Album::Library(value) => Id::Number(value.id as u64),
            Album::Tidal(value) => Id::Number(value.id),
            Album::Qobuz(value) => Id::String(value.id),
        }
    }
}

impl From<Track> for Id {
    fn from(value: Track) -> Self {
        match value {
            Track::Library(value) => Id::Number(value.id as u64),
            Track::Tidal(value) => Id::Number(value.id),
            Track::Qobuz(value) => Id::Number(value.id),
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

#[async_trait]
pub trait MusicApi {
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

    async fn albums(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<AlbumOrder>,
        order_direction: Option<AlbumOrderDirection>,
    ) -> PagingResult<Album, AlbumsError>;

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

    async fn library_album(&self, album_id: &Id)
        -> Result<Option<LibraryAlbum>, LibraryAlbumError>;

    async fn add_album(&self, album_id: &Id) -> Result<(), AddAlbumError>;

    async fn remove_album(&self, album_id: &Id) -> Result<(), RemoveAlbumError>;

    async fn tracks(
        &self,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> PagingResult<Track, TracksError>;

    async fn track(&self, track_id: &Id) -> Result<Option<Track>, TrackError>;

    async fn add_track(&self, track_id: &Id) -> Result<(), AddTrackError>;

    async fn remove_track(&self, track_id: &Id) -> Result<(), RemoveTrackError>;
}
