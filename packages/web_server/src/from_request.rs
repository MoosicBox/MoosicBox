//! Request data extraction trait and implementations.
//!
//! This module provides the [`FromRequest`] trait, which enables type-safe extraction of data
//! from HTTP requests. It supports both synchronous and asynchronous extraction to accommodate
//! different backend requirements.
//!
//! # Overview
//!
//! The module includes:
//!
//! * [`FromRequest`] - Core trait for extracting typed data from requests
//! * [`IntoHandlerError`] - Trait for converting errors into handler errors
//! * [`RequestData`] - Send-safe wrapper for commonly needed request data
//! * [`RequestInfo`] - Basic request information (method, path, query)
//! * [`Headers`] - Send-safe header extraction
//! * [`Query`] - Query parameter extraction (requires `serde` feature)
//! * [`Json`] - JSON body extraction (requires `serde` feature)
//!
//! # Dual-Mode Support
//!
//! The trait supports both sync and async extraction:
//!
//! * **Actix backend**: Uses synchronous extraction to avoid Send bounds issues
//! * **Simulator backend**: Can use either sync or async extraction
//!
//! # Example
//!
//! ```rust,ignore
//! use moosicbox_web_server::{HttpRequest, HttpResponse, Error};
//! use moosicbox_web_server::from_request::{FromRequest, RequestData};
//!
//! async fn handler(data: RequestData) -> Result<HttpResponse, Error> {
//!     println!("Method: {:?}", data.method);
//!     println!("Path: {}", data.path);
//!     Ok(HttpResponse::ok())
//! }
//! ```

use std::{collections::BTreeMap, future::Future};

use crate::{Error, HttpRequest, Method};
#[cfg(feature = "serde")]
use serde::de::DeserializeOwned;

/// Trait for converting errors into handler errors
pub trait IntoHandlerError {
    /// Convert into a handler error
    fn into_handler_error(self) -> Error;
}

impl IntoHandlerError for Error {
    fn into_handler_error(self) -> Error {
        self
    }
}

/// Trait for extracting data from HTTP requests with dual-mode support
///
/// This trait supports both synchronous and asynchronous extraction to solve
/// the Send bounds issue with different backends:
///
/// * **Actix backend**: Uses synchronous extraction to avoid Send bounds issues
/// * **Simulator backend**: Can use either sync or async extraction
///
/// # Example
///
/// ```rust
/// use moosicbox_web_server::{HttpRequest, from_request::FromRequest};
///
/// struct MyExtractor {
///     value: String,
/// }
///
/// impl FromRequest for MyExtractor {
///     type Error = moosicbox_web_server::Error;
///
///     fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
///         // Extract data synchronously from the request
///         let value = req.query_string().to_string();
///         Ok(MyExtractor { value })
///     }
///
///     // Async version can delegate to sync version for simple cases
///     type Future = std::future::Ready<Result<Self, Self::Error>>;
///     fn from_request_async(req: HttpRequest) -> Self::Future {
///         std::future::ready(Self::from_request_sync(&req))
///     }
/// }
/// ```
pub trait FromRequest: Sized {
    /// The error type returned if extraction fails
    type Error: IntoHandlerError;

    /// Extract data from the request synchronously
    ///
    /// This method is used by the Actix backend to avoid Send bounds issues.
    /// It takes a reference to the request to avoid moving non-Send types.
    ///
    /// # Errors
    ///
    /// Returns an error if the extraction fails, such as when required data
    /// is missing from the request or cannot be parsed into the expected format.
    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error>;

    /// The future type returned by async extraction
    type Future: Future<Output = Result<Self, Self::Error>>;

    /// Extract data from the request asynchronously
    ///
    /// This method is used by the Simulator backend and for extractors that
    /// need to perform async operations (like reading request bodies).
    fn from_request_async(req: HttpRequest) -> Self::Future;
}

/// Identity extraction for `HttpRequest` itself
///
/// Note: This implementation has limitations due to `HttpRequest` not implementing Clone
/// for the Actix backend. In practice, extractors should extract specific data rather
/// than trying to extract the entire `HttpRequest`.
impl FromRequest for HttpRequest {
    type Error = Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(_req: &HttpRequest) -> Result<Self, Self::Error> {
        // We cannot clone HttpRequest due to Actix's non-Clone types
        // This is a limitation that users should work around by extracting specific data
        Err(Error::bad_request(
            "Cannot extract HttpRequest directly due to Clone limitations. Extract specific data instead.",
        ))
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        // For async extraction, we can move the request
        std::future::ready(Ok(req))
    }
}

// Note: HttpRequestRef implementation is not provided due to lifetime complexities.
// Users should extract RequestData or specific fields instead.

/// A Send-safe wrapper containing commonly needed request data
///
/// This struct extracts and stores commonly needed data from the `HttpRequest`,
/// making it safe to pass across async boundaries. This solves the Send bounds
/// issue by extracting data synchronously before entering async contexts.
///
/// # Example
///
/// ```rust,ignore
/// use moosicbox_web_server::{RequestData, HttpResponse, from_request::FromRequest};
///
/// async fn my_handler(data: RequestData) -> Result<HttpResponse, Box<dyn std::error::Error>> {
///     println!("Method: {:?}", data.method);
///     println!("Path: {}", data.path);
///     println!("Query: {}", data.query);
///     Ok(HttpResponse::ok())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct RequestData {
    /// HTTP method (GET, POST, etc.)
    pub method: Method,
    /// Request path (e.g., "/api/users")
    pub path: String,
    /// Query string (e.g., "name=john&age=30")
    pub query: String,
    /// Request headers as key-value pairs
    pub headers: BTreeMap<String, String>,
    /// Remote client address if available
    pub remote_addr: Option<String>,
    /// User-Agent header if present
    pub user_agent: Option<String>,
    /// Content-Type header if present
    pub content_type: Option<String>,
}

impl RequestData {
    /// Get a specific header value
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&String> {
        self.headers.get(name)
    }

    /// Check if the request has a specific header
    #[must_use]
    pub fn has_header(&self, name: &str) -> bool {
        self.headers.contains_key(name)
    }

    /// Get the content length from headers
    #[must_use]
    pub fn content_length(&self) -> Option<usize> {
        self.header("content-length").and_then(|v| v.parse().ok())
    }
}

impl FromRequest for RequestData {
    type Error = Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        // Extract all commonly needed data synchronously
        let method = req.method();
        let path = req.path().to_string();
        let query = req.query_string().to_string();
        let remote_addr = req.remote_addr();

        // Extract headers
        let mut headers = BTreeMap::new();
        let cookies = req.cookies();
        for (name, value) in &cookies {
            headers.insert(format!("cookie-{name}"), value.clone());
        }

        // Extract common headers
        let user_agent = req
            .header("user-agent")
            .map(std::string::ToString::to_string);
        if let Some(ua) = &user_agent {
            headers.insert("user-agent".to_string(), ua.clone());
        }

        let content_type = req
            .header("content-type")
            .map(std::string::ToString::to_string);
        if let Some(ct) = &content_type {
            headers.insert("content-type".to_string(), ct.clone());
        }

        // Add other common headers
        if let Some(auth) = req.header("authorization") {
            headers.insert("authorization".to_string(), auth.to_string());
        }

        if let Some(accept) = req.header("accept") {
            headers.insert("accept".to_string(), accept.to_string());
        }

        Ok(Self {
            method,
            path,
            query,
            headers,
            remote_addr,
            user_agent,
            content_type,
        })
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

// Basic type implementations for common use cases

impl FromRequest for String {
    type Error = Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        // For String, we'll extract the query string as a reasonable default
        Ok(req.query_string().to_string())
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

impl FromRequest for u32 {
    type Error = Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        // For u32, we'll try to parse the query string as a number
        let query = req.query_string();
        query
            .parse::<Self>()
            .map_err(|e| Error::bad_request(format!("Failed to parse query as u32: {e}")))
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

impl FromRequest for i32 {
    type Error = Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        // For i32, we'll try to parse the query string as a number
        let query = req.query_string();
        query
            .parse::<Self>()
            .map_err(|e| Error::bad_request(format!("Failed to parse query as i32: {e}")))
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

impl FromRequest for bool {
    type Error = Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        // For bool, we'll check if query string is "true", "1", "yes", or "on"
        let query = req.query_string().to_lowercase();
        let value = matches!(query.as_str(), "true" | "1" | "yes" | "on");
        Ok(value)
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

// ============================================================================
// PROPER EXTRACTORS - These are the real solution to the Send bounds issue
// ============================================================================

#[cfg(feature = "serde")]
/// Extractor for query parameters
///
/// This extractor parses query parameters from the URL query string.
///
/// # Example
///
/// ```rust,ignore
/// use moosicbox_web_server::{Query, HttpResponse};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct SearchParams {
///     q: String,
///     limit: Option<u32>,
/// }
///
/// async fn search(Query(params): Query<SearchParams>) -> Result<HttpResponse, Box<dyn std::error::Error>> {
///     // params.q and params.limit extracted from ?q=hello&limit=10
///     Ok(HttpResponse::ok())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Query<T>(pub T);

#[cfg(feature = "serde")]
impl<T> FromRequest for Query<T>
where
    T: DeserializeOwned + Send + 'static,
{
    type Error = Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        // Parse query string using the existing parse_query method
        req.parse_query::<T>()
            .map(Query)
            .map_err(|e| Error::bad_request(format!("Failed to parse query parameters: {e}")))
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

#[cfg(feature = "serde")]
/// Extractor for JSON request bodies
///
/// This extractor parses the request body as JSON.
/// Note: For the current implementation, we'll simulate JSON extraction
/// since we don't have access to the actual body in our `HttpRequest`.
///
/// # Example
///
/// ```rust,ignore
/// use moosicbox_web_server::{Json, HttpResponse};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct CreateUser {
///     name: String,
///     email: String,
/// }
///
/// async fn create_user(Json(user): Json<CreateUser>) -> Result<HttpResponse, Box<dyn std::error::Error>> {
///     // user.name and user.email extracted from JSON body
///     Ok(HttpResponse::ok())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Json<T>(pub T);

#[cfg(feature = "serde")]
impl<T> FromRequest for Json<T>
where
    T: DeserializeOwned + Send + 'static,
{
    type Error = Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        // Try to read the request body and parse as JSON
        req.body().map_or_else(|| Err(Error::bad_request(
                "JSON body not available. For Actix backend, body must be pre-extracted. For Simulator backend, ensure body is set on the request."
            )), |body| match serde_json::from_slice::<T>(body) {
                Ok(value) => Ok(Self(value)),
                Err(e) => Err(Error::bad_request(format!("Failed to parse JSON body: {e}"))),
            })
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

/// Extractor for request headers
///
/// This extractor provides access to all request headers in a Send-safe way.
///
/// # Example
///
/// ```rust,ignore
/// use moosicbox_web_server::{Headers, HttpResponse};
///
/// async fn handler(headers: Headers) -> Result<HttpResponse, Box<dyn std::error::Error>> {
///     if let Some(auth) = headers.get("authorization") {
///         // Handle authorization header
///     }
///     Ok(HttpResponse::ok())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Headers {
    headers: BTreeMap<String, String>,
}

impl Headers {
    /// Get a header value by name (case-insensitive)
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&String> {
        self.headers.get(&name.to_lowercase())
    }

    /// Check if a header exists
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.headers.contains_key(&name.to_lowercase())
    }

    /// Get all headers
    #[must_use]
    pub const fn all(&self) -> &BTreeMap<String, String> {
        &self.headers
    }

    /// Get authorization header
    #[must_use]
    pub fn authorization(&self) -> Option<&String> {
        self.get("authorization")
    }

    /// Get content-type header
    #[must_use]
    pub fn content_type(&self) -> Option<&String> {
        self.get("content-type")
    }

    /// Get user-agent header
    #[must_use]
    pub fn user_agent(&self) -> Option<&String> {
        self.get("user-agent")
    }
}

impl FromRequest for Headers {
    type Error = Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        let mut headers = BTreeMap::new();

        // Extract common headers
        if let Some(auth) = req.header("authorization") {
            headers.insert("authorization".to_string(), auth.to_string());
        }

        if let Some(ct) = req.header("content-type") {
            headers.insert("content-type".to_string(), ct.to_string());
        }

        if let Some(ua) = req.header("user-agent") {
            headers.insert("user-agent".to_string(), ua.to_string());
        }

        if let Some(accept) = req.header("accept") {
            headers.insert("accept".to_string(), accept.to_string());
        }

        if let Some(host) = req.header("host") {
            headers.insert("host".to_string(), host.to_string());
        }

        Ok(Self { headers })
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

/// Extractor for basic request information
///
/// This is a simpler version of `RequestData` with just the most commonly needed info.
///
/// # Example
///
/// ```rust,ignore
/// use moosicbox_web_server::{RequestInfo, HttpResponse};
///
/// async fn handler(info: RequestInfo) -> Result<HttpResponse, Box<dyn std::error::Error>> {
///     println!("Request to {} via {}", info.path, info.method);
///     Ok(HttpResponse::ok())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct RequestInfo {
    /// HTTP method (GET, POST, etc.)
    pub method: Method,
    /// Request path (e.g., `/api/users`)
    pub path: String,
    /// Query string (e.g., `page=1&limit=20`)
    pub query: String,
    /// Optional remote address of the client
    pub remote_addr: Option<String>,
}

impl FromRequest for RequestInfo {
    type Error = Error;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            method: req.method(),
            path: req.path().to_string(),
            query: req.query_string().to_string(),
            remote_addr: req.remote_addr(),
        })
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    use crate::{Method, Stub, simulator::SimulationRequest};

    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn create_test_request() -> HttpRequest {
        let sim_req = SimulationRequest::new(Method::Get, "/api/users")
            .with_query_string("page=1&limit=20")
            .with_header("user-agent", "TestAgent/1.0")
            .with_header("content-type", "application/json")
            .with_header("authorization", "Bearer token123")
            .with_header("accept", "application/json");

        HttpRequest::Stub(Stub::Simulator(sim_req.into()))
    }

    #[test_log::test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_request_data_extraction() {
        let req = create_test_request();
        let result = RequestData::from_request_sync(&req);

        assert!(result.is_ok());
        let data = result.unwrap();

        assert_eq!(data.method, Method::Get);
        assert_eq!(data.path, "/api/users");
        assert_eq!(data.query, "page=1&limit=20");
        assert_eq!(data.user_agent, Some("TestAgent/1.0".to_string()));
        assert_eq!(data.content_type, Some("application/json".to_string()));
    }

    #[test_log::test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_request_data_header_methods() {
        let req = create_test_request();
        let data = RequestData::from_request_sync(&req).unwrap();

        assert!(data.has_header("user-agent"));
        assert!(data.has_header("content-type"));
        assert!(!data.has_header("non-existent"));

        assert_eq!(
            data.header("authorization"),
            Some(&"Bearer token123".to_string())
        );
        assert_eq!(data.header("accept"), Some(&"application/json".to_string()));
        assert_eq!(data.header("non-existent"), None);
    }

    #[test_log::test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_request_info_extraction() {
        let req = create_test_request();
        let result = RequestInfo::from_request_sync(&req);

        assert!(result.is_ok());
        let info = result.unwrap();

        assert_eq!(info.method, Method::Get);
        assert_eq!(info.path, "/api/users");
        assert_eq!(info.query, "page=1&limit=20");
        assert!(info.remote_addr.is_none());
    }

    #[test_log::test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_headers_extraction() {
        let req = create_test_request();
        let result = Headers::from_request_sync(&req);

        assert!(result.is_ok());
        let headers = result.unwrap();

        assert!(headers.contains("authorization"));
        assert!(headers.contains("content-type"));
        assert!(headers.contains("user-agent"));
        assert!(!headers.contains("non-existent"));

        assert_eq!(
            headers.authorization(),
            Some(&"Bearer token123".to_string())
        );
        assert_eq!(
            headers.content_type(),
            Some(&"application/json".to_string())
        );
        assert_eq!(headers.user_agent(), Some(&"TestAgent/1.0".to_string()));
    }

    #[test_log::test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_headers_all() {
        let req = create_test_request();
        let headers = Headers::from_request_sync(&req).unwrap();

        let all_headers = headers.all();
        assert!(!all_headers.is_empty());
        assert!(all_headers.contains_key("authorization"));
        assert!(all_headers.contains_key("content-type"));
    }

    #[test_log::test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_string_from_request() {
        let req = create_test_request();
        let result = String::from_request_sync(&req);

        assert!(result.is_ok());
        let query = result.unwrap();
        assert_eq!(query, "page=1&limit=20");
    }

    #[test_log::test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_u32_from_request_valid() {
        let sim_req = SimulationRequest::new(Method::Get, "/test").with_query_string("42");
        let req = HttpRequest::Stub(Stub::Simulator(sim_req.into()));

        let result = u32::from_request_sync(&req);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 42);
    }

    #[test_log::test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_u32_from_request_invalid() {
        let sim_req =
            SimulationRequest::new(Method::Get, "/test").with_query_string("not_a_number");
        let req = HttpRequest::Stub(Stub::Simulator(sim_req.into()));

        let result = u32::from_request_sync(&req);
        assert!(result.is_err());
    }

    #[test_log::test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_i32_from_request_valid() {
        let sim_req = SimulationRequest::new(Method::Get, "/test").with_query_string("-123");
        let req = HttpRequest::Stub(Stub::Simulator(sim_req.into()));

        let result = i32::from_request_sync(&req);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), -123);
    }

    #[test_log::test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_bool_from_request_true_values() {
        for true_value in &["true", "1", "yes", "on", "TRUE", "YES", "ON"] {
            let sim_req =
                SimulationRequest::new(Method::Get, "/test").with_query_string(*true_value);
            let req = HttpRequest::Stub(Stub::Simulator(sim_req.into()));

            let result = bool::from_request_sync(&req);
            assert!(result.is_ok());
            assert!(result.unwrap(), "Failed for value: {true_value}");
        }
    }

    #[test_log::test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_bool_from_request_false_values() {
        for false_value in &[
            "false",
            "0",
            "no",
            "off",
            "FALSE",
            "NO",
            "OFF",
            "anything_else",
        ] {
            let sim_req =
                SimulationRequest::new(Method::Get, "/test").with_query_string(*false_value);
            let req = HttpRequest::Stub(Stub::Simulator(sim_req.into()));

            let result = bool::from_request_sync(&req);
            assert!(result.is_ok());
            assert!(!result.unwrap(), "Failed for value: {false_value}");
        }
    }

    #[test_log::test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_http_request_from_request_error() {
        let req = create_test_request();
        let result = HttpRequest::from_request_sync(&req);

        // Should return an error because HttpRequest cannot be cloned
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_into_handler_error() {
        let error = Error::bad_request("Test error");
        let handler_error = error.into_handler_error();

        match handler_error {
            Error::Http { status_code, .. } => {
                assert_eq!(status_code, crate::StatusCode::BadRequest);
            }
        }
    }
}
