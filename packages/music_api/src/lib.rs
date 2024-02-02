#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use std::{
    fmt::{Display, Formatter},
    ops::Deref,
    pin::Pin,
    sync::Arc,
};

use async_trait::async_trait;
use futures::Future;
use moosicbox_core::sqlite::models::{
    Album, AlbumId, ApiSource, Artist, ArtistId, LibraryAlbum, ToApi, Track,
};
use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};
use thiserror::Error;
use tokio::sync::Mutex;

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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum Page<T> {
    WithTotal {
        items: Vec<T>,
        offset: u32,
        limit: u32,
        total: u32,
    },
    WithHasMore {
        items: Vec<T>,
        offset: u32,
        limit: u32,
        has_more: bool,
    },
}

impl<T> Page<T> {
    pub fn offset(&self) -> u32 {
        match self {
            Self::WithTotal { offset, .. } => *offset,
            Self::WithHasMore { offset, .. } => *offset,
        }
    }

    pub fn limit(&self) -> u32 {
        match self {
            Self::WithTotal { limit, .. } => *limit,
            Self::WithHasMore { limit, .. } => *limit,
        }
    }

    pub fn has_more(&self) -> bool {
        match self {
            Self::WithTotal {
                items,
                offset,
                total,
                ..
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
}

type FuturePagingResponse<T, E> = Pin<Box<dyn Future<Output = PagingResult<T, E>> + Send>>;
type FetchPagingResponse<T, E> = Box<dyn FnMut(u32, u32) -> FuturePagingResponse<T, E> + Send>;

pub struct PagingResponse<T, E> {
    pub page: Page<T>,
    pub fetch: Arc<Mutex<FetchPagingResponse<T, E>>>,
}

impl<T, E> PagingResponse<T, E> {
    pub async fn rest_of_pages_in_batches(self) -> Result<Vec<Page<T>>, E> {
        self.rest_of_pages_in_batches_inner(false).await
    }

    async fn rest_of_pages_in_batches_inner(self, include_self: bool) -> Result<Vec<Page<T>>, E> {
        let total = if let Some(total) = self.total() {
            total
        } else {
            return self.rest_of_pages_inner(include_self).await;
        };

        let limit = self.limit();
        let mut offset = self.offset() + limit;
        let mut requests = vec![];

        while offset < total {
            log::debug!(
                "Adding request into batch: request {} offset={offset} limit={limit}",
                requests.len() + 1
            );
            requests.push((offset, limit));

            offset += limit;
        }

        let mut responses = vec![];

        if include_self {
            responses.push(self.page);
        }

        let mut fetch = self.fetch.lock().await;
        let page_responses = futures::future::join_all(
            requests
                .into_iter()
                .map(|(offset, limit)| fetch(offset, limit)),
        )
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

        for response in page_responses {
            responses.push(response.page);
        }

        Ok(responses)
    }

    pub async fn rest_of_items_in_batches(self) -> Result<Vec<T>, E> {
        Ok(self
            .rest_of_pages_in_batches()
            .await?
            .into_iter()
            .flat_map(|response| response.items())
            .collect::<Vec<_>>())
    }

    pub async fn with_rest_of_pages_in_batches(self) -> Result<Vec<Page<T>>, E> {
        self.rest_of_pages_in_batches_inner(true).await
    }

    pub async fn with_rest_of_items_in_batches(self) -> Result<Vec<T>, E> {
        Ok(self
            .with_rest_of_pages_in_batches()
            .await?
            .into_iter()
            .flat_map(|response| response.items())
            .collect::<Vec<_>>())
    }

    pub async fn rest_of_pages(self) -> Result<Vec<Page<T>>, E> {
        self.rest_of_pages_inner(false).await
    }

    async fn rest_of_pages_inner(self, include_self: bool) -> Result<Vec<Page<T>>, E> {
        let mut limit = self.limit();
        let mut offset = self.offset() + limit;
        let mut fetch = self.fetch;
        let mut responses = vec![];

        if include_self {
            responses.push(self.page);
        }

        loop {
            let response = (fetch.lock().await)(offset, limit).await?;

            let has_more = response.has_more();
            limit = response.limit();
            offset = response.offset() + limit;
            fetch = response.fetch;

            responses.push(response.page);

            if !has_more {
                break;
            }
        }

        Ok(responses)
    }

    pub async fn rest_of_items(self) -> Result<Vec<T>, E> {
        Ok(self
            .rest_of_pages()
            .await?
            .into_iter()
            .flat_map(|response| response.items())
            .collect::<Vec<_>>())
    }

    pub async fn with_rest_of_pages(self) -> Result<Vec<Page<T>>, E> {
        self.rest_of_pages_inner(true).await
    }

    pub async fn with_rest_of_items(self) -> Result<Vec<T>, E> {
        Ok(self
            .with_rest_of_pages()
            .await?
            .into_iter()
            .flat_map(|response| response.items())
            .collect::<Vec<_>>())
    }

    pub fn offset(&self) -> u32 {
        self.page.offset()
    }

    pub fn limit(&self) -> u32 {
        self.page.limit()
    }

    pub fn has_more(&self) -> bool {
        self.page.has_more()
    }

    pub fn total(&self) -> Option<u32> {
        self.page.total()
    }

    pub fn items(self) -> Vec<T> {
        self.page.items()
    }

    pub fn map<U, F, OE>(self, mut f: F) -> PagingResponse<U, OE>
    where
        F: FnMut(T) -> U + Send + Clone + 'static,
        T: 'static,
        OE: 'static,
        E: Into<OE> + 'static,
    {
        let page = match self.page {
            Page::WithTotal {
                items,
                offset,
                limit,
                total,
            } => Page::WithTotal {
                items: items.into_iter().map(&mut f).collect::<Vec<_>>(),
                offset,
                limit,
                total,
            },
            Page::WithHasMore {
                items,
                offset,
                limit,
                has_more,
            } => Page::WithHasMore {
                items: items.into_iter().map(&mut f).collect::<Vec<_>>(),
                offset,
                limit,
                has_more,
            },
        };

        let fetch = self.fetch;

        PagingResponse {
            page,
            fetch: Arc::new(Mutex::new(Box::new(move |offset, count| {
                let fetch = fetch.clone();
                let f = f.clone();

                let closure = async move {
                    let mut fetch = fetch.lock().await;
                    fetch(offset, count)
                        .await
                        .map_err(|e| e.into())
                        .map(|results| results.map(f))
                };

                Box::pin(closure)
            }))),
        }
    }
}

impl<T, E> Deref for PagingResponse<T, E> {
    type Target = Page<T>;

    fn deref(&self) -> &Self::Target {
        &self.page
    }
}

impl<T> Deref for Page<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::WithTotal { items, .. } => items,
            Self::WithHasMore { items, .. } => items,
        }
    }
}

impl<T, E> From<PagingResponse<T, E>> for Page<T> {
    fn from(value: PagingResponse<T, E>) -> Self {
        value.page
    }
}

impl<T> From<Page<T>> for Vec<T> {
    fn from(value: Page<T>) -> Self {
        match value {
            Page::WithTotal { items, .. } => items,
            Page::WithHasMore { items, .. } => items,
        }
    }
}

impl<In, Out, E> ToApi<PagingResponse<Out, E>> for PagingResponse<In, E>
where
    In: ToApi<Out> + Clone + 'static,
    E: 'static,
{
    fn to_api(self) -> PagingResponse<Out, E> {
        self.map(|item| item.to_api())
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

pub type PagingResult<T, E> = Result<PagingResponse<T, E>, E>;

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
