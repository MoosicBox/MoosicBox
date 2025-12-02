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
use switchy_async::sync::Mutex;

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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // ===== Page Tests =====

    #[test_log::test]
    fn test_page_empty() {
        let page: Page<i32> = Page::empty();
        assert_eq!(page.offset(), 0);
        assert_eq!(page.limit(), 0);
        assert_eq!(page.total(), Some(0));
        assert_eq!(page.items(), &[] as &[i32]);
        assert!(!page.has_more());
    }

    #[test_log::test]
    fn test_page_with_total_has_more_true() {
        let page = Page::WithTotal {
            items: vec![1, 2, 3],
            offset: 0,
            limit: 3,
            total: 10,
        };
        assert!(page.has_more());
        assert_eq!(page.total(), Some(10));
        assert_eq!(page.remaining(), Some(7));
    }

    #[test_log::test]
    fn test_page_with_total_has_more_false() {
        let page = Page::WithTotal {
            items: vec![1, 2, 3],
            offset: 7,
            limit: 3,
            total: 10,
        };
        assert!(!page.has_more());
        assert_eq!(page.total(), Some(10));
        assert_eq!(page.remaining(), Some(0));
    }

    #[test_log::test]
    fn test_page_with_has_more_true() {
        let page = Page::WithHasMore {
            items: vec![1, 2, 3],
            offset: 0,
            limit: 3,
            has_more: true,
        };
        assert!(page.has_more());
        assert_eq!(page.total(), None);
        assert_eq!(page.remaining(), None);
    }

    #[test_log::test]
    fn test_page_with_has_more_false() {
        let page = Page::WithHasMore {
            items: vec![1, 2, 3],
            offset: 0,
            limit: 3,
            has_more: false,
        };
        assert!(!page.has_more());
        assert_eq!(page.total(), None);
        assert_eq!(page.remaining(), None);
    }

    #[test_log::test]
    fn test_page_items_and_into_items() {
        let page = Page::WithTotal {
            items: vec![1, 2, 3],
            offset: 0,
            limit: 3,
            total: 10,
        };
        assert_eq!(page.items(), &[1, 2, 3]);

        let items = page.into_items();
        assert_eq!(items, vec![1, 2, 3]);
    }

    #[test_log::test]
    fn test_page_map_with_total() {
        let page = Page::WithTotal {
            items: vec![1, 2, 3],
            offset: 5,
            limit: 3,
            total: 20,
        };

        let mapped = page.map(|x| x * 2);

        assert_eq!(mapped.items(), &[2, 4, 6]);
        assert_eq!(mapped.offset(), 5);
        assert_eq!(mapped.limit(), 3);
        assert_eq!(mapped.total(), Some(20));
    }

    #[test_log::test]
    fn test_page_map_with_has_more() {
        let page = Page::WithHasMore {
            items: vec!["a", "b"],
            offset: 2,
            limit: 2,
            has_more: true,
        };

        let mapped = page.map(str::to_uppercase);

        assert_eq!(mapped.items(), &["A", "B"]);
        assert_eq!(mapped.offset(), 2);
        assert_eq!(mapped.limit(), 2);
        assert!(mapped.has_more());
    }

    #[test_log::test]
    fn test_page_into_conversion() {
        #[derive(Debug, PartialEq)]
        struct From(i32);
        #[derive(Debug, PartialEq)]
        struct To(i32);

        impl std::convert::From<From> for To {
            fn from(f: From) -> Self {
                Self(f.0 * 10)
            }
        }

        let page = Page::WithTotal {
            items: vec![From(1), From(2)],
            offset: 0,
            limit: 2,
            total: 5,
        };

        let converted: Page<To> = page.into();
        assert_eq!(converted.items(), &[To(10), To(20)]);
        assert_eq!(converted.total(), Some(5));
    }

    #[test_log::test]
    fn test_page_try_into_success() {
        #[derive(Debug, Clone)]
        struct Value(i32);

        impl TryInto<i32> for Value {
            type Error = String;

            fn try_into(self) -> Result<i32, Self::Error> {
                if self.0 >= 0 {
                    Ok(self.0)
                } else {
                    Err("negative".to_string())
                }
            }
        }

        let page = Page::WithTotal {
            items: vec![Value(1), Value(2)],
            offset: 0,
            limit: 2,
            total: 5,
        };

        let result: Result<Page<i32>, String> = page.try_into();
        assert!(result.is_ok());
        let converted = result.unwrap();
        assert_eq!(converted.items(), &[1, 2]);
    }

    #[test_log::test]
    fn test_page_try_into_failure() {
        #[derive(Debug, Clone)]
        struct Value(i32);

        impl TryInto<i32> for Value {
            type Error = String;

            fn try_into(self) -> Result<i32, Self::Error> {
                if self.0 >= 0 {
                    Ok(self.0)
                } else {
                    Err("negative".to_string())
                }
            }
        }

        let page = Page::WithTotal {
            items: vec![Value(1), Value(-1)],
            offset: 0,
            limit: 2,
            total: 5,
        };

        let result: Result<Page<i32>, String> = page.try_into();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "negative");
    }

    #[test_log::test]
    fn test_page_transpose_success() {
        let page = Page::WithTotal {
            items: vec![Ok(1), Ok(2), Ok(3)],
            offset: 0,
            limit: 3,
            total: 5,
        };

        let result: Result<Page<i32>, String> = page.transpose();
        assert!(result.is_ok());
        let transposed = result.unwrap();
        assert_eq!(transposed.items(), &[1, 2, 3]);
        assert_eq!(transposed.total(), Some(5));
    }

    #[test_log::test]
    fn test_page_transpose_failure() {
        let page = Page::WithTotal {
            items: vec![Ok(1), Err("error"), Ok(3)],
            offset: 0,
            limit: 3,
            total: 5,
        };

        let result: Result<Page<i32>, &str> = page.transpose();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "error");
    }

    #[test_log::test]
    fn test_page_transpose_with_has_more() {
        let page = Page::WithHasMore {
            items: vec![Ok(1), Ok(2)],
            offset: 2,
            limit: 2,
            has_more: true,
        };

        let result: Result<Page<i32>, String> = page.transpose();
        assert!(result.is_ok());
        let transposed = result.unwrap();
        assert_eq!(transposed.items(), &[1, 2]);
        assert!(transposed.has_more());
        assert_eq!(transposed.total(), None);
    }

    #[test_log::test]
    fn test_page_serialization_with_total() {
        let page = Page::WithTotal {
            items: vec![1, 2, 3],
            offset: 0,
            limit: 3,
            total: 10,
        };

        let json = serde_json::to_string(&page).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["items"], serde_json::json!([1, 2, 3]));
        assert_eq!(parsed["offset"], 0);
        assert_eq!(parsed["limit"], 3);
        assert_eq!(parsed["total"], 10);
        assert_eq!(parsed["hasMore"], true);
    }

    #[test_log::test]
    fn test_page_serialization_with_has_more() {
        let page = Page::WithHasMore {
            items: vec!["a", "b"],
            offset: 5,
            limit: 2,
            has_more: false,
        };

        let json = serde_json::to_string(&page).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["items"], serde_json::json!(["a", "b"]));
        assert_eq!(parsed["offset"], 5);
        assert_eq!(parsed["limit"], 2);
        assert_eq!(parsed["hasMore"], false);
        assert!(parsed["total"].is_null());
    }

    #[test_log::test]
    fn test_page_deserialization_with_total() {
        let json = r#"{"items":[1,2,3],"offset":0,"limit":3,"total":10,"hasMore":true}"#;

        let page: Page<i32> = serde_json::from_str(json).unwrap();

        assert_eq!(page.items(), &[1, 2, 3]);
        assert_eq!(page.offset(), 0);
        assert_eq!(page.limit(), 3);
        assert_eq!(page.total(), Some(10));
        assert!(page.has_more());
    }

    #[test_log::test]
    fn test_page_deserialization_without_total() {
        let json = r#"{"items":["x","y"],"offset":2,"limit":2,"hasMore":false}"#;

        let page: Page<String> = serde_json::from_str(json).unwrap();

        assert_eq!(page.items(), &["x", "y"]);
        assert_eq!(page.offset(), 2);
        assert_eq!(page.limit(), 2);
        assert_eq!(page.total(), None);
        assert!(!page.has_more());
    }

    #[test_log::test]
    fn test_page_deref() {
        let page = Page::WithTotal {
            items: vec![1, 2, 3],
            offset: 0,
            limit: 3,
            total: 5,
        };

        // Deref should give us access to Vec methods
        let vec_ref: &Vec<i32> = &page;
        assert_eq!(vec_ref.len(), 3);
        assert_eq!(vec_ref[0], 1);
    }

    #[test_log::test]
    fn test_page_into_vec() {
        let page = Page::WithTotal {
            items: vec![1, 2, 3],
            offset: 0,
            limit: 3,
            total: 5,
        };

        let vec: Vec<i32> = Vec::from(page);
        assert_eq!(vec, vec![1, 2, 3]);
    }

    // ===== PagingResponse Tests =====

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_empty() {
        let response: PagingResponse<i32, String> = PagingResponse::empty();

        assert_eq!(response.offset(), 0);
        assert_eq!(response.limit(), 0);
        assert_eq!(response.total(), Some(0));
        assert_eq!(response.items(), &[] as &[i32]);
        assert!(!response.has_more());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_basic_properties() {
        let page = Page::WithTotal {
            items: vec![1, 2, 3],
            offset: 0,
            limit: 3,
            total: 10,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |_offset, _limit| {
            Box::pin(async { Ok(PagingResponse::empty()) })
        });

        assert_eq!(response.offset(), 0);
        assert_eq!(response.limit(), 3);
        assert_eq!(response.total(), Some(10));
        assert_eq!(response.items(), &[1, 2, 3]);
        assert!(response.has_more());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_into_items() {
        let page = Page::WithTotal {
            items: vec![1, 2, 3],
            offset: 0,
            limit: 3,
            total: 5,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |_offset, _limit| {
            Box::pin(async { Ok(PagingResponse::empty()) })
        });

        let items = response.into_items();
        assert_eq!(items, vec![1, 2, 3]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_rest_of_pages_sequential() {
        let page1 = Page::WithTotal {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            total: 6,
        };

        let response = PagingResponse::new(page1, |offset, _limit| {
            Box::pin(async move {
                match offset {
                    2 => Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![3, 4],
                            offset: 2,
                            limit: 2,
                            total: 6,
                        },
                        |off, _lim| {
                            Box::pin(async move {
                                if off == 4 {
                                    Ok(PagingResponse::new(
                                        Page::WithTotal {
                                            items: vec![5, 6],
                                            offset: 4,
                                            limit: 2,
                                            total: 6,
                                        },
                                        |_, _| {
                                            Box::pin(async {
                                                Ok(PagingResponse::<i32, String>::empty())
                                            })
                                        },
                                    ))
                                } else {
                                    Ok(PagingResponse::empty())
                                }
                            })
                        },
                    )),
                    4 => Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![5, 6],
                            offset: 4,
                            limit: 2,
                            total: 6,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    )),
                    _ => Ok(PagingResponse::empty()),
                }
            })
        });

        let pages = response.rest_of_pages().await.unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].items(), &[3, 4]);
        assert_eq!(pages[1].items(), &[5, 6]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_with_rest_of_pages() {
        let page = Page::WithTotal {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            total: 4,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |_offset, _limit| {
            Box::pin(async {
                Ok(PagingResponse::new(
                    Page::WithTotal {
                        items: vec![3, 4],
                        offset: 2,
                        limit: 2,
                        total: 4,
                    },
                    |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                ))
            })
        });

        let pages = response.with_rest_of_pages().await.unwrap();
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].items(), &[1, 2]); // Includes initial page
        assert_eq!(pages[1].items(), &[3, 4]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_rest_of_items() {
        let page = Page::WithTotal {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            total: 4,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |_offset, _limit| {
            Box::pin(async {
                Ok(PagingResponse::new(
                    Page::WithTotal {
                        items: vec![3, 4],
                        offset: 2,
                        limit: 2,
                        total: 4,
                    },
                    |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                ))
            })
        });

        let items = response.rest_of_items().await.unwrap();
        assert_eq!(items, vec![3, 4]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_with_rest_of_items() {
        let page = Page::WithTotal {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            total: 4,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |_offset, _limit| {
            Box::pin(async {
                Ok(PagingResponse::new(
                    Page::WithTotal {
                        items: vec![3, 4],
                        offset: 2,
                        limit: 2,
                        total: 4,
                    },
                    |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                ))
            })
        });

        let items = response.with_rest_of_items().await.unwrap();
        assert_eq!(items, vec![1, 2, 3, 4]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_rest_of_pages_in_batches() {
        let page = Page::WithTotal {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            total: 6,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |offset, _limit| {
            Box::pin(async move {
                match offset {
                    2 => Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![3, 4],
                            offset: 2,
                            limit: 2,
                            total: 6,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    )),
                    4 => Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![5, 6],
                            offset: 4,
                            limit: 2,
                            total: 6,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    )),
                    _ => Ok(PagingResponse::empty()),
                }
            })
        });

        let pages = response.rest_of_pages_in_batches().await.unwrap();
        assert_eq!(pages.len(), 2);
        // Order may vary in concurrent execution, but all items should be present
        let all_items: Vec<i32> = pages
            .into_iter()
            .flat_map(super::Page::into_items)
            .collect();
        assert_eq!(all_items.len(), 4);
        assert!(all_items.contains(&3));
        assert!(all_items.contains(&4));
        assert!(all_items.contains(&5));
        assert!(all_items.contains(&6));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_with_rest_of_items_in_batches() {
        let page = Page::WithTotal {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            total: 6,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |offset, _limit| {
            Box::pin(async move {
                match offset {
                    2 => Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![3, 4],
                            offset: 2,
                            limit: 2,
                            total: 6,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    )),
                    4 => Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![5, 6],
                            offset: 4,
                            limit: 2,
                            total: 6,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    )),
                    _ => Ok(PagingResponse::empty()),
                }
            })
        });

        let items = response.with_rest_of_items_in_batches().await.unwrap();
        assert_eq!(items.len(), 6);
        assert_eq!(&items[0..2], &[1, 2]); // Initial page is first
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_map() {
        let page = Page::WithTotal {
            items: vec![1, 2, 3],
            offset: 0,
            limit: 3,
            total: 6,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |_offset, _limit| {
            Box::pin(async {
                Ok(PagingResponse::new(
                    Page::WithTotal {
                        items: vec![4, 5, 6],
                        offset: 3,
                        limit: 3,
                        total: 6,
                    },
                    |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                ))
            })
        });

        let mapped = response.map(|x| x * 2);
        assert_eq!(mapped.items(), &[2, 4, 6]);

        // Test that mapping applies to fetched pages too
        let all_items = mapped.with_rest_of_items().await.unwrap();
        assert_eq!(all_items, vec![2, 4, 6, 8, 10, 12]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_map_err() {
        let page = Page::WithTotal {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            total: 4,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |_offset, _limit| {
            Box::pin(async { Err("original error".to_string()) })
        });

        let mapped = response.map_err(|e| format!("wrapped: {e}"));

        let result = mapped.rest_of_items().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "wrapped: original error");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_transpose_success() {
        let page = Page::WithTotal {
            items: vec![Ok(1), Ok(2)],
            offset: 0,
            limit: 2,
            total: 4,
        };

        let response: PagingResponse<Result<i32, String>, String> =
            PagingResponse::new(page, |_offset, _limit| {
                Box::pin(async {
                    Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![Ok(3), Ok(4)],
                            offset: 2,
                            limit: 2,
                            total: 4,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    ))
                })
            });

        let transposed = response.transpose().unwrap();
        assert_eq!(transposed.items(), &[1, 2]);

        let all = transposed.with_rest_of_items().await.unwrap();
        assert_eq!(all, vec![1, 2, 3, 4]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_transpose_failure() {
        let page = Page::WithTotal {
            items: vec![Ok(1), Err("error")],
            offset: 0,
            limit: 2,
            total: 2,
        };

        let response: PagingResponse<Result<i32, &str>, &str> =
            PagingResponse::new(page, |_offset, _limit| {
                Box::pin(async { Ok(PagingResponse::empty()) })
            });

        let result = response.transpose();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "error");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_ok_into() {
        #[derive(Debug, PartialEq)]
        struct From(i32);
        #[derive(Debug, PartialEq)]
        struct To(i32);

        impl std::convert::From<From> for To {
            fn from(f: From) -> Self {
                Self(f.0 * 10)
            }
        }

        let page = Page::WithTotal {
            items: vec![From(1), From(2)],
            offset: 0,
            limit: 2,
            total: 2,
        };

        let response: PagingResponse<From, String> =
            PagingResponse::new(page, |_offset, _limit| {
                Box::pin(async { Ok(PagingResponse::empty()) })
            });

        let converted: PagingResponse<To, String> = response.ok_into();
        assert_eq!(converted.items(), &[To(10), To(20)]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_err_into() {
        #[derive(Debug, PartialEq)]
        struct ErrorA(String);
        #[derive(Debug, PartialEq)]
        struct ErrorB(String);

        impl std::convert::From<ErrorA> for ErrorB {
            fn from(e: ErrorA) -> Self {
                Self(format!("converted: {}", e.0))
            }
        }

        let page = Page::WithTotal {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            total: 4,
        };

        let response: PagingResponse<i32, ErrorA> = PagingResponse::new(page, |_offset, _limit| {
            Box::pin(async { Err(ErrorA("test error".to_string())) })
        });

        let converted: PagingResponse<i32, ErrorB> = response.err_into();

        let result = converted.rest_of_items().await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            ErrorB("converted: test error".to_string())
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_deref() {
        let page = Page::WithTotal {
            items: vec![1, 2, 3],
            offset: 0,
            limit: 3,
            total: 5,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |_offset, _limit| {
            Box::pin(async { Ok(PagingResponse::empty()) })
        });

        // Deref should give us access to Page
        let page_ref: &Page<i32> = &response;
        assert_eq!(page_ref.items(), &[1, 2, 3]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_into_page() {
        let page = Page::WithTotal {
            items: vec![1, 2, 3],
            offset: 0,
            limit: 3,
            total: 5,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |_offset, _limit| {
            Box::pin(async { Ok(PagingResponse::empty()) })
        });

        let converted_page: Page<i32> = response.into();
        assert_eq!(converted_page.items(), &[1, 2, 3]);
        assert_eq!(converted_page.total(), Some(5));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_rest_of_pages_with_has_more() {
        // Test that cursor-based pagination (WithHasMore) uses sequential fetch
        let page = Page::WithHasMore {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            has_more: true,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |offset, _limit| {
            Box::pin(async move {
                if offset == 2 {
                    Ok(PagingResponse::new(
                        Page::WithHasMore {
                            items: vec![3, 4],
                            offset: 2,
                            limit: 2,
                            has_more: false,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    ))
                } else {
                    Ok(PagingResponse::empty())
                }
            })
        });

        let pages = response.rest_of_pages().await.unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].items(), &[3, 4]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_error_propagation() {
        let page = Page::WithTotal {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            total: 6,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |offset, _limit| {
            Box::pin(async move {
                if offset == 2 {
                    Err("fetch failed".to_string())
                } else {
                    Ok(PagingResponse::empty())
                }
            })
        });

        let result = response.rest_of_items().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "fetch failed");
    }

    // ===== PagingRequest Tests =====

    #[test_log::test]
    fn test_paging_request_serialization() {
        let request = PagingRequest {
            offset: 10,
            limit: 20,
        };

        let json = serde_json::to_string(&request).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["offset"], 10);
        assert_eq!(parsed["limit"], 20);
    }

    #[test_log::test]
    fn test_paging_request_deserialization() {
        let json = r#"{"offset":5,"limit":15}"#;

        let request: PagingRequest = serde_json::from_str(json).unwrap();

        assert_eq!(request.offset, 5);
        assert_eq!(request.limit, 15);
    }

    #[test_log::test]
    fn test_paging_request_equality() {
        let req1 = PagingRequest {
            offset: 10,
            limit: 20,
        };
        let req2 = PagingRequest {
            offset: 10,
            limit: 20,
        };
        let req3 = PagingRequest {
            offset: 10,
            limit: 30,
        };

        assert_eq!(req1, req2);
        assert_ne!(req1, req3);
    }

    // ===== Additional Page Tests =====

    #[test_log::test]
    fn test_page_try_into_with_has_more_success() {
        #[derive(Debug, Clone)]
        struct Value(i32);

        impl TryInto<i32> for Value {
            type Error = String;

            fn try_into(self) -> Result<i32, Self::Error> {
                if self.0 >= 0 {
                    Ok(self.0)
                } else {
                    Err("negative".to_string())
                }
            }
        }

        let page = Page::WithHasMore {
            items: vec![Value(1), Value(2)],
            offset: 5,
            limit: 2,
            has_more: true,
        };

        let result: Result<Page<i32>, String> = page.try_into();
        assert!(result.is_ok());
        let converted = result.unwrap();
        assert_eq!(converted.items(), &[1, 2]);
        assert_eq!(converted.offset(), 5);
        assert_eq!(converted.limit(), 2);
        assert!(converted.has_more());
    }

    #[test_log::test]
    fn test_page_try_into_with_has_more_failure() {
        #[derive(Debug, Clone)]
        struct Value(i32);

        impl TryInto<i32> for Value {
            type Error = String;

            fn try_into(self) -> Result<i32, Self::Error> {
                if self.0 >= 0 {
                    Ok(self.0)
                } else {
                    Err("negative".to_string())
                }
            }
        }

        let page = Page::WithHasMore {
            items: vec![Value(1), Value(-5)],
            offset: 0,
            limit: 2,
            has_more: false,
        };

        let result: Result<Page<i32>, String> = page.try_into();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "negative");
    }

    #[test_log::test]
    fn test_page_into_with_has_more() {
        #[derive(Debug, PartialEq)]
        struct From(i32);
        #[derive(Debug, PartialEq)]
        struct To(i32);

        impl std::convert::From<From> for To {
            fn from(f: From) -> Self {
                Self(f.0 * 10)
            }
        }

        let page = Page::WithHasMore {
            items: vec![From(1), From(2)],
            offset: 3,
            limit: 2,
            has_more: true,
        };

        let converted: Page<To> = page.into();
        assert_eq!(converted.items(), &[To(10), To(20)]);
        assert_eq!(converted.offset(), 3);
        assert_eq!(converted.limit(), 2);
        assert!(converted.has_more());
        assert_eq!(converted.total(), None);
    }

    #[test_log::test]
    fn test_page_transpose_with_has_more_failure() {
        let page = Page::WithHasMore {
            items: vec![Ok(1), Err("error in has_more page")],
            offset: 2,
            limit: 2,
            has_more: false,
        };

        let result: Result<Page<i32>, &str> = page.transpose();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "error in has_more page");
    }

    // ===== Additional PagingResponse Tests =====

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_rest_of_items_in_batches() {
        let page = Page::WithTotal {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            total: 6,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |offset, _limit| {
            Box::pin(async move {
                match offset {
                    2 => Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![3, 4],
                            offset: 2,
                            limit: 2,
                            total: 6,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    )),
                    4 => Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![5, 6],
                            offset: 4,
                            limit: 2,
                            total: 6,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    )),
                    _ => Ok(PagingResponse::empty()),
                }
            })
        });

        let items = response.rest_of_items_in_batches().await.unwrap();
        // Should contain items from remaining pages (3,4,5,6) but not initial (1,2)
        assert_eq!(items.len(), 4);
        assert!(items.contains(&3));
        assert!(items.contains(&4));
        assert!(items.contains(&5));
        assert!(items.contains(&6));
        assert!(!items.contains(&1));
        assert!(!items.contains(&2));
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_with_rest_of_pages_in_batches() {
        let page = Page::WithTotal {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            total: 6,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |offset, _limit| {
            Box::pin(async move {
                match offset {
                    2 => Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![3, 4],
                            offset: 2,
                            limit: 2,
                            total: 6,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    )),
                    4 => Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![5, 6],
                            offset: 4,
                            limit: 2,
                            total: 6,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    )),
                    _ => Ok(PagingResponse::empty()),
                }
            })
        });

        let pages = response.with_rest_of_pages_in_batches().await.unwrap();
        assert_eq!(pages.len(), 3);
        // First page should be the initial page
        assert_eq!(pages[0].items(), &[1, 2]);
        // Remaining pages should include all items (order may vary for concurrent fetch)
        let all_items: Vec<i32> = pages
            .into_iter()
            .flat_map(super::Page::into_items)
            .collect();
        assert_eq!(all_items.len(), 6);
        for i in 1..=6 {
            assert!(all_items.contains(&i));
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_inner_into() {
        #[derive(Debug, PartialEq, Clone)]
        struct ItemFrom(i32);
        #[derive(Debug, PartialEq)]
        struct ItemTo(i32);
        #[derive(Debug, PartialEq, Clone)]
        struct ErrorFrom(String);
        #[derive(Debug, PartialEq)]
        struct ErrorTo(String);

        impl From<ItemFrom> for ItemTo {
            fn from(f: ItemFrom) -> Self {
                Self(f.0 * 10)
            }
        }

        impl From<ErrorFrom> for ErrorTo {
            fn from(e: ErrorFrom) -> Self {
                Self(format!("converted: {}", e.0))
            }
        }

        let page = Page::WithTotal {
            items: vec![ItemFrom(1), ItemFrom(2)],
            offset: 0,
            limit: 2,
            total: 4,
        };

        let response: PagingResponse<ItemFrom, ErrorFrom> =
            PagingResponse::new(page, |_offset, _limit| {
                Box::pin(async {
                    Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![ItemFrom(3), ItemFrom(4)],
                            offset: 2,
                            limit: 2,
                            total: 4,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    ))
                })
            });

        let converted: PagingResponse<ItemTo, ErrorTo> = response.inner_into();
        assert_eq!(converted.items(), &[ItemTo(10), ItemTo(20)]);

        // Verify the fetch function is also transformed
        let all_items = converted.with_rest_of_items().await.unwrap();
        assert_eq!(
            all_items,
            vec![ItemTo(10), ItemTo(20), ItemTo(30), ItemTo(40)]
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_inner_into_error_conversion() {
        #[derive(Debug, PartialEq, Clone)]
        struct ErrorFrom(String);
        #[derive(Debug, PartialEq)]
        struct ErrorTo(String);

        impl From<ErrorFrom> for ErrorTo {
            fn from(e: ErrorFrom) -> Self {
                Self(format!("converted: {}", e.0))
            }
        }

        let page = Page::WithTotal {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            total: 4,
        };

        let response: PagingResponse<i32, ErrorFrom> =
            PagingResponse::new(page, |_offset, _limit| {
                Box::pin(async { Err(ErrorFrom("original error".to_string())) })
            });

        let converted: PagingResponse<i32, ErrorTo> = response.inner_into();

        let result = converted.rest_of_items().await;
        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            ErrorTo("converted: original error".to_string())
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_inner_try_into_success() {
        #[derive(Debug, Clone)]
        struct ItemFrom(i32);
        #[derive(Debug, PartialEq)]
        struct ItemTo(i32);
        #[derive(Debug, PartialEq, Clone)]
        struct ErrorFrom(String);
        #[derive(Debug, PartialEq)]
        struct ErrorTo(String);

        impl TryInto<ItemTo> for ItemFrom {
            type Error = String;

            fn try_into(self) -> Result<ItemTo, Self::Error> {
                if self.0 >= 0 {
                    Ok(ItemTo(self.0 * 10))
                } else {
                    Err("negative value".to_string())
                }
            }
        }

        impl From<String> for ErrorTo {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl From<ErrorFrom> for ErrorTo {
            fn from(e: ErrorFrom) -> Self {
                Self(e.0)
            }
        }

        let page = Page::WithTotal {
            items: vec![ItemFrom(1), ItemFrom(2)],
            offset: 0,
            limit: 2,
            total: 4,
        };

        let response: PagingResponse<ItemFrom, ErrorFrom> =
            PagingResponse::new(page, |_offset, _limit| {
                Box::pin(async {
                    Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![ItemFrom(3), ItemFrom(4)],
                            offset: 2,
                            limit: 2,
                            total: 4,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    ))
                })
            });

        let result: Result<PagingResponse<ItemTo, ErrorTo>, String> = response.inner_try_into();
        assert!(result.is_ok());
        let converted = result.unwrap();
        assert_eq!(converted.items(), &[ItemTo(10), ItemTo(20)]);

        let all_items = converted.with_rest_of_items().await.unwrap();
        assert_eq!(
            all_items,
            vec![ItemTo(10), ItemTo(20), ItemTo(30), ItemTo(40)]
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_inner_try_into_failure() {
        #[derive(Debug, Clone)]
        struct ItemFrom(i32);
        #[derive(Debug, PartialEq)]
        struct ItemTo(i32);

        impl TryInto<ItemTo> for ItemFrom {
            type Error = String;

            fn try_into(self) -> Result<ItemTo, Self::Error> {
                if self.0 >= 0 {
                    Ok(ItemTo(self.0 * 10))
                } else {
                    Err("negative value".to_string())
                }
            }
        }

        let page = Page::WithTotal {
            items: vec![ItemFrom(1), ItemFrom(-2)], // Second item will fail
            offset: 0,
            limit: 2,
            total: 2,
        };

        let response: PagingResponse<ItemFrom, String> =
            PagingResponse::new(page, |_, _| Box::pin(async { Ok(PagingResponse::empty()) }));

        let result: Result<PagingResponse<ItemTo, String>, String> = response.inner_try_into();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "negative value");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_inner_try_into_map_err_success() {
        #[derive(Debug, Clone)]
        struct ItemFrom(i32);
        #[derive(Debug, PartialEq)]
        struct ItemTo(i32);
        #[derive(Debug, PartialEq, Clone)]
        struct ErrorFrom(String);
        #[derive(Debug, PartialEq)]
        struct ErrorTo(String);

        impl TryInto<ItemTo> for ItemFrom {
            type Error = String;

            fn try_into(self) -> Result<ItemTo, Self::Error> {
                if self.0 >= 0 {
                    Ok(ItemTo(self.0 * 10))
                } else {
                    Err("negative value".to_string())
                }
            }
        }

        impl From<ErrorFrom> for ErrorTo {
            fn from(e: ErrorFrom) -> Self {
                Self(e.0)
            }
        }

        let page = Page::WithTotal {
            items: vec![ItemFrom(1), ItemFrom(2)],
            offset: 0,
            limit: 2,
            total: 2,
        };

        let response: PagingResponse<ItemFrom, ErrorFrom> =
            PagingResponse::new(page, |_, _| Box::pin(async { Ok(PagingResponse::empty()) }));

        let result: Result<PagingResponse<ItemTo, ErrorTo>, ErrorTo> =
            response.inner_try_into_map_err(|e| ErrorTo(format!("mapped: {e}")));

        assert!(result.is_ok());
        let converted = result.unwrap();
        assert_eq!(converted.items(), &[ItemTo(10), ItemTo(20)]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_inner_try_into_map_err_failure() {
        #[derive(Debug, Clone)]
        struct ItemFrom(i32);
        #[derive(Debug, PartialEq)]
        struct ItemTo(i32);
        #[derive(Debug, PartialEq, Clone)]
        struct ErrorFrom(String);
        #[derive(Debug, PartialEq)]
        struct ErrorTo(String);

        impl TryInto<ItemTo> for ItemFrom {
            type Error = String;

            fn try_into(self) -> Result<ItemTo, Self::Error> {
                if self.0 >= 0 {
                    Ok(ItemTo(self.0 * 10))
                } else {
                    Err("negative value".to_string())
                }
            }
        }

        impl From<ErrorFrom> for ErrorTo {
            fn from(e: ErrorFrom) -> Self {
                Self(e.0)
            }
        }

        let page = Page::WithTotal {
            items: vec![ItemFrom(1), ItemFrom(-2)],
            offset: 0,
            limit: 2,
            total: 2,
        };

        let response: PagingResponse<ItemFrom, ErrorFrom> =
            PagingResponse::new(page, |_, _| Box::pin(async { Ok(PagingResponse::empty()) }));

        let result: Result<PagingResponse<ItemTo, ErrorTo>, ErrorTo> =
            response.inner_try_into_map_err(|e| ErrorTo(format!("mapped: {e}")));

        assert!(result.is_err());
        assert_eq!(
            result.unwrap_err(),
            ErrorTo("mapped: negative value".to_string())
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_ok_try_into_success() {
        #[derive(Debug, Clone)]
        struct ItemFrom(i32);
        #[derive(Debug, PartialEq)]
        struct ItemTo(i32);

        impl TryInto<ItemTo> for ItemFrom {
            type Error = String;

            fn try_into(self) -> Result<ItemTo, Self::Error> {
                if self.0 >= 0 {
                    Ok(ItemTo(self.0 * 10))
                } else {
                    Err("negative value".to_string())
                }
            }
        }

        let page = Page::WithTotal {
            items: vec![ItemFrom(1), ItemFrom(2)],
            offset: 0,
            limit: 2,
            total: 4,
        };

        let response: PagingResponse<ItemFrom, String> =
            PagingResponse::new(page, |_offset, _limit| {
                Box::pin(async {
                    Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![ItemFrom(3), ItemFrom(4)],
                            offset: 2,
                            limit: 2,
                            total: 4,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    ))
                })
            });

        let result: Result<PagingResponse<ItemTo, String>, String> = response.ok_try_into();
        assert!(result.is_ok());
        let converted = result.unwrap();
        assert_eq!(converted.items(), &[ItemTo(10), ItemTo(20)]);

        let all_items = converted.with_rest_of_items().await.unwrap();
        assert_eq!(
            all_items,
            vec![ItemTo(10), ItemTo(20), ItemTo(30), ItemTo(40)]
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_ok_try_into_failure() {
        #[derive(Debug, Clone)]
        struct ItemFrom(i32);
        #[derive(Debug, PartialEq)]
        struct ItemTo(i32);

        impl TryInto<ItemTo> for ItemFrom {
            type Error = String;

            fn try_into(self) -> Result<ItemTo, Self::Error> {
                if self.0 >= 0 {
                    Ok(ItemTo(self.0 * 10))
                } else {
                    Err("negative value".to_string())
                }
            }
        }

        let page = Page::WithTotal {
            items: vec![ItemFrom(1), ItemFrom(-2)],
            offset: 0,
            limit: 2,
            total: 2,
        };

        let response: PagingResponse<ItemFrom, String> =
            PagingResponse::new(page, |_, _| Box::pin(async { Ok(PagingResponse::empty()) }));

        let result: Result<PagingResponse<ItemTo, String>, String> = response.ok_try_into();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "negative value");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_ok_try_into_map_err_success() {
        #[derive(Debug, Clone)]
        struct ItemFrom(i32);
        #[derive(Debug, PartialEq)]
        struct ItemTo(i32);

        impl TryInto<ItemTo> for ItemFrom {
            type Error = &'static str;

            fn try_into(self) -> Result<ItemTo, Self::Error> {
                if self.0 >= 0 {
                    Ok(ItemTo(self.0 * 10))
                } else {
                    Err("negative value")
                }
            }
        }

        let page = Page::WithTotal {
            items: vec![ItemFrom(1), ItemFrom(2)],
            offset: 0,
            limit: 2,
            total: 4,
        };

        let response: PagingResponse<ItemFrom, String> =
            PagingResponse::new(page, |_offset, _limit| {
                Box::pin(async {
                    Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![ItemFrom(3), ItemFrom(4)],
                            offset: 2,
                            limit: 2,
                            total: 4,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    ))
                })
            });

        let result: Result<PagingResponse<ItemTo, String>, String> =
            response.ok_try_into_map_err(|e| format!("mapped: {e}"));

        assert!(result.is_ok());
        let converted = result.unwrap();
        assert_eq!(converted.items(), &[ItemTo(10), ItemTo(20)]);

        let all_items = converted.with_rest_of_items().await.unwrap();
        assert_eq!(
            all_items,
            vec![ItemTo(10), ItemTo(20), ItemTo(30), ItemTo(40)]
        );
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_ok_try_into_map_err_failure() {
        #[derive(Debug, Clone)]
        struct ItemFrom(i32);
        #[derive(Debug, PartialEq)]
        struct ItemTo(i32);

        impl TryInto<ItemTo> for ItemFrom {
            type Error = &'static str;

            fn try_into(self) -> Result<ItemTo, Self::Error> {
                if self.0 >= 0 {
                    Ok(ItemTo(self.0 * 10))
                } else {
                    Err("negative value")
                }
            }
        }

        let page = Page::WithTotal {
            items: vec![ItemFrom(1), ItemFrom(-2)],
            offset: 0,
            limit: 2,
            total: 2,
        };

        let response: PagingResponse<ItemFrom, String> =
            PagingResponse::new(page, |_, _| Box::pin(async { Ok(PagingResponse::empty()) }));

        let result: Result<PagingResponse<ItemTo, String>, String> =
            response.ok_try_into_map_err(|e| format!("mapped: {e}"));

        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "mapped: negative value");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_batches_falls_back_to_sequential_for_has_more() {
        // When total is not known (WithHasMore), batch methods should fall back to sequential
        let page = Page::WithHasMore {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            has_more: true,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |offset, _limit| {
            Box::pin(async move {
                if offset == 2 {
                    Ok(PagingResponse::new(
                        Page::WithHasMore {
                            items: vec![3, 4],
                            offset: 2,
                            limit: 2,
                            has_more: false,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    ))
                } else {
                    Ok(PagingResponse::empty())
                }
            })
        });

        // Even though we call "in_batches", it should fall back to sequential since total is unknown
        let pages = response.rest_of_pages_in_batches().await.unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].items(), &[3, 4]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_batches_no_more_pages() {
        // Test batch fetching when there are no more pages to fetch
        let page = Page::WithTotal {
            items: vec![1, 2, 3],
            offset: 0,
            limit: 3,
            total: 3,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |_, _| {
            Box::pin(async { panic!("Should not be called when no more pages in batch mode") })
        });

        // Using batch mode, which correctly skips fetching when offset >= total
        let pages = response.rest_of_pages_in_batches().await.unwrap();
        assert!(pages.is_empty());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_batches_error_propagation() {
        let page = Page::WithTotal {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            total: 6,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |offset, _limit| {
            Box::pin(async move {
                if offset == 4 {
                    Err("batch fetch failed".to_string())
                } else {
                    Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![3, 4],
                            offset: 2,
                            limit: 2,
                            total: 6,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    ))
                }
            })
        });

        let result = response.rest_of_pages_in_batches().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "batch fetch failed");
    }

    // ===== Additional Edge Case Tests =====

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_transpose_fetch_error_propagation() {
        // Test that errors from the underlying fetch are correctly propagated through transpose
        let page = Page::WithTotal {
            items: vec![Ok(1), Ok(2)],
            offset: 0,
            limit: 2,
            total: 4,
        };

        let response: PagingResponse<Result<i32, String>, String> =
            PagingResponse::new(page, |_offset, _limit| {
                Box::pin(async { Err("fetch error".to_string()) })
            });

        let transposed = response.transpose().unwrap();

        // Initial page should be OK
        assert_eq!(transposed.items(), &[1, 2]);

        // But fetching more pages should propagate the error
        let result = transposed.rest_of_items().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "fetch error");
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_transpose_subsequent_page_item_error() {
        // Test that Err items in subsequent pages are correctly handled by transpose
        let page = Page::WithTotal {
            items: vec![Ok(1), Ok(2)],
            offset: 0,
            limit: 2,
            total: 4,
        };

        let response: PagingResponse<Result<i32, String>, String> =
            PagingResponse::new(page, |_offset, _limit| {
                Box::pin(async {
                    Ok(PagingResponse::new(
                        Page::WithTotal {
                            items: vec![Ok(3), Err("item error".to_string())],
                            offset: 2,
                            limit: 2,
                            total: 4,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    ))
                })
            });

        let transposed = response.transpose().unwrap();
        assert_eq!(transposed.items(), &[1, 2]);

        // Fetching more should fail due to Err item in subsequent page
        let result = transposed.rest_of_items().await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "item error");
    }

    #[test_log::test]
    fn test_page_has_more_exact_boundary() {
        // Test when offset + items.len() exactly equals total (boundary condition)
        let page = Page::WithTotal {
            items: vec![1, 2, 3],
            offset: 7,
            limit: 5,
            total: 10,
        };

        // offset (7) + items.len() (3) = 10, which equals total (10)
        assert!(!page.has_more());
    }

    #[test_log::test]
    fn test_page_serialization_has_more_exact_boundary() {
        // Test serialization when offset + items.len() == total produces hasMore: false
        let page = Page::WithTotal {
            items: vec![8, 9, 10],
            offset: 7,
            limit: 3,
            total: 10,
        };

        let json = serde_json::to_string(&page).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["hasMore"], false);
        assert_eq!(parsed["total"], 10);
    }

    #[test_log::test]
    fn test_page_with_has_more_empty_items() {
        // Edge case: empty items with has_more true (unusual but valid)
        let page: Page<i32> = Page::WithHasMore {
            items: vec![],
            offset: 0,
            limit: 10,
            has_more: true,
        };

        assert!(page.has_more());
        assert!(page.items().is_empty());
        assert_eq!(page.offset(), 0);
        assert_eq!(page.limit(), 10);
    }

    #[test_log::test]
    fn test_page_deserialization_with_null_total() {
        // Explicit null total should result in WithHasMore variant
        let json = r#"{"items":[1,2],"offset":0,"limit":2,"total":null,"hasMore":true}"#;

        let page: Page<i32> = serde_json::from_str(json).unwrap();

        assert_eq!(page.items(), &[1, 2]);
        assert_eq!(page.total(), None);
        assert!(page.has_more());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_map_fetch_error_transformation() {
        // Test that map's fetch transformation correctly applies to both success and error paths
        let page = Page::WithTotal {
            items: vec![1, 2],
            offset: 0,
            limit: 2,
            total: 4,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |_offset, _limit| {
            Box::pin(async {
                Ok(PagingResponse::new(
                    Page::WithTotal {
                        items: vec![3, 4],
                        offset: 2,
                        limit: 2,
                        total: 4,
                    },
                    |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                ))
            })
        });

        // Apply map transformation
        let mapped = response.map(|x| x * 3);

        // Verify subsequent fetches are also mapped
        let all = mapped.with_rest_of_items().await.unwrap();
        assert_eq!(all, vec![3, 6, 9, 12]);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_paging_response_sequential_multiple_pages() {
        // Test sequential fetching across more than 2 pages using WithHasMore
        let page = Page::WithHasMore {
            items: vec![1],
            offset: 0,
            limit: 1,
            has_more: true,
        };

        let response: PagingResponse<i32, String> = PagingResponse::new(page, |offset, _limit| {
            Box::pin(async move {
                match offset {
                    1 => Ok(PagingResponse::new(
                        Page::WithHasMore {
                            items: vec![2],
                            offset: 1,
                            limit: 1,
                            has_more: true,
                        },
                        |off, _| {
                            Box::pin(async move {
                                if off == 2 {
                                    Ok(PagingResponse::new(
                                        Page::WithHasMore {
                                            items: vec![3],
                                            offset: 2,
                                            limit: 1,
                                            has_more: false,
                                        },
                                        |_, _| {
                                            Box::pin(async {
                                                Ok(PagingResponse::<i32, String>::empty())
                                            })
                                        },
                                    ))
                                } else {
                                    Ok(PagingResponse::empty())
                                }
                            })
                        },
                    )),
                    2 => Ok(PagingResponse::new(
                        Page::WithHasMore {
                            items: vec![3],
                            offset: 2,
                            limit: 1,
                            has_more: false,
                        },
                        |_, _| Box::pin(async { Ok(PagingResponse::empty()) }),
                    )),
                    _ => Ok(PagingResponse::empty()),
                }
            })
        });

        let all_items = response.with_rest_of_items().await.unwrap();
        assert_eq!(all_items, vec![1, 2, 3]);
    }

    #[test_log::test]
    fn test_page_into_items_with_has_more() {
        let page = Page::WithHasMore {
            items: vec!["a".to_string(), "b".to_string()],
            offset: 5,
            limit: 2,
            has_more: true,
        };

        let items = page.into_items();
        assert_eq!(items, vec!["a", "b"]);
    }

    #[test_log::test]
    fn test_page_into_vec_with_has_more() {
        let page = Page::WithHasMore {
            items: vec![10, 20, 30],
            offset: 0,
            limit: 3,
            has_more: false,
        };

        let vec: Vec<i32> = Vec::from(page);
        assert_eq!(vec, vec![10, 20, 30]);
    }

    #[test_log::test]
    fn test_page_deref_with_has_more() {
        let page = Page::WithHasMore {
            items: vec![100, 200],
            offset: 0,
            limit: 2,
            has_more: true,
        };

        // Deref to Vec should work
        let vec_ref: &Vec<i32> = &page;
        assert_eq!(vec_ref.len(), 2);
        assert_eq!(vec_ref[1], 200);
    }

    #[test_log::test]
    fn test_paging_request_clone() {
        let req1 = PagingRequest {
            offset: 10,
            limit: 20,
        };
        let req2 = req1.clone();

        assert_eq!(req1, req2);
        assert_eq!(req2.offset, 10);
        assert_eq!(req2.limit, 20);
    }
}
