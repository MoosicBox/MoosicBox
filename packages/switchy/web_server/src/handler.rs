//! HTTP handler conversion traits and implementations.
//!
//! This module provides the [`IntoHandler`] trait, which enables functions with various
//! signatures to be used as HTTP request handlers. The trait system automatically extracts
//! typed data from requests and provides it as function arguments.
//!
//! # Overview
//!
//! The handler system supports:
//!
//! * Functions with 0-16 parameters that implement [`FromRequest`]
//! * Async functions returning `Result<HttpResponse, Error>`
//! * Dual-mode extraction (sync for Actix, async for Simulator)
//!
//! # Example
//!
//! ```rust,ignore
//! use switchy_web_server::{HttpResponse, Error};
//! use switchy_web_server::extractors::Query;
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct Params {
//!     name: String,
//! }
//!
//! // Handler with no parameters
//! async fn hello() -> Result<HttpResponse, Error> {
//!     Ok(HttpResponse::text("Hello!"))
//! }
//!
//! // Handler with extractors
//! async fn greet(Query(params): Query<Params>) -> Result<HttpResponse, Error> {
//!     Ok(HttpResponse::text(format!("Hello, {}!", params.name)))
//! }
//! ```

use std::{future::Future, pin::Pin};

use crate::from_request::{FromRequest, IntoHandlerError};
use crate::{Error, HttpRequest, HttpResponse};

/// Trait for converting functions into HTTP handlers
///
/// This trait allows functions with various signatures to be used as HTTP handlers.
/// The Args type parameter represents the arguments that can be extracted from the request.
pub trait IntoHandler<Args> {
    /// The future type returned by the handler
    type Future: Future<Output = Result<HttpResponse, Error>>;

    /// Convert the handler into a callable that takes an `HttpRequest`
    fn into_handler(self) -> Box<dyn Fn(HttpRequest) -> Self::Future + Send + Sync>;
}

/// Wrapper for handler futures that provides consistent behavior across runtimes
pub struct HandlerFuture<F> {
    inner: F,
}

impl<F> HandlerFuture<F> {
    /// Create a new `HandlerFuture`
    pub const fn new(future: F) -> Self {
        Self { inner: future }
    }
}

impl<F> Future for HandlerFuture<F>
where
    F: Future<Output = Result<HttpResponse, Error>>,
{
    type Output = Result<HttpResponse, Error>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // SAFETY: We're not moving the inner future, just projecting the pin
        let inner = unsafe { self.map_unchecked_mut(|s| &mut s.inner) };
        inner.poll(cx)
    }
}

/// Marker trait for handler Send bounds
///
/// This trait is automatically implemented for all types and provides
/// feature-gated Send bounds for different runtimes:
///
/// * With `actix` feature: Requires `Send` for thread-safe operation
/// * Without `actix` feature: No Send requirement for single-threaded operation
#[cfg(feature = "actix")]
pub trait HandlerSend: Send {}

/// Marker trait for handler Send bounds
///
/// This trait is automatically implemented for all types and provides
/// feature-gated Send bounds for different runtimes:
///
/// * With `actix` feature: Requires `Send` for thread-safe operation
/// * Without `actix` feature: No Send requirement for single-threaded operation
#[cfg(not(feature = "actix"))]
pub trait HandlerSend {}

#[cfg(feature = "actix")]
impl<T: Send> HandlerSend for T {}

#[cfg(not(feature = "actix"))]
impl<T> HandlerSend for T {}

/// Macro to generate `IntoHandler` implementations for functions with different parameter counts
///
/// This macro now uses our dual-mode `FromRequest` trait to handle both Actix (sync) and
/// simulator (async) backends properly.
macro_rules! impl_handler {
    // Single parameter case - uses dual-mode extraction
    ($T1:ident) => {
        impl<F, Fut, $T1> IntoHandler<($T1,)> for F
        where
            F: Fn($T1) -> Fut + Send + Sync + 'static,
            Fut: Future<Output = Result<HttpResponse, Error>> + Send + 'static,
            $T1: FromRequest + Send + 'static,
            $T1::Error: Send + 'static,
            $T1::Future: Send + 'static,
        {
            type Future = Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>;

            fn into_handler(self) -> Box<dyn Fn(HttpRequest) -> Self::Future + Send + Sync> {
                let handler = std::sync::Arc::new(self);
                Box::new(move |req| {
                    let handler = handler.clone();

                    // For Actix, extract synchronously to avoid Send bounds issues
                    #[cfg(feature = "actix")]
                    {
                        let extracted = match $T1::from_request_sync(&req) {
                            Ok(value) => value,
                            Err(e) => return Box::pin(async move { Err(e.into_handler_error()) }),
                        };
                        Box::pin(async move {
                            handler(extracted).await
                        })
                    }

                    // For non-Actix, use async extraction
                    #[cfg(not(feature = "actix"))]
                    {
                        Box::pin(async move {
                            let extracted = match $T1::from_request_async(req).await {
                                Ok(value) => value,
                                Err(e) => return Err(e.into_handler_error()),
                            };
                            handler(extracted).await
                        })
                    }
                })
            }
        }
    };

    // Multiple parameters case - uses dual-mode extraction for all parameters
    ($T1:ident, $($T:ident),+) => {
        impl<F, Fut, $T1, $($T),+> IntoHandler<($T1, $($T),+)> for F
        where
            F: Fn($T1, $($T),+) -> Fut + Send + Sync + 'static,
            Fut: Future<Output = Result<HttpResponse, Error>> + Send + 'static,
            $T1: FromRequest + Send + 'static,
            $T1::Error: Send + 'static,
            $T1::Future: Send + 'static,
            $(
                $T: FromRequest + Send + 'static,
                $T::Error: Send + 'static,
                $T::Future: Send + 'static,
            )+
        {
            type Future = Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>;

            fn into_handler(self) -> Box<dyn Fn(HttpRequest) -> Self::Future + Send + Sync> {
                let handler = std::sync::Arc::new(self);
                Box::new(move |req| {
                    let handler = handler.clone();

                    // For Actix, extract synchronously to avoid Send bounds issues
                    #[cfg(feature = "actix")]
                    {
                        let param1 = match $T1::from_request_sync(&req) {
                            Ok(value) => value,
                            Err(e) => return Box::pin(async move { Err(e.into_handler_error()) }),
                        };
                        $(
                            let $T = match $T::from_request_sync(&req) {
                                Ok(value) => value,
                                Err(e) => return Box::pin(async move { Err(e.into_handler_error()) }),
                            };
                        )+

                        Box::pin(async move {
                            handler(param1, $($T),+).await
                        })
                    }

                    // For non-Actix, use async extraction
                    #[cfg(not(feature = "actix"))]
                    {
                        Box::pin(async move {
                            let param1 = match $T1::from_request_async(req.clone()).await {
                                Ok(value) => value,
                                Err(e) => return Err(e.into_handler_error()),
                            };
                            $(
                                let $T = match $T::from_request_async(req.clone()).await {
                                    Ok(value) => value,
                                    Err(e) => return Err(e.into_handler_error()),
                                };
                            )+

                            handler(param1, $($T),+).await
                        })
                    }
                })
            }
        }
    };
}

// Implementation for handlers with NO parameters (the Send-safe solution!)
impl<F, Fut> IntoHandler<()> for F
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<HttpResponse, Error>> + Send + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>;

    fn into_handler(self) -> Box<dyn Fn(HttpRequest) -> Self::Future + Send + Sync> {
        let handler = std::sync::Arc::new(self);
        Box::new(move |_req| {
            let handler = handler.clone();
            // No extraction needed - just call the handler
            // This is completely Send-safe because no HttpRequest is captured!
            Box::pin(async move { handler().await })
        })
    }
}

// Generate IntoHandler implementations for 1-16 parameters
// Allow non-snake-case for type parameters in macros
// Generate IntoHandler implementations for different parameter counts
#[allow(non_snake_case, unused_attributes)]
mod handler_impls {
    use super::{
        Error, FromRequest, Future, HttpRequest, HttpResponse, IntoHandler, IntoHandlerError, Pin,
    };

    impl_handler!(T1); // 1 parameter
    impl_handler!(T1, T2); // 2 parameters
    impl_handler!(T1, T2, T3); // 3 parameters
    impl_handler!(T1, T2, T3, T4); // 4 parameters
    impl_handler!(T1, T2, T3, T4, T5); // 5 parameters
    impl_handler!(T1, T2, T3, T4, T5, T6); // 6 parameters
    impl_handler!(T1, T2, T3, T4, T5, T6, T7); // 7 parameters
    impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8); // 8 parameters
    impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9); // 9 parameters
    impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10); // 10 parameters
    impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11); // 11 parameters
    impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12); // 12 parameters
    impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13); // 13 parameters
    impl_handler!(T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14); // 14 parameters
    impl_handler!(
        T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15
    ); // 15 parameters
    impl_handler!(
        T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11, T12, T13, T14, T15, T16
    ); // 16 parameters
}

// Note: Legacy HttpRequest handlers are now handled by the generic impl_handler!(T1) macro
// since HttpRequest implements FromRequest. However, this will have Send bounds issues with Actix.
// Users should migrate to using extractors instead of raw HttpRequest.
