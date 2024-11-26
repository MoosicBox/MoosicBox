#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use moosicbox_core::sqlite::models::ToApi;
use std::{ops::Deref, pin::Pin, sync::Arc};

use futures::Future;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Debug)]
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

#[cfg(feature = "openapi")]
impl<T> utoipa::PartialSchema for Page<T> {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::{ObjectBuilder, RefOr, Schema};

        RefOr::T(Schema::Object(ObjectBuilder::new().build()))
    }
}

#[cfg(feature = "openapi")]
impl<T> utoipa::ToSchema for Page<T> {
    fn name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Page")
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Page<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Extended<T> {
            items: Vec<T>,
            offset: u32,
            limit: u32,
            total: Option<u32>,
            has_more: bool,
        }

        let extended: Extended<T> = Extended::deserialize(deserializer)?;

        Ok(if let Some(total) = extended.total {
            Self::WithTotal {
                offset: extended.offset,
                limit: extended.limit,
                total,
                items: extended.items,
            }
        } else {
            Self::WithHasMore {
                offset: extended.offset,
                limit: extended.limit,
                has_more: extended.has_more,
                items: extended.items,
            }
        })
    }
}

impl<T: Serialize> Serialize for Page<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::WithTotal {
                items,
                offset,
                limit,
                total,
            } => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Extended<'a, T> {
                    items: &'a Vec<T>,
                    offset: &'a u32,
                    limit: &'a u32,
                    total: &'a u32,
                    has_more: bool,
                }

                let ext = Extended {
                    items,
                    offset,
                    limit,
                    total,
                    has_more: u32::try_from(items.len()).unwrap() + offset < *total,
                };

                Ok(ext.serialize(serializer)?)
            }
            Self::WithHasMore {
                items,
                offset,
                limit,
                has_more,
            } => {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Copy<'a, T> {
                    items: &'a Vec<T>,
                    offset: &'a u32,
                    limit: &'a u32,
                    has_more: &'a bool,
                }
                let copy = Copy {
                    items,
                    offset,
                    limit,
                    has_more,
                };
                Ok(copy.serialize(serializer)?)
            }
        }
    }
}

impl<T> Page<T> {
    #[must_use]
    pub const fn offset(&self) -> u32 {
        match self {
            Self::WithTotal { offset, .. } | Self::WithHasMore { offset, .. } => *offset,
        }
    }

    #[must_use]
    pub const fn limit(&self) -> u32 {
        match self {
            Self::WithTotal { limit, .. } | Self::WithHasMore { limit, .. } => *limit,
        }
    }

    /// # Panics
    ///
    /// * If the `items.len()` cannot be converted to a `u32`
    #[must_use]
    pub fn has_more(&self) -> bool {
        match self {
            Self::WithTotal {
                items,
                offset,
                total,
                ..
            } => *offset + u32::try_from(items.len()).unwrap() < *total,
            Self::WithHasMore { has_more, .. } => *has_more,
        }
    }

    #[must_use]
    pub const fn total(&self) -> Option<u32> {
        match self {
            Self::WithTotal { total, .. } => Some(*total),
            Self::WithHasMore { .. } => None,
        }
    }

    #[must_use]
    pub const fn remaining(&self) -> Option<u32> {
        match self {
            Self::WithTotal {
                total,
                offset,
                limit,
                ..
            } => Some(*total - *offset - *limit),
            Self::WithHasMore { .. } => None,
        }
    }

    #[must_use]
    pub fn items(&self) -> &[T] {
        match self {
            Self::WithTotal { items, .. } | Self::WithHasMore { items, .. } => items,
        }
    }

    #[must_use]
    pub fn into_items(self) -> Vec<T> {
        match self {
            Self::WithTotal { items, .. } | Self::WithHasMore { items, .. } => items,
        }
    }

    pub fn map<U, F>(self, mut f: F) -> Page<U>
    where
        F: FnMut(T) -> U + Send + Clone + 'static,
        T: 'static,
    {
        match self {
            Self::WithTotal {
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
            Self::WithHasMore {
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
        }
    }

    pub fn into<TU>(self) -> Page<TU>
    where
        T: Into<TU> + 'static,
    {
        match self {
            Self::WithTotal {
                items,
                offset,
                limit,
                total,
            } => Page::WithTotal {
                items: items.into_iter().map(Into::into).collect::<Vec<_>>(),
                offset,
                limit,
                total,
            },
            Self::WithHasMore {
                items,
                offset,
                limit,
                has_more,
            } => Page::WithHasMore {
                items: items.into_iter().map(Into::into).collect::<Vec<_>>(),
                offset,
                limit,
                has_more,
            },
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PagingRequest {
    pub offset: u32,
    pub limit: u32,
}

type FuturePagingResponse<T, E> = Pin<Box<dyn Future<Output = PagingResult<T, E>> + Send>>;
type FetchPagingResponse<T, E> = Box<dyn FnMut(u32, u32) -> FuturePagingResponse<T, E> + Send>;

pub struct PagingResponse<T, E> {
    pub page: Page<T>,
    pub fetch: Arc<Mutex<FetchPagingResponse<T, E>>>,
}

impl<T: std::fmt::Debug, E> std::fmt::Debug for PagingResponse<T, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PagingResponse")
            .field("page", &self.page)
            .finish_non_exhaustive()
    }
}

impl<T: Send, E: Send> PagingResponse<T, E> {
    /// # Errors
    ///
    /// * If failed to fetch any of the subsequent `Page`s
    pub async fn rest_of_pages_in_batches(self) -> Result<Vec<Page<T>>, E> {
        self.rest_of_pages_in_batches_inner(false).await
    }

    async fn rest_of_pages_in_batches_inner(self, include_self: bool) -> Result<Vec<Page<T>>, E> {
        let Some(total) = self.total() else {
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

    /// # Errors
    ///
    /// * If failed to fetch any of the subsequent `Page`s
    pub async fn rest_of_items_in_batches(self) -> Result<Vec<T>, E> {
        Ok(self
            .rest_of_pages_in_batches()
            .await?
            .into_iter()
            .flat_map(Page::into_items)
            .collect::<Vec<_>>())
    }

    /// # Errors
    ///
    /// * If failed to fetch any of the subsequent `Page`s
    pub async fn with_rest_of_pages_in_batches(self) -> Result<Vec<Page<T>>, E> {
        self.rest_of_pages_in_batches_inner(true).await
    }

    /// # Errors
    ///
    /// * If failed to fetch any of the subsequent `Page`s
    pub async fn with_rest_of_items_in_batches(self) -> Result<Vec<T>, E> {
        Ok(self
            .with_rest_of_pages_in_batches()
            .await?
            .into_iter()
            .flat_map(Page::into_items)
            .collect::<Vec<_>>())
    }

    /// # Errors
    ///
    /// * If failed to fetch any of the subsequent `Page`s
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

    /// # Errors
    ///
    /// * If failed to fetch any of the subsequent `Page`s
    pub async fn rest_of_items(self) -> Result<Vec<T>, E> {
        Ok(self
            .rest_of_pages()
            .await?
            .into_iter()
            .flat_map(Page::into_items)
            .collect::<Vec<_>>())
    }

    /// # Errors
    ///
    /// * If failed to fetch any of the subsequent `Page`s
    pub async fn with_rest_of_pages(self) -> Result<Vec<Page<T>>, E> {
        self.rest_of_pages_inner(true).await
    }

    /// # Errors
    ///
    /// * If failed to fetch any of the subsequent `Page`s
    pub async fn with_rest_of_items(self) -> Result<Vec<T>, E> {
        Ok(self
            .with_rest_of_pages()
            .await?
            .into_iter()
            .flat_map(Page::into_items)
            .collect::<Vec<_>>())
    }

    #[must_use]
    pub const fn offset(&self) -> u32 {
        self.page.offset()
    }

    #[must_use]
    pub const fn limit(&self) -> u32 {
        self.page.limit()
    }

    #[must_use]
    pub fn has_more(&self) -> bool {
        self.page.has_more()
    }

    #[must_use]
    pub const fn total(&self) -> Option<u32> {
        self.page.total()
    }

    #[must_use]
    pub fn items(&self) -> &[T] {
        self.page.items()
    }

    #[must_use]
    pub fn into_items(self) -> Vec<T> {
        self.page.into_items()
    }

    pub fn map<U, F>(self, f: F) -> PagingResponse<U, E>
    where
        F: FnMut(T) -> U + Send + Clone + 'static,
        T: 'static,
        E: 'static,
    {
        let page = self.page.map(f.clone());

        let fetch = self.fetch;

        PagingResponse {
            page,
            fetch: Arc::new(Mutex::new(Box::new(move |offset, count| {
                let fetch = fetch.clone();
                let f = f.clone();

                let closure = async move {
                    let mut fetch = fetch.lock().await;
                    fetch(offset, count).await.map(|results| results.map(f))
                };

                Box::pin(closure)
            }))),
        }
    }

    pub fn map_err<U, F>(self, f: F) -> PagingResponse<T, U>
    where
        F: FnMut(E) -> U + Send + Clone + 'static,
        E: 'static,
        T: 'static,
    {
        let page = self.page;

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
                        .map({
                            let f = f.clone();
                            |x| x.map_err(f)
                        })
                        .map_err(f)
                };

                Box::pin(closure)
            }))),
        }
    }

    #[must_use]
    pub fn inner_into<TU: Send + 'static, EU: Send + 'static>(self) -> PagingResponse<TU, EU>
    where
        T: Into<TU> + Send + 'static,
        E: Into<EU> + Send + 'static,
    {
        let page = self.page.into();

        let fetch = self.fetch;

        PagingResponse {
            page,
            fetch: Arc::new(Mutex::new(Box::new(move |offset, count| {
                let fetch = fetch.clone();

                let closure = async move {
                    let mut fetch = fetch.lock().await;
                    fetch(offset, count)
                        .await
                        .map(|x| x.map(Into::into).map_err(Into::into))
                        .map_err(Into::into)
                };

                Box::pin(closure)
            }))),
        }
    }

    #[must_use]
    pub fn ok_into<TU: Send + 'static>(self) -> PagingResponse<TU, E>
    where
        T: Into<TU> + Send + 'static,
        E: Send + 'static,
    {
        let page = self.page.into();

        let fetch = self.fetch;

        PagingResponse {
            page,
            fetch: Arc::new(Mutex::new(Box::new(move |offset, count| {
                let fetch = fetch.clone();

                let closure = async move {
                    let mut fetch = fetch.lock().await;
                    fetch(offset, count).await.map(|x| x.map(Into::into))
                };

                Box::pin(closure)
            }))),
        }
    }

    #[must_use]
    pub fn err_into<EU: 'static>(self) -> PagingResponse<T, EU>
    where
        T: Send + 'static,
        E: Into<EU> + Send + 'static,
    {
        let page = self.page;

        let fetch = self.fetch;

        PagingResponse {
            page,
            fetch: Arc::new(Mutex::new(Box::new(move |offset, count| {
                let fetch = fetch.clone();

                let closure = async move {
                    let mut fetch = fetch.lock().await;
                    fetch(offset, count)
                        .await
                        .map(|x| x.map_err(Into::into))
                        .map_err(Into::into)
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
            Self::WithTotal { items, .. } | Self::WithHasMore { items, .. } => items,
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
            Page::WithTotal { items, .. } | Page::WithHasMore { items, .. } => items,
        }
    }
}

impl<In, Out: Send + 'static, E> ToApi<PagingResponse<Out, E>> for PagingResponse<In, E>
where
    In: Send + ToApi<Out> + 'static,
    E: Send + 'static,
{
    fn to_api(self) -> PagingResponse<Out, E> {
        self.map(ToApi::to_api)
    }
}

pub type PagingResult<T, E> = Result<PagingResponse<T, E>, E>;
