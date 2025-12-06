//! Backend-agnostic HTTP request abstractions.
//!
//! This module provides a trait-based approach for HTTP requests that allows
//! different backends (Actix, Simulator, etc.) to provide their own implementations
//! while maintaining a consistent API for handlers and extractors.
//!
//! # Architecture
//!
//! The core abstraction is [`HttpRequestTrait`], which defines the interface that
//! all backend request types must implement. The [`HttpRequest`] struct wraps a
//! trait object, providing a concrete type that can be used throughout the framework.
//!
//! # Example
//!
//! ```rust,ignore
//! use moosicbox_web_server::request::{HttpRequest, HttpRequestTrait};
//!
//! fn handle_request(req: HttpRequest) {
//!     println!("Path: {}", req.path());
//!     println!("Method: {:?}", req.method());
//! }
//! ```

use std::{any::TypeId, collections::BTreeMap, sync::Arc};

use bytes::Bytes;
use switchy_http_models::Method;

use crate::PathParams;

/// Type-erased application state that can be downcasted to the original type.
///
/// This is used internally to store and retrieve application state in a dyn-compatible way.
pub type ErasedState = Arc<dyn std::any::Any + Send + Sync>;

/// Backend-agnostic HTTP request interface.
///
/// This trait defines the common operations that all HTTP request backends must support.
/// Implementations are provided by specific backends (e.g., Actix, Simulator).
///
/// # Thread Safety
///
/// All implementations must be `Send + Sync` to allow requests to be passed between
/// threads and used in async contexts.
pub trait HttpRequestTrait: Send + Sync {
    /// Returns the request path (e.g., `/api/users`).
    fn path(&self) -> &str;

    /// Returns the query string without the leading `?` (e.g., `name=john&age=30`).
    fn query_string(&self) -> &str;

    /// Returns the HTTP method (GET, POST, etc.).
    fn method(&self) -> Method;

    /// Returns a header value by name (case-insensitive).
    ///
    /// Returns `None` if the header doesn't exist.
    fn header(&self, name: &str) -> Option<&str>;

    /// Returns all headers as a map.
    fn headers(&self) -> BTreeMap<String, String>;

    /// Returns the request body as bytes if available.
    ///
    /// Note: Some backends (like Actix) consume the body during extraction,
    /// so this may return `None` even if a body was originally present.
    fn body(&self) -> Option<&Bytes>;

    /// Returns a cookie value by name.
    ///
    /// Returns `None` if the cookie doesn't exist.
    fn cookie(&self, name: &str) -> Option<String>;

    /// Returns all cookies as a map of name-value pairs.
    fn cookies(&self) -> BTreeMap<String, String>;

    /// Returns the remote client address if available.
    fn remote_addr(&self) -> Option<String>;

    /// Returns path parameters extracted from route matching.
    ///
    /// Path parameters are extracted from dynamic route segments like `/users/{id}`.
    /// Returns an empty map if no path parameters are present.
    fn path_params(&self) -> &PathParams;

    /// Returns type-erased application state by type ID.
    ///
    /// This method is used internally by the `app_state<T>` method on `HttpRequest`.
    /// Backends should implement this to retrieve state from their storage mechanism.
    ///
    /// Returns `None` if:
    /// - No state of the given type has been registered
    /// - The backend doesn't support state management
    fn app_state_any(&self, type_id: TypeId) -> Option<ErasedState>;
}

/// Backend-agnostic HTTP request wrapper.
///
/// This struct wraps a trait object implementing [`HttpRequestTrait`], providing
/// a concrete type that can be used throughout the framework. It delegates all
/// method calls to the underlying implementation.
///
/// # Creating Requests
///
/// Requests are typically created by backends when handling incoming HTTP requests.
/// Use [`HttpRequest::new`] to wrap a backend-specific request type:
///
/// ```rust,ignore
/// use moosicbox_web_server::request::HttpRequest;
///
/// // Backend creates its specific request type
/// let backend_request = MyBackendRequest::new(...);
///
/// // Wrap it in the generic HttpRequest
/// let request = HttpRequest::new(backend_request);
/// ```
#[derive(Clone)]
pub struct HttpRequest {
    inner: Arc<dyn HttpRequestTrait>,
}

impl std::fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpRequest")
            .field("path", &self.path())
            .field("method", &self.method())
            .field("query_string", &self.query_string())
            .finish_non_exhaustive()
    }
}

impl HttpRequest {
    /// Creates a new `HttpRequest` from a backend-specific request type.
    ///
    /// # Type Parameters
    ///
    /// * `R` - The backend request type, must implement [`HttpRequestTrait`]
    pub fn new<R: HttpRequestTrait + 'static>(request: R) -> Self {
        Self {
            inner: Arc::new(request),
        }
    }

    /// Returns the request path (e.g., `/api/users`).
    #[must_use]
    pub fn path(&self) -> &str {
        self.inner.path()
    }

    /// Returns the query string without the leading `?` (e.g., `name=john&age=30`).
    #[must_use]
    pub fn query_string(&self) -> &str {
        self.inner.query_string()
    }

    /// Returns the HTTP method (GET, POST, etc.).
    #[must_use]
    pub fn method(&self) -> Method {
        self.inner.method()
    }

    /// Returns a header value by name (case-insensitive).
    ///
    /// Returns `None` if the header doesn't exist.
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.inner.header(name)
    }

    /// Returns all headers as a map.
    #[must_use]
    pub fn headers(&self) -> BTreeMap<String, String> {
        self.inner.headers()
    }

    /// Returns the request body as bytes if available.
    ///
    /// Note: Some backends (like Actix) consume the body during extraction,
    /// so this may return `None` even if a body was originally present.
    #[must_use]
    pub fn body(&self) -> Option<&Bytes> {
        self.inner.body()
    }

    /// Returns a cookie value by name.
    ///
    /// Returns `None` if the cookie doesn't exist.
    #[must_use]
    pub fn cookie(&self, name: &str) -> Option<String> {
        self.inner.cookie(name)
    }

    /// Returns all cookies as a map of name-value pairs.
    #[must_use]
    pub fn cookies(&self) -> BTreeMap<String, String> {
        self.inner.cookies()
    }

    /// Returns the remote client address if available.
    #[must_use]
    pub fn remote_addr(&self) -> Option<String> {
        self.inner.remote_addr()
    }

    /// Returns path parameters extracted from route matching.
    ///
    /// Path parameters are extracted from dynamic route segments like `/users/{id}`.
    /// Returns an empty map if no path parameters are present.
    #[must_use]
    pub fn path_params(&self) -> &PathParams {
        self.inner.path_params()
    }

    /// Returns a specific path parameter by name.
    ///
    /// Returns `None` if the parameter doesn't exist.
    #[must_use]
    pub fn path_param(&self, name: &str) -> Option<&str> {
        self.path_params().get(name).map(String::as_str)
    }

    /// Returns application state of type T if available.
    ///
    /// This method allows extractors to access application-level state that was
    /// registered with the web server.
    ///
    /// Returns `None` if no state of type T has been registered.
    #[must_use]
    pub fn app_state<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.inner
            .app_state_any(TypeId::of::<T>())
            .and_then(|erased| erased.downcast::<T>().ok())
    }

    /// Parses the query string into a typed structure.
    ///
    /// # Errors
    ///
    /// Returns `qs::Error` if the query string parsing fails.
    pub fn parse_query<'a, T: serde::Deserialize<'a>>(
        &'a self,
    ) -> Result<T, serde_querystring::Error> {
        serde_querystring::from_str(
            self.query_string(),
            serde_querystring::ParseMode::UrlEncoded,
        )
    }
}

/// An empty HTTP request stub for testing.
///
/// This type provides a minimal [`HttpRequestTrait`] implementation that returns
/// empty/default values for all fields. It's useful for testing code paths that
/// don't depend on specific request data.
///
/// # Example
///
/// ```rust
/// use moosicbox_web_server::request::{HttpRequest, EmptyRequest};
///
/// let request = HttpRequest::new(EmptyRequest);
/// assert_eq!(request.path(), "");
/// assert_eq!(request.query_string(), "");
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct EmptyRequest;

impl HttpRequestTrait for EmptyRequest {
    fn path(&self) -> &'static str {
        ""
    }

    fn query_string(&self) -> &'static str {
        ""
    }

    fn method(&self) -> Method {
        Method::Get
    }

    fn header(&self, _name: &str) -> Option<&str> {
        None
    }

    fn headers(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn body(&self) -> Option<&Bytes> {
        None
    }

    fn cookie(&self, _name: &str) -> Option<String> {
        None
    }

    fn cookies(&self) -> BTreeMap<String, String> {
        BTreeMap::new()
    }

    fn remote_addr(&self) -> Option<String> {
        None
    }

    fn path_params(&self) -> &PathParams {
        static EMPTY: PathParams = BTreeMap::new();
        &EMPTY
    }

    fn app_state_any(&self, _type_id: TypeId) -> Option<ErasedState> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple mock request for testing
    struct MockRequest {
        path: String,
        query_string: String,
        method: Method,
        headers: BTreeMap<String, String>,
        body: Option<Bytes>,
        cookies: BTreeMap<String, String>,
        path_params: PathParams,
    }

    impl MockRequest {
        fn new(path: &str, method: Method) -> Self {
            Self {
                path: path.to_string(),
                query_string: String::new(),
                method,
                headers: BTreeMap::new(),
                body: None,
                cookies: BTreeMap::new(),
                path_params: BTreeMap::new(),
            }
        }
    }

    impl HttpRequestTrait for MockRequest {
        fn path(&self) -> &str {
            &self.path
        }

        fn query_string(&self) -> &str {
            &self.query_string
        }

        fn method(&self) -> Method {
            self.method
        }

        fn header(&self, name: &str) -> Option<&str> {
            self.headers.get(name).map(String::as_str)
        }

        fn headers(&self) -> BTreeMap<String, String> {
            self.headers.clone()
        }

        fn body(&self) -> Option<&Bytes> {
            self.body.as_ref()
        }

        fn cookie(&self, name: &str) -> Option<String> {
            self.cookies.get(name).cloned()
        }

        fn cookies(&self) -> BTreeMap<String, String> {
            self.cookies.clone()
        }

        fn remote_addr(&self) -> Option<String> {
            None
        }

        fn path_params(&self) -> &PathParams {
            &self.path_params
        }

        fn app_state_any(&self, _type_id: TypeId) -> Option<ErasedState> {
            None
        }
    }

    #[test]
    fn test_http_request_delegates_to_inner() {
        let mock = MockRequest::new("/api/users", Method::Get);
        let request = HttpRequest::new(mock);

        assert_eq!(request.path(), "/api/users");
        assert_eq!(request.method(), Method::Get);
        assert_eq!(request.query_string(), "");
    }

    #[test]
    fn test_http_request_debug() {
        let mock = MockRequest::new("/test", Method::Post);
        let request = HttpRequest::new(mock);

        let debug_str = format!("{request:?}");
        assert!(debug_str.contains("/test"));
        assert!(debug_str.contains("Post"));
    }
}
