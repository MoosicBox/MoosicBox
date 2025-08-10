use std::{future::Future, pin::Pin};

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

/// Error conversion utilities for handler errors
pub trait IntoHandlerError {
    /// Convert into a handler error
    fn into_handler_error(self) -> Error;
}

impl IntoHandlerError for Error {
    fn into_handler_error(self) -> Error {
        self
    }
}

// Feature-gated Send bounds for different runtimes
#[cfg(feature = "actix")]
pub trait HandlerSend: Send {}

#[cfg(not(feature = "actix"))]
pub trait HandlerSend {}

#[cfg(feature = "actix")]
impl<T: Send> HandlerSend for T {}

#[cfg(not(feature = "actix"))]
impl<T> HandlerSend for T {}

/// Trait for extracting data from HTTP requests
pub trait FromRequest: Sized {
    /// The error type returned if extraction fails
    type Error: IntoHandlerError;

    /// The future type returned by the extraction
    type Future: Future<Output = Result<Self, Self::Error>>;

    /// Extract data from the request
    fn from_request(req: HttpRequest) -> Self::Future;
}

// Basic implementation for HttpRequest itself
impl FromRequest for HttpRequest {
    type Error = Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request(req: HttpRequest) -> Self::Future {
        std::future::ready(Ok(req))
    }
}

// Implementation for async functions that return Result<HttpResponse, Error>
impl<F, Fut> IntoHandler<()> for F
where
    F: Fn(HttpRequest) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<HttpResponse, Error>> + Send + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Result<HttpResponse, Error>> + Send>>;

    fn into_handler(self) -> Box<dyn Fn(HttpRequest) -> Self::Future + Send + Sync> {
        Box::new(move |req| Box::pin((self)(req)))
    }
}
