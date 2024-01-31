#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    fmt::{Display, Formatter},
    ops::Deref,
};

use async_trait::async_trait;
use moosicbox_core::sqlite::models::{Album, Artist, ToApi, Track};
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PagingResponse<T> {
    WithTotal {
        items: Vec<T>,
        offset: u32,
        total: u32,
    },
    WithHasMore {
        items: Vec<T>,
        offset: u32,
        has_more: bool,
    },
}

impl<T> PagingResponse<T> {
    pub fn offset(&self) -> u32 {
        match self {
            Self::WithTotal { offset, .. } => *offset,
            Self::WithHasMore { offset, .. } => *offset,
        }
    }

    pub fn has_more(&self) -> bool {
        match self {
            Self::WithTotal {
                items,
                offset,
                total,
            } => *offset + (items.len() as u32) < *total,
            Self::WithHasMore { has_more, .. } => *has_more,
        }
    }

    pub fn total(&self) -> Option<u32> {
        match self {
            Self::WithTotal { total, .. } => Some(*total),
            Self::WithHasMore { .. } => None,
        }
    }

    pub fn items(self) -> Vec<T> {
        match self {
            Self::WithTotal { items, .. } => items,
            Self::WithHasMore { items, .. } => items,
        }
    }

    pub fn map<U, F>(self, f: F) -> PagingResponse<U>
    where
        F: FnMut(T) -> U,
    {
        match self {
            Self::WithTotal {
                items,
                offset,
                total,
            } => PagingResponse::WithTotal {
                items: items.into_iter().map(f).collect::<Vec<_>>(),
                offset,
                total,
            },
            Self::WithHasMore {
                items,
                offset,
                has_more,
            } => PagingResponse::WithHasMore {
                items: items.into_iter().map(f).collect::<Vec<_>>(),
                offset,
                has_more,
            },
        }
    }
}

impl<T> Deref for PagingResponse<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::WithTotal { items, .. } => items,
            Self::WithHasMore { items, .. } => items,
        }
    }
}

impl<In, Out> ToApi<PagingResponse<Out>> for PagingResponse<In>
where
    In: ToApi<Out>,
{
    fn to_api(&self) -> PagingResponse<Out> {
        let items = self.iter().map(|item| item.to_api()).collect::<Vec<Out>>();

        match self {
            Self::WithTotal { total, offset, .. } => PagingResponse::WithTotal {
                items,
                offset: *offset,
                total: *total,
            },
            Self::WithHasMore {
                has_more, offset, ..
            } => PagingResponse::WithHasMore {
                items,
                offset: *offset,
                has_more: *has_more,
            },
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
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Error)]
pub enum ArtistError {
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
pub enum TracksError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Error)]
pub enum TrackError {
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error>),
}

#[derive(Debug, Clone)]
pub enum Id {
    String(String),
    Number(u64),
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
    async fn artists(
        &self,
        #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<ArtistOrder>,
        order_direction: Option<ArtistOrderDirection>,
    ) -> Result<PagingResponse<Artist>, ArtistsError>;

    async fn artist(
        &self,
        #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
        artist_id: Id,
    ) -> Result<Option<Artist>, ArtistError>;

    async fn albums(
        &self,
        #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<AlbumOrder>,
        order_direction: Option<AlbumOrderDirection>,
    ) -> Result<PagingResponse<Album>, AlbumsError>;

    async fn album(
        &self,
        #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
        album_id: Id,
    ) -> Result<Option<Album>, AlbumError>;

    async fn tracks(
        &self,
        #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
        offset: Option<u32>,
        limit: Option<u32>,
        order: Option<TrackOrder>,
        order_direction: Option<TrackOrderDirection>,
    ) -> Result<PagingResponse<Track>, TracksError>;

    async fn track(
        &self,
        #[cfg(feature = "db")] db: &moosicbox_core::app::Db,
        track_id: Id,
    ) -> Result<Option<Track>, TrackError>;
}
