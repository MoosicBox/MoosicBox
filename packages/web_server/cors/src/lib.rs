//! CORS (Cross-Origin Resource Sharing) configuration for web servers.
//!
//! This crate provides types for configuring CORS policies, allowing you to control
//! which origins, HTTP methods, and headers are permitted for cross-origin requests.
//!
//! # Example
//!
//! ```rust
//! use moosicbox_web_server_cors::Cors;
//! use switchy_http_models::Method;
//!
//! let cors = Cors::default()
//!     .allow_origin("https://example.com")
//!     .allow_method(Method::Get)
//!     .allow_method(Method::Post)
//!     .allow_header("Content-Type")
//!     .support_credentials()
//!     .max_age(3600);
//! ```
//!
//! For permissive CORS policies during development:
//!
//! ```rust
//! use moosicbox_web_server_cors::Cors;
//!
//! let cors = Cors::default()
//!     .allow_any_origin()
//!     .allow_any_method()
//!     .allow_any_header();
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// Re-exported HTTP models from the `switchy_http_models` crate.
///
/// This includes types like [`switchy_http_models::Method`] used in CORS configuration.
pub use switchy_http_models;

use switchy_http_models::Method;

/// An enum signifying that some of type `T` is allowed, or `All` (anything is allowed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllOrSome<T> {
    /// Everything is allowed. Usually equivalent to the `*` value.
    All,

    /// Only some of `T` is allowed
    Some(T),
}

/// Default as `AllOrSome::All`.
impl<T> Default for AllOrSome<T> {
    fn default() -> Self {
        Self::All
    }
}

impl<T> AllOrSome<T> {
    /// Returns whether this is an `All` variant.
    #[must_use]
    pub const fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }

    /// Returns whether this is a `Some` variant.
    #[must_use]
    pub const fn is_some(&self) -> bool {
        !self.is_all()
    }

    /// Provides a shared reference to `T` if variant is `Some`.
    #[must_use]
    pub const fn as_ref(&self) -> Option<&T> {
        match *self {
            Self::All => None,
            Self::Some(ref t) => Some(t),
        }
    }

    /// Provides a mutable reference to `T` if variant is `Some`.
    #[must_use]
    pub const fn as_mut(&mut self) -> Option<&mut T> {
        match *self {
            Self::All => None,
            Self::Some(ref mut t) => Some(t),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_or_some_is_all_and_is_some() {
        assert!(AllOrSome::<()>::All.is_all());
        assert!(!AllOrSome::<()>::All.is_some());

        assert!(!AllOrSome::Some(()).is_all());
        assert!(AllOrSome::Some(()).is_some());
    }

    #[test]
    fn test_all_or_some_default_is_all() {
        let default: AllOrSome<Vec<String>> = AllOrSome::default();
        assert!(default.is_all());
    }

    #[test]
    fn test_all_or_some_as_ref() {
        let all = AllOrSome::<String>::All;
        assert_eq!(all.as_ref(), None);

        let some = AllOrSome::Some(String::from("test"));
        assert_eq!(some.as_ref(), Some(&String::from("test")));
    }

    #[test]
    fn test_all_or_some_as_mut() {
        let mut all = AllOrSome::<String>::All;
        assert_eq!(all.as_mut(), None);

        let mut some = AllOrSome::Some(String::from("test"));
        if let Some(value) = some.as_mut() {
            value.push_str("_modified");
        }
        assert_eq!(some.as_ref(), Some(&String::from("test_modified")));
    }

    #[test]
    fn test_cors_default() {
        let cors = Cors::default();
        assert!(matches!(cors.allowed_origins, AllOrSome::Some(ref v) if v.is_empty()));
        assert!(matches!(cors.allowed_methods, AllOrSome::Some(ref v) if v.is_empty()));
        assert!(matches!(cors.allowed_headers, AllOrSome::Some(ref v) if v.is_empty()));
        assert!(matches!(cors.expose_headers, AllOrSome::Some(ref v) if v.is_empty()));
        assert!(!cors.supports_credentials);
        assert_eq!(cors.max_age, None);
    }

    #[test]
    fn test_cors_allow_any_origin() {
        let cors = Cors::default().allow_any_origin();
        assert!(cors.allowed_origins.is_all());
    }

    #[test]
    fn test_cors_allow_origin_single() {
        let cors = Cors::default().allow_origin("https://example.com");
        match cors.allowed_origins {
            AllOrSome::Some(origins) => {
                assert_eq!(origins.len(), 1);
                assert_eq!(origins[0], "https://example.com");
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }
    }

    #[test]
    fn test_cors_allow_origin_multiple() {
        let cors = Cors::default()
            .allow_origin("https://example.com")
            .allow_origin("https://test.com");
        match cors.allowed_origins {
            AllOrSome::Some(origins) => {
                assert_eq!(origins.len(), 2);
                assert_eq!(origins[0], "https://example.com");
                assert_eq!(origins[1], "https://test.com");
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }
    }

    #[test]
    fn test_cors_allow_origin_after_allow_any_has_no_effect() {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_origin("https://example.com");
        assert!(cors.allowed_origins.is_all());
    }

    #[test]
    fn test_cors_allowed_origins_iterator() {
        let origins = vec!["https://a.com", "https://b.com", "https://c.com"];
        let cors = Cors::default().allowed_origins(origins);
        match cors.allowed_origins {
            AllOrSome::Some(result) => {
                assert_eq!(result.len(), 3);
                assert_eq!(result[0], "https://a.com");
                assert_eq!(result[1], "https://b.com");
                assert_eq!(result[2], "https://c.com");
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }
    }

    #[test]
    fn test_cors_allowed_origins_after_allow_any_has_no_effect() {
        let origins = vec!["https://a.com", "https://b.com"];
        let cors = Cors::default().allow_any_origin().allowed_origins(origins);
        assert!(cors.allowed_origins.is_all());
    }

    #[test]
    fn test_cors_allow_any_method() {
        let cors = Cors::default().allow_any_method();
        assert!(cors.allowed_methods.is_all());
    }

    #[test]
    fn test_cors_allow_method_single() {
        let cors = Cors::default().allow_method(Method::Get);
        match cors.allowed_methods {
            AllOrSome::Some(methods) => {
                assert_eq!(methods.len(), 1);
                assert_eq!(methods[0], Method::Get);
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }
    }

    #[test]
    fn test_cors_allow_method_multiple() {
        let cors = Cors::default()
            .allow_method(Method::Get)
            .allow_method(Method::Post)
            .allow_method(Method::Put);
        match cors.allowed_methods {
            AllOrSome::Some(methods) => {
                assert_eq!(methods.len(), 3);
                assert_eq!(methods[0], Method::Get);
                assert_eq!(methods[1], Method::Post);
                assert_eq!(methods[2], Method::Put);
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }
    }

    #[test]
    fn test_cors_allow_method_after_allow_any_has_no_effect() {
        let cors = Cors::default().allow_any_method().allow_method(Method::Get);
        assert!(cors.allowed_methods.is_all());
    }

    #[test]
    fn test_cors_allowed_methods_iterator() {
        let methods = vec![Method::Get, Method::Post, Method::Delete];
        let cors = Cors::default().allowed_methods(methods);
        match cors.allowed_methods {
            AllOrSome::Some(result) => {
                assert_eq!(result.len(), 3);
                assert_eq!(result[0], Method::Get);
                assert_eq!(result[1], Method::Post);
                assert_eq!(result[2], Method::Delete);
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }
    }

    #[test]
    fn test_cors_allowed_methods_after_allow_any_has_no_effect() {
        let methods = vec![Method::Get, Method::Post];
        let cors = Cors::default().allow_any_method().allowed_methods(methods);
        assert!(cors.allowed_methods.is_all());
    }

    #[test]
    fn test_cors_allow_any_header() {
        let cors = Cors::default().allow_any_header();
        assert!(cors.allowed_headers.is_all());
    }

    #[test]
    fn test_cors_allow_header_single() {
        let cors = Cors::default().allow_header("Content-Type");
        match cors.allowed_headers {
            AllOrSome::Some(headers) => {
                assert_eq!(headers.len(), 1);
                assert_eq!(headers[0], "Content-Type");
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }
    }

    #[test]
    fn test_cors_allow_header_multiple() {
        let cors = Cors::default()
            .allow_header("Content-Type")
            .allow_header("Authorization")
            .allow_header("X-Custom-Header");
        match cors.allowed_headers {
            AllOrSome::Some(headers) => {
                assert_eq!(headers.len(), 3);
                assert_eq!(headers[0], "Content-Type");
                assert_eq!(headers[1], "Authorization");
                assert_eq!(headers[2], "X-Custom-Header");
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }
    }

    #[test]
    fn test_cors_allow_header_after_allow_any_has_no_effect() {
        let cors = Cors::default()
            .allow_any_header()
            .allow_header("Content-Type");
        assert!(cors.allowed_headers.is_all());
    }

    #[test]
    fn test_cors_allowed_headers_iterator() {
        let headers = vec!["Content-Type", "Authorization", "X-Api-Key"];
        let cors = Cors::default().allowed_headers(headers);
        match cors.allowed_headers {
            AllOrSome::Some(result) => {
                assert_eq!(result.len(), 3);
                assert_eq!(result[0], "Content-Type");
                assert_eq!(result[1], "Authorization");
                assert_eq!(result[2], "X-Api-Key");
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }
    }

    #[test]
    fn test_cors_allowed_headers_after_allow_any_has_no_effect() {
        let headers = vec!["Content-Type", "Authorization"];
        let cors = Cors::default().allow_any_header().allowed_headers(headers);
        assert!(cors.allowed_headers.is_all());
    }

    #[test]
    fn test_cors_expose_any_header() {
        let cors = Cors::default().expose_any_header();
        assert!(cors.expose_headers.is_all());
    }

    #[test]
    fn test_cors_expose_header_single() {
        let cors = Cors::default().expose_header("X-Custom-Header");
        match cors.expose_headers {
            AllOrSome::Some(headers) => {
                assert_eq!(headers.len(), 1);
                assert_eq!(headers[0], "X-Custom-Header");
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }
    }

    #[test]
    fn test_cors_expose_header_multiple() {
        let cors = Cors::default()
            .expose_header("X-Custom-Header")
            .expose_header("X-Request-Id")
            .expose_header("X-Rate-Limit");
        match cors.expose_headers {
            AllOrSome::Some(headers) => {
                assert_eq!(headers.len(), 3);
                assert_eq!(headers[0], "X-Custom-Header");
                assert_eq!(headers[1], "X-Request-Id");
                assert_eq!(headers[2], "X-Rate-Limit");
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }
    }

    #[test]
    fn test_cors_expose_header_after_expose_any_has_no_effect() {
        let cors = Cors::default()
            .expose_any_header()
            .expose_header("X-Custom-Header");
        assert!(cors.expose_headers.is_all());
    }

    #[test]
    fn test_cors_expose_headers_iterator() {
        let headers = vec!["X-Header-1", "X-Header-2", "X-Header-3"];
        let cors = Cors::default().expose_headers(headers);
        match cors.expose_headers {
            AllOrSome::Some(result) => {
                assert_eq!(result.len(), 3);
                assert_eq!(result[0], "X-Header-1");
                assert_eq!(result[1], "X-Header-2");
                assert_eq!(result[2], "X-Header-3");
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }
    }

    #[test]
    fn test_cors_expose_headers_after_expose_any_has_no_effect() {
        let headers = vec!["X-Header-1", "X-Header-2"];
        let cors = Cors::default().expose_any_header().expose_headers(headers);
        assert!(cors.expose_headers.is_all());
    }

    #[test]
    fn test_cors_support_credentials() {
        let cors = Cors::default().support_credentials();
        assert!(cors.supports_credentials);
    }

    #[test]
    fn test_cors_max_age() {
        let cors = Cors::default().max_age(3600);
        assert_eq!(cors.max_age, Some(3600));
    }

    #[test]
    fn test_cors_max_age_with_option() {
        let cors = Cors::default().max_age(Some(7200));
        assert_eq!(cors.max_age, Some(7200));

        let cors_none = Cors::default().max_age(None);
        assert_eq!(cors_none.max_age, None);
    }

    #[test]
    fn test_cors_builder_chain_permissive() {
        let cors = Cors::default()
            .allow_any_origin()
            .allow_any_method()
            .allow_any_header()
            .expose_any_header()
            .support_credentials()
            .max_age(3600);

        assert!(cors.allowed_origins.is_all());
        assert!(cors.allowed_methods.is_all());
        assert!(cors.allowed_headers.is_all());
        assert!(cors.expose_headers.is_all());
        assert!(cors.supports_credentials);
        assert_eq!(cors.max_age, Some(3600));
    }

    #[test]
    fn test_cors_builder_chain_restrictive() {
        let cors = Cors::default()
            .allow_origin("https://example.com")
            .allow_method(Method::Get)
            .allow_method(Method::Post)
            .allow_header("Content-Type")
            .allow_header("Authorization")
            .expose_header("X-Request-Id")
            .support_credentials()
            .max_age(1800);

        match cors.allowed_origins {
            AllOrSome::Some(origins) => {
                assert_eq!(origins.len(), 1);
                assert_eq!(origins[0], "https://example.com");
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }

        match cors.allowed_methods {
            AllOrSome::Some(methods) => {
                assert_eq!(methods.len(), 2);
                assert_eq!(methods[0], Method::Get);
                assert_eq!(methods[1], Method::Post);
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }

        match cors.allowed_headers {
            AllOrSome::Some(headers) => {
                assert_eq!(headers.len(), 2);
                assert_eq!(headers[0], "Content-Type");
                assert_eq!(headers[1], "Authorization");
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }

        match cors.expose_headers {
            AllOrSome::Some(headers) => {
                assert_eq!(headers.len(), 1);
                assert_eq!(headers[0], "X-Request-Id");
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }

        assert!(cors.supports_credentials);
        assert_eq!(cors.max_age, Some(1800));
    }

    #[test]
    fn test_cors_combining_individual_and_iterator_methods() {
        let cors = Cors::default()
            .allow_origin("https://example.com")
            .allowed_origins(vec!["https://test.com", "https://demo.com"])
            .allow_method(Method::Get)
            .allowed_methods(vec![Method::Post, Method::Put])
            .allow_header("Content-Type")
            .allowed_headers(vec!["Authorization", "X-Api-Key"]);

        match cors.allowed_origins {
            AllOrSome::Some(origins) => {
                assert_eq!(origins.len(), 3);
                assert_eq!(origins[0], "https://example.com");
                assert_eq!(origins[1], "https://test.com");
                assert_eq!(origins[2], "https://demo.com");
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }

        match cors.allowed_methods {
            AllOrSome::Some(methods) => {
                assert_eq!(methods.len(), 3);
                assert_eq!(methods[0], Method::Get);
                assert_eq!(methods[1], Method::Post);
                assert_eq!(methods[2], Method::Put);
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }

        match cors.allowed_headers {
            AllOrSome::Some(headers) => {
                assert_eq!(headers.len(), 3);
                assert_eq!(headers[0], "Content-Type");
                assert_eq!(headers[1], "Authorization");
                assert_eq!(headers[2], "X-Api-Key");
            }
            AllOrSome::All => panic!("Expected Some, got All"),
        }
    }
}

/// CORS (Cross-Origin Resource Sharing) configuration.
///
/// Defines policies for allowing cross-origin requests, including which origins,
/// methods, and headers are permitted.
#[derive(Debug, Clone)]
pub struct Cors {
    /// Origins allowed to make cross-origin requests.
    pub allowed_origins: AllOrSome<Vec<String>>,

    /// HTTP methods allowed for cross-origin requests.
    pub allowed_methods: AllOrSome<Vec<Method>>,

    /// Headers allowed in cross-origin requests.
    pub allowed_headers: AllOrSome<Vec<String>>,

    /// Headers that should be exposed to the browser.
    pub expose_headers: AllOrSome<Vec<String>>,

    /// Whether credentials (cookies, authorization headers) are supported.
    pub supports_credentials: bool,

    /// Maximum age in seconds for preflight request caching.
    pub max_age: Option<u32>,
}

/// Creates a restrictive default CORS configuration.
///
/// By default, no origins, methods, or headers are allowed. Use the builder
/// methods to configure the allowed cross-origin behavior.
#[allow(clippy::derivable_impls)]
impl Default for Cors {
    fn default() -> Self {
        Self {
            allowed_origins: AllOrSome::Some(vec![]),
            allowed_methods: AllOrSome::Some(vec![]),
            allowed_headers: AllOrSome::Some(vec![]),
            expose_headers: AllOrSome::Some(vec![]),
            supports_credentials: false,
            max_age: None,
        }
    }
}

impl Cors {
    /// Allows requests from any origin (sets `Access-Control-Allow-Origin: *`).
    #[must_use]
    pub fn allow_any_origin(mut self) -> Self {
        self.allowed_origins = AllOrSome::All;
        self
    }

    /// Adds an allowed origin. Has no effect if [`allow_any_origin`](Self::allow_any_origin) was called.
    #[must_use]
    pub fn allow_origin<T: Into<String>>(mut self, origin: T) -> Self {
        match &mut self.allowed_origins {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.push(origin.into());
            }
        }

        self
    }

    /// Adds multiple allowed origins. Has no effect if [`allow_any_origin`](Self::allow_any_origin) was called.
    #[must_use]
    pub fn allowed_origins<T: Into<String>, I: IntoIterator<Item = T>>(
        mut self,
        origins: I,
    ) -> Self {
        match &mut self.allowed_origins {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.extend(origins.into_iter().map(Into::into));
            }
        }

        self
    }

    /// Allows any HTTP method (sets `Access-Control-Allow-Methods: *`).
    #[must_use]
    pub fn allow_any_method(mut self) -> Self {
        self.allowed_methods = AllOrSome::All;
        self
    }

    /// Adds an allowed HTTP method. Has no effect if [`allow_any_method`](Self::allow_any_method) was called.
    #[must_use]
    pub fn allow_method<T: Into<Method>>(mut self, method: T) -> Self {
        match &mut self.allowed_methods {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.push(method.into());
            }
        }

        self
    }

    /// Adds multiple allowed HTTP methods. Has no effect if [`allow_any_method`](Self::allow_any_method) was called.
    #[must_use]
    pub fn allowed_methods<T: Into<Method>, I: IntoIterator<Item = T>>(
        mut self,
        methods: I,
    ) -> Self {
        match &mut self.allowed_methods {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.extend(methods.into_iter().map(Into::into));
            }
        }

        self
    }

    /// Allows any header (sets `Access-Control-Allow-Headers: *`).
    #[must_use]
    pub fn allow_any_header(mut self) -> Self {
        self.allowed_headers = AllOrSome::All;
        self
    }

    /// Adds an allowed request header. Has no effect if [`allow_any_header`](Self::allow_any_header) was called.
    #[must_use]
    pub fn allow_header<T: Into<String>>(mut self, header: T) -> Self {
        match &mut self.allowed_headers {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.push(header.into());
            }
        }

        self
    }

    /// Adds multiple allowed request headers. Has no effect if [`allow_any_header`](Self::allow_any_header) was called.
    #[must_use]
    pub fn allowed_headers<T: Into<String>, I: IntoIterator<Item = T>>(
        mut self,
        headers: I,
    ) -> Self {
        match &mut self.allowed_headers {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.extend(headers.into_iter().map(Into::into));
            }
        }

        self
    }

    /// Exposes any response header (sets `Access-Control-Expose-Headers: *`).
    #[must_use]
    pub fn expose_any_header(mut self) -> Self {
        self.expose_headers = AllOrSome::All;
        self
    }

    /// Adds a response header to expose. Has no effect if [`expose_any_header`](Self::expose_any_header) was called.
    #[must_use]
    pub fn expose_header<T: Into<String>>(mut self, header: T) -> Self {
        match &mut self.expose_headers {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.push(header.into());
            }
        }

        self
    }

    /// Adds multiple response headers to expose. Has no effect if [`expose_any_header`](Self::expose_any_header) was called.
    #[must_use]
    pub fn expose_headers<T: Into<String>, I: IntoIterator<Item = T>>(
        mut self,
        headers: I,
    ) -> Self {
        match &mut self.expose_headers {
            AllOrSome::All => {}
            AllOrSome::Some(existing) => {
                existing.extend(headers.into_iter().map(Into::into));
            }
        }

        self
    }

    /// Enables credentials support (sets `Access-Control-Allow-Credentials: true`).
    #[must_use]
    pub const fn support_credentials(mut self) -> Self {
        self.supports_credentials = true;
        self
    }

    /// Sets the maximum age for preflight request caching in seconds.
    #[must_use]
    pub fn max_age(mut self, max_age: impl Into<Option<u32>>) -> Self {
        self.max_age = max_age.into();
        self
    }
}
