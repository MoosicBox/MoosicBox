//! Pagination utilities for handling paginated API responses.
//!
//! This crate provides types and utilities for working with paginated data, supporting
//! two pagination styles:
//!
//! * **Total-based pagination** - When the total number of items is known upfront
//! * **Cursor-based pagination** - When only the existence of more items is known
//!
//! # Core Types
//!
//! * [`Page`] - A single page of items from a paginated result
//! * [`PagingResponse`] - A page with a function to fetch additional pages
//! * [`PagingRequest`] - A request for a specific page of results
//!
//! # Examples
//!
//! Creating and using a page with known total:
//!
//! ```rust
//! use moosicbox_paging::Page;
//!
//! let page = Page::WithTotal {
//!     items: vec![1, 2, 3],
//!     offset: 0,
//!     limit: 3,
//!     total: 10,
//! };
//!
//! assert_eq!(page.items(), &[1, 2, 3]);
//! assert_eq!(page.total(), Some(10));
//! assert!(page.has_more());
//! ```
//!
//! Fetching all remaining pages:
//!
//! ```rust,no_run
//! use moosicbox_paging::{Page, PagingResponse};
//!
//! # async fn example() -> Result<(), String> {
//! # let initial_page = Page::WithTotal {
//! #     items: vec![1, 2, 3],
//! #     offset: 0,
//! #     limit: 3,
//! #     total: 10,
//! # };
//! # let fetch_page = |offset: u32, limit: u32| {
//! #     Box::pin(async move {
//! #         Ok::<_, String>(PagingResponse::empty())
//! #     }) as _
//! # };
//! let response = PagingResponse::new(initial_page, fetch_page);
//! let all_items = response.with_rest_of_items().await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{ops::Deref, pin::Pin, sync::Arc};

use futures::Future;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

/// A page of items from a paginated result.
///
/// This enum represents two types of pagination:
/// * `WithTotal` - When the total number of items is known
/// * `WithHasMore` - When only whether there are more items is known
#[derive(Debug)]
pub enum Page<T> {
    /// Pagination with a known total number of items.
    WithTotal {
        /// The items in this page.
        items: Vec<T>,
        /// The offset of this page from the start of the result set.
        offset: u32,
        /// The maximum number of items per page.
        limit: u32,
        /// The total number of items across all pages.
        total: u32,
    },
    /// Pagination with only knowledge of whether more items exist.
    WithHasMore {
        /// The items in this page.
        items: Vec<T>,
        /// The offset of this page from the start of the result set.
        offset: u32,
        /// The maximum number of items per page.
        limit: u32,
        /// Whether there are more items available after this page.
        has_more: bool,
    },
}

impl<T> Page<T> {
    /// Creates an empty page with zero items, offset, limit, and total.
    #[must_use]
    pub const fn empty() -> Self {
        Self::WithTotal {
            items: vec![],
            offset: 0,
            limit: 0,
            total: 0,
        }
    }
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
    /// Deserializes a [`Page`] from a format supporting both total-based and cursor-based pagination.
    ///
    /// # Errors
    ///
    /// * If deserialization fails
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
    /// Serializes a [`Page`] in camelCase format with pagination metadata.
    ///
    /// # Errors
    ///
    /// * If serialization fails
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

impl<T, E> Page<Result<T, E>> {
    /// Transposes a `Page<Result<T, E>>` into a `Result<Page<T>, E>`.
    ///
    /// # Errors
    ///
    /// * If any of the items are `Err`, they will bubble up to the top-level
    pub fn transpose(self) -> Result<Page<T>, E>
    where
        T: 'static,
    {
        Ok(match self {
            Self::WithTotal {
                items,
                offset,
                limit,
                total,
            } => Page::WithTotal {
                items: items.into_iter().collect::<Result<Vec<_>, _>>()?,
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
                items: items.into_iter().collect::<Result<Vec<_>, _>>()?,
                offset,
                limit,
                has_more,
            },
        })
    }
}

impl<T> Page<T> {
    /// Returns the offset of this page.
    #[must_use]
    pub const fn offset(&self) -> u32 {
        match self {
            Self::WithTotal { offset, .. } | Self::WithHasMore { offset, .. } => *offset,
        }
    }

    /// Returns the limit of this page.
    #[must_use]
    pub const fn limit(&self) -> u32 {
        match self {
            Self::WithTotal { limit, .. } | Self::WithHasMore { limit, .. } => *limit,
        }
    }

    /// Returns whether there are more items available after this page.
    ///
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

    /// Returns the total number of items across all pages, if known.
    #[must_use]
    pub const fn total(&self) -> Option<u32> {
        match self {
            Self::WithTotal { total, .. } => Some(*total),
            Self::WithHasMore { .. } => None,
        }
    }

    /// Returns the number of remaining items after this page, if the total is known.
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

    /// Returns a slice of the items in this page.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn items(&self) -> &[T] {
        match self {
            Self::WithTotal { items, .. } | Self::WithHasMore { items, .. } => items,
        }
    }

    /// Consumes this page and returns the items as a `Vec`.
    #[must_use]
    pub fn into_items(self) -> Vec<T> {
        match self {
            Self::WithTotal { items, .. } | Self::WithHasMore { items, .. } => items,
        }
    }

    /// Transforms each item in this page using the provided function.
    ///
    /// This method consumes the page and returns a new page with transformed items,
    /// preserving the pagination metadata (offset, limit, `total`/`has_more`).
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

    /// Converts the items in this page into a different type using `Into`.
    ///
    /// This method consumes the page and returns a new page with items converted to type `TU`,
    /// preserving the pagination metadata (offset, limit, `total`/`has_more`).
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

    /// Attempts to convert the items in this page into a different type using `TryInto`.
    ///
    /// This method consumes the page and returns a new page with items converted to type `TU`,
    /// preserving the pagination metadata (offset, limit, `total`/`has_more`).
    ///
    /// # Errors
    ///
    /// * If any of the items fail to `try_into`
    pub fn try_into<TU>(self) -> Result<Page<TU>, T::Error>
    where
        T: TryInto<TU> + 'static,
    {
        Ok(match self {
            Self::WithTotal {
                items,
                offset,
                limit,
                total,
            } => Page::WithTotal {
                items: items
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
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
                items: items
                    .into_iter()
                    .map(TryInto::try_into)
                    .collect::<Result<Vec<_>, _>>()?,
                offset,
                limit,
                has_more,
            },
        })
    }
}

/// A request for a specific page of results.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PagingRequest {
    /// The offset from the start of the result set.
    pub offset: u32,
    /// The maximum number of items to return.
    pub limit: u32,
}

/// A boxed future that resolves to a [`PagingResult`].
type FuturePagingResponse<T, E> = Pin<Box<dyn Future<Output = PagingResult<T, E>> + Send>>;

/// A boxed function that fetches a page given an offset and limit.
type FetchPagingResponse<T, E> = Box<dyn FnMut(u32, u32) -> FuturePagingResponse<T, E> + Send>;

/// A paginated response containing a page of items and a function to fetch additional pages.
pub struct PagingResponse<T, E> {
    /// The current page of items.
    pub page: Page<T>,
    /// A function to fetch additional pages.
    pub fetch: Arc<Mutex<FetchPagingResponse<T, E>>>,
}

impl<T: std::fmt::Debug, E> std::fmt::Debug for PagingResponse<T, E> {
    /// Formats the [`PagingResponse`] for debug output, showing only the page.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PagingResponse")
            .field("page", &self.page)
            .finish_non_exhaustive()
    }
}

impl<T: Send, E: Send> PagingResponse<Result<T, E>, E> {
    /// Transposes a `PagingResponse<Result<T, E>, E>` into a `Result<PagingResponse<T, E>, E>`.
    ///
    /// # Errors
    ///
    /// * If any of the items are `Err`, they will bubble up to the top-level
    pub fn transpose(self) -> Result<PagingResponse<T, E>, E>
    where
        T: 'static,
        E: 'static,
    {
        let page = self.page.transpose()?;

        let fetch = self.fetch;

        Ok(PagingResponse {
            page,
            fetch: Arc::new(Mutex::new(Box::new(move |offset, count| {
                let fetch = fetch.clone();

                let closure = async move {
                    let mut fetch = fetch.lock().await;
                    fetch(offset, count).await.map(Self::transpose)?
                };

                Box::pin(closure)
            }))),
        })
    }
}

impl<T: Send, E: Send> PagingResponse<T, E> {
    /// Creates a new `PagingResponse` with the given page and fetch function.
    #[must_use]
    pub fn new(
        page: Page<T>,
        fetch: impl FnMut(u32, u32) -> FuturePagingResponse<T, E> + Send + 'static,
    ) -> Self {
        Self {
            page,
            fetch: Arc::new(Mutex::new(Box::new(fetch))),
        }
    }

    /// Creates an empty `PagingResponse` with zero items.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            page: Page::WithTotal {
                items: vec![],
                offset: 0,
                limit: 0,
                total: 0,
            },
            fetch: Arc::new(Mutex::new(Box::new(move |_offset, _count| {
                Box::pin(async move { Ok(Self::empty()) })
            }))),
        }
    }

    /// Fetches all remaining pages concurrently in batches.
    ///
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

    /// Fetches all remaining items concurrently in batches, excluding the current page.
    ///
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

    /// Fetches all pages concurrently in batches, including the current page.
    ///
    /// # Errors
    ///
    /// * If failed to fetch any of the subsequent `Page`s
    pub async fn with_rest_of_pages_in_batches(self) -> Result<Vec<Page<T>>, E> {
        self.rest_of_pages_in_batches_inner(true).await
    }

    /// Fetches all items concurrently in batches, including the current page.
    ///
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

    /// Fetches all remaining pages sequentially, excluding the current page.
    ///
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

    /// Fetches all remaining items sequentially, excluding the current page.
    ///
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

    /// Fetches all pages sequentially, including the current page.
    ///
    /// # Errors
    ///
    /// * If failed to fetch any of the subsequent `Page`s
    pub async fn with_rest_of_pages(self) -> Result<Vec<Page<T>>, E> {
        self.rest_of_pages_inner(true).await
    }

    /// Fetches all items sequentially, including the current page.
    ///
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

    /// Returns the offset of the current page.
    #[must_use]
    pub const fn offset(&self) -> u32 {
        self.page.offset()
    }

    /// Returns the limit of the current page.
    #[must_use]
    pub const fn limit(&self) -> u32 {
        self.page.limit()
    }

    /// Returns whether there are more items available.
    #[must_use]
    pub fn has_more(&self) -> bool {
        self.page.has_more()
    }

    /// Returns the total number of items across all pages, if known.
    #[must_use]
    pub const fn total(&self) -> Option<u32> {
        self.page.total()
    }

    /// Returns a slice of the items in the current page.
    #[must_use]
    pub fn items(&self) -> &[T] {
        self.page.items()
    }

    /// Consumes this response and returns the items from the current page as a `Vec`.
    #[must_use]
    pub fn into_items(self) -> Vec<T> {
        self.page.into_items()
    }

    /// Maps the items in this response and all future pages using the provided function.
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

    /// Maps the error type in this response and all future pages using the provided function.
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

    /// Converts both the item and error types using `Into`.
    ///
    /// This method transforms the current page and all future pages retrieved via the fetch function,
    /// converting items from type `T` to `TU` and errors from type `E` to `EU`.
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

    /// Attempts to convert both the item and error types using `TryInto` and `Into`.
    ///
    /// This method transforms the current page and all future pages retrieved via the fetch function,
    /// attempting to convert items from type `T` to `TU` using `TryInto`, and converting errors from
    /// type `E` to `EU` using `Into`. Conversion errors from `T::Error` are also converted to `EU`.
    ///
    /// # Errors
    ///
    /// * If the `try_into` call fails
    pub fn inner_try_into<TU: Send + 'static, EU: Send + 'static>(
        self,
    ) -> Result<PagingResponse<TU, EU>, T::Error>
    where
        T: TryInto<TU> + Send + 'static,
        T::Error: Into<EU>,
        E: Into<EU> + Send + 'static,
    {
        let page = self.page.try_into()?;

        let fetch = self.fetch;

        Ok(PagingResponse {
            page,
            fetch: Arc::new(Mutex::new(Box::new(move |offset, count| {
                let fetch = fetch.clone();

                let closure = async move {
                    let x: Result<Self, E> = {
                        let mut fetch = fetch.lock().await;
                        fetch(offset, count).await
                    };

                    let x: Result<Result<PagingResponse<TU, EU>, T::Error>, E> =
                        x.map(Self::inner_try_into);

                    let x: Result<Result<PagingResponse<TU, EU>, T::Error>, EU> =
                        x.map_err(Into::into);

                    let x: Result<Result<PagingResponse<TU, EU>, EU>, EU> = match x {
                        Ok(x) => match x {
                            Ok(x) => Ok(Ok(x)),
                            Err(e) => Err(e.into()),
                        },
                        Err(e) => Ok(Err(e)),
                    };

                    match x {
                        Ok(x) => match x {
                            Ok(x) => Ok(x),
                            Err(e) => Err(e),
                        },
                        Err(e) => Err(e),
                    }
                };

                Box::pin(closure)
            }))),
        })
    }

    /// Attempts to convert both the item and error types, mapping conversion errors with a custom function.
    ///
    /// This method transforms the current page and all future pages retrieved via the fetch function,
    /// attempting to convert items from type `T` to `TU` using `TryInto`, and converting errors from
    /// type `E` to `EU` using `Into`. Conversion errors from `T::Error` are mapped to `EU` using the
    /// provided `map_err` function.
    ///
    /// # Errors
    ///
    /// * If the `try_into` call fails
    pub fn inner_try_into_map_err<TU: Send + 'static, EU: Send + 'static, F>(
        self,
        map_err: F,
    ) -> Result<PagingResponse<TU, EU>, EU>
    where
        T: TryInto<TU> + Send + 'static,
        E: Into<EU> + Send + 'static,
        F: FnMut(T::Error) -> EU + Send + Clone + 'static,
    {
        let page = self.page.try_into().map_err(map_err.clone())?;

        let fetch = self.fetch;

        Ok(PagingResponse {
            page,
            fetch: Arc::new(Mutex::new(Box::new(move |offset, count| {
                let fetch = fetch.clone();
                let map_err = map_err.clone();

                let closure = async move {
                    let x: Result<Self, E> = {
                        let mut fetch = fetch.lock().await;
                        fetch(offset, count).await
                    };

                    let x: Result<Result<PagingResponse<TU, EU>, EU>, E> =
                        x.map(|e| Self::inner_try_into_map_err(e, map_err));

                    let x: Result<Result<PagingResponse<TU, EU>, EU>, EU> = x.map_err(Into::into);

                    let x: Result<Result<PagingResponse<TU, EU>, EU>, EU> = match x {
                        Ok(x) => match x {
                            Ok(x) => Ok(Ok(x)),
                            Err(e) => Err(e),
                        },
                        Err(e) => Ok(Err(e)),
                    };

                    match x {
                        Ok(x) => match x {
                            Ok(x) => Ok(x),
                            Err(e) => Err(e),
                        },
                        Err(e) => Err(e),
                    }
                };

                Box::pin(closure)
            }))),
        })
    }

    /// Converts the item type using `Into`, leaving the error type unchanged.
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

    /// Attempts to convert the item type using `TryInto`, leaving the error type unchanged.
    ///
    /// This method transforms the current page and all future pages retrieved via the fetch function,
    /// attempting to convert items from type `T` to `TU` using `TryInto`. The error type `E` remains unchanged.
    /// Conversion errors from `T::Error` are converted to `E` using `Into`.
    ///
    /// # Errors
    ///
    /// * If the `try_into` call fails
    pub fn ok_try_into<TU: Send + 'static>(self) -> Result<PagingResponse<TU, E>, T::Error>
    where
        T: TryInto<TU> + Send + 'static,
        T::Error: Into<E>,
        E: Send + 'static,
    {
        let page = self.page.try_into()?;

        let fetch = self.fetch;

        Ok(PagingResponse {
            page,
            fetch: Arc::new(Mutex::new(Box::new(move |offset, count| {
                let fetch = fetch.clone();

                let closure = async move {
                    let x: Result<Self, E> = {
                        let mut fetch = fetch.lock().await;
                        fetch(offset, count).await
                    };

                    let x: Result<Result<PagingResponse<TU, E>, E>, E> =
                        x.map(|e| Self::ok_try_into(e).map_err(Into::into));

                    match x {
                        Ok(x) => match x {
                            Ok(x) => Ok(x),
                            Err(e) => Err(e),
                        },
                        Err(e) => Err(e),
                    }
                };

                Box::pin(closure)
            }))),
        })
    }

    /// Attempts to convert the item type, mapping conversion errors with a custom function.
    ///
    /// This method transforms the current page and all future pages retrieved via the fetch function,
    /// attempting to convert items from type `T` to `TU` using `TryInto`. The error type `E` remains unchanged.
    /// Conversion errors from `T::Error` are mapped to `E` using the provided `map_err` function.
    ///
    /// # Errors
    ///
    /// * If the `try_into` call fails
    pub fn ok_try_into_map_err<TU: Send + 'static, F>(
        self,
        map_err: F,
    ) -> Result<PagingResponse<TU, E>, E>
    where
        T: TryInto<TU> + Send + 'static,
        E: Send + 'static,
        F: FnMut(T::Error) -> E + Send + Clone + 'static,
    {
        let page = self.page.try_into().map_err(map_err.clone())?;

        let fetch = self.fetch;

        Ok(PagingResponse {
            page,
            fetch: Arc::new(Mutex::new(Box::new(move |offset, count| {
                let fetch = fetch.clone();
                let map_err = map_err.clone();

                let closure = async move {
                    let x: Result<Self, E> = {
                        let mut fetch = fetch.lock().await;
                        fetch(offset, count).await
                    };

                    let x: Result<Result<PagingResponse<TU, E>, E>, E> =
                        x.map(|e| Self::ok_try_into_map_err(e, map_err));

                    match x {
                        Ok(x) => match x {
                            Ok(x) => Ok(x),
                            Err(e) => Err(e),
                        },
                        Err(e) => Err(e),
                    }
                };

                Box::pin(closure)
            }))),
        })
    }

    /// Converts the error type using `Into`, leaving the item type unchanged.
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

    /// Returns a reference to the current page.
    fn deref(&self) -> &Self::Target {
        &self.page
    }
}

impl<T> Deref for Page<T> {
    type Target = Vec<T>;

    /// Returns a reference to the items in this page as a `Vec`.
    fn deref(&self) -> &Self::Target {
        match self {
            Self::WithTotal { items, .. } | Self::WithHasMore { items, .. } => items,
        }
    }
}

impl<T, E> From<PagingResponse<T, E>> for Page<T> {
    /// Converts a [`PagingResponse`] into its underlying [`Page`].
    fn from(value: PagingResponse<T, E>) -> Self {
        value.page
    }
}

impl<T> From<Page<T>> for Vec<T> {
    /// Converts a [`Page`] into a `Vec` of its items.
    fn from(value: Page<T>) -> Self {
        match value {
            Page::WithTotal { items, .. } | Page::WithHasMore { items, .. } => items,
        }
    }
}

/// A result type for paging operations.
pub type PagingResult<T, E> = Result<PagingResponse<T, E>, E>;
