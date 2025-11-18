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
#[test]
fn tests() {
    assert!(AllOrSome::<()>::All.is_all());
    assert!(!AllOrSome::<()>::All.is_some());

    assert!(!AllOrSome::Some(()).is_all());
    assert!(AllOrSome::Some(()).is_some());
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
