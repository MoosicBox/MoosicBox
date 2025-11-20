//! Async routing system for `HyperChad` applications with request handling and navigation.
//!
//! This crate provides a comprehensive routing solution with support for:
//!
//! * Flexible route matching (exact paths, multiple alternatives, prefix matching)
//! * Async request handling with full HTTP method support
//! * Request body parsing (JSON, URL-encoded forms, multipart forms with file uploads)
//! * Client information detection and request metadata
//! * Programmatic navigation with content delivery channels
//!
//! # Features
//!
//! * **`serde`** - Enable JSON and form parsing (enabled by default)
//! * **`form`** - Enable multipart form support (enabled by default)
//! * **`static-routes`** - Enable static route compilation (enabled by default)
//! * **`json`** - Enable JSON content support (enabled by default)
//! * **`format`** - Enable HTML formatting (enabled by default)
//! * **`syntax-highlighting`** - Enable syntax highlighting support
//! * **`simd`** - Enable SIMD optimizations
//!
//! # Basic Example
//!
//! ```rust
//! use hyperchad_router::Router;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a router with some routes
//! let router = Router::new()
//!     .with_route("/", |_req| async {
//!         "<h1>Home</h1>".to_string()
//!     })
//!     .with_route("/about", |_req| async {
//!         "<h1>About</h1>".to_string()
//!     });
//!
//! // Navigate to a route
//! let content = router.navigate("/").await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Route Patterns
//!
//! Routes can match in different ways:
//!
//! ```rust
//! use hyperchad_router::{Router, RoutePath};
//!
//! # async fn example() {
//! let router = Router::new()
//!     // Exact path match
//!     .with_route("/home", |_req| async { "Home".to_string() })
//!     // Multiple alternative paths
//!     .with_route(&["/api/v1", "/api/v2"][..], |_req| async { "API".to_string() })
//!     // Prefix match for static files
//!     .with_route(RoutePath::LiteralPrefix("/static/".to_string()), |_req| async {
//!         "Static content".to_string()
//!     });
//! # }
//! ```
//!
//! # Request Handling
//!
//! Handle requests with full access to HTTP method, headers, query parameters, and body:
//!
//! ```rust
//! # #[cfg(all(feature = "serde", feature = "form"))]
//! # {
//! use hyperchad_router::Router;
//! use serde::Deserialize;
//! use switchy::http::models::Method;
//!
//! #[derive(Deserialize)]
//! struct LoginForm {
//!     username: String,
//!     password: String,
//! }
//!
//! # async fn example() {
//! let router = Router::new()
//!     .with_route_result("/login", |req| async move {
//!         if req.method == Method::Post {
//!             let form: LoginForm = req.parse_form()?;
//!             Ok::<_, Box<dyn std::error::Error>>(format!("Welcome, {}!", form.username))
//!         } else {
//!             Ok("<form>...</form>".to_string())
//!         }
//!     });
//! # }
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeMap,
    pin::Pin,
    sync::{Arc, RwLock},
};

use bytes::Bytes;
use flume::{Receiver, Sender};
use futures::Future;
use hyperchad_renderer::Content;
pub use hyperchad_transformer::{Container, Element};
use qstring::QString;
use switchy::http::models::Method;
use switchy_async::task::JoinHandle;
use thiserror::Error;

/// Default client information based on the current operating system.
///
/// This is lazily initialized on first access and provides OS information
/// for the default [`ClientInfo`].
pub static DEFAULT_CLIENT_INFO: std::sync::LazyLock<std::sync::Arc<ClientInfo>> =
    std::sync::LazyLock::new(|| {
        let os_name = os_info::get().os_type().to_string();
        std::sync::Arc::new(ClientInfo {
            os: ClientOs { name: os_name },
        })
    });

/// A route handler function type.
///
/// Route handlers take a [`RouteRequest`] and return a future that resolves to
/// an optional [`Content`] or an error.
pub type RouteFunc = Arc<
    Box<
        dyn (Fn(
                RouteRequest,
            ) -> Pin<
                Box<
                    dyn Future<Output = Result<Option<Content>, Box<dyn std::error::Error>>> + Send,
                >,
            >) + Send
            + Sync,
    >,
>;

/// Errors that can occur when parsing request data.
#[cfg(feature = "serde")]
#[derive(Debug, Error)]
pub enum ParseError {
    /// JSON deserialization error.
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    /// URL-encoded form deserialization error.
    #[error(transparent)]
    SerdeUrlEncoded(#[from] serde_urlencoded::de::Error),
    /// Request body is missing.
    #[error("Missing body")]
    MissingBody,
    /// Content-Type header is invalid or unsupported.
    #[error("Invalid Content-Type")]
    InvalidContentType,
    /// I/O error during form parsing.
    #[cfg(feature = "form")]
    #[error(transparent)]
    IO(#[from] std::io::Error),
    /// Multipart form is missing boundary parameter.
    #[cfg(feature = "form")]
    #[error("Missing boundary")]
    MissingBoundary,
    /// UTF-8 parsing error.
    #[cfg(feature = "form")]
    #[error(transparent)]
    ParseUtf8(#[from] std::string::FromUtf8Error),
    /// Multipart parsing error.
    #[cfg(feature = "form")]
    #[error(transparent)]
    Multipart(#[from] mime_multipart::Error),
    /// Content-Disposition header is invalid.
    #[cfg(feature = "form")]
    #[error("Invalid Content‑Disposition")]
    InvalidContentDisposition,
    /// Custom deserialization error.
    #[cfg(feature = "form")]
    #[error("Custom deserialization error: {0}")]
    CustomDeserialize(String),
}

#[cfg(feature = "form")]
/// Serde deserializers for multipart form data.
///
/// This module provides custom deserializers for converting form field data into
/// strongly-typed Rust structures.
mod form_deserializer {
    use serde::de::{self, Deserializer, IntoDeserializer, MapAccess, Visitor};
    use std::collections::BTreeMap;
    use std::fmt;

    /// Deserializer for multipart form data.
    ///
    /// Converts a map of form fields into a Rust structure using serde.
    pub struct FormDataDeserializer {
        fields: std::collections::btree_map::IntoIter<String, String>,
    }

    impl FormDataDeserializer {
        /// Create a new form data deserializer from a map of field names to values.
        pub fn new(data: BTreeMap<String, String>) -> Self {
            Self {
                fields: data.into_iter(),
            }
        }
    }

    /// Deserializer for individual form field string values.
    ///
    /// Attempts to parse string values into appropriate types with automatic
    /// type inference for booleans, numbers, and strings.
    pub struct StringValueDeserializer {
        value: String,
    }

    impl StringValueDeserializer {
        /// Create a new string value deserializer.
        #[allow(clippy::missing_const_for_fn)]
        pub fn new(value: String) -> Self {
            Self { value }
        }
    }

    /// Deserialization error for form data.
    #[derive(Debug)]
    pub struct DeserializeError(String);

    impl fmt::Display for DeserializeError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    impl std::error::Error for DeserializeError {}

    impl de::Error for DeserializeError {
        fn custom<T: fmt::Display>(msg: T) -> Self {
            Self(msg.to_string())
        }
    }

    macro_rules! deserialize_primitive {
        ($method:ident, $visit:ident, $ty:ty) => {
            fn $method<V>(self, visitor: V) -> Result<V::Value, Self::Error>
            where
                V: Visitor<'de>,
            {
                self.value
                    .parse::<$ty>()
                    .map_err(|e| {
                        de::Error::custom(format!(
                            "failed to parse '{}' as {}: {}",
                            self.value,
                            stringify!($ty),
                            e
                        ))
                    })
                    .and_then(|v| visitor.$visit(v))
            }
        };
    }

    impl<'de> Deserializer<'de> for StringValueDeserializer {
        type Error = DeserializeError;

        fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            if self.value.eq_ignore_ascii_case("true") {
                return visitor.visit_bool(true);
            }
            if self.value.eq_ignore_ascii_case("false") {
                return visitor.visit_bool(false);
            }

            if self.value.eq_ignore_ascii_case("null") {
                return visitor.visit_unit();
            }

            if let Ok(v) = self.value.parse::<u64>() {
                return visitor.visit_u64(v);
            }

            if let Ok(v) = self.value.parse::<i64>() {
                return visitor.visit_i64(v);
            }

            if let Ok(v) = self.value.parse::<f64>() {
                return visitor.visit_f64(v);
            }

            visitor.visit_string(self.value)
        }

        deserialize_primitive!(deserialize_bool, visit_bool, bool);
        deserialize_primitive!(deserialize_i8, visit_i8, i8);
        deserialize_primitive!(deserialize_i16, visit_i16, i16);
        deserialize_primitive!(deserialize_i32, visit_i32, i32);
        deserialize_primitive!(deserialize_i64, visit_i64, i64);
        deserialize_primitive!(deserialize_i128, visit_i128, i128);
        deserialize_primitive!(deserialize_u8, visit_u8, u8);
        deserialize_primitive!(deserialize_u16, visit_u16, u16);
        deserialize_primitive!(deserialize_u32, visit_u32, u32);
        deserialize_primitive!(deserialize_u64, visit_u64, u64);
        deserialize_primitive!(deserialize_u128, visit_u128, u128);
        deserialize_primitive!(deserialize_f32, visit_f32, f32);
        deserialize_primitive!(deserialize_f64, visit_f64, f64);

        fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            if self.value.len() == 1 {
                visitor.visit_char(self.value.chars().next().unwrap())
            } else {
                Err(de::Error::custom(format!(
                    "expected single character, got '{}'",
                    self.value
                )))
            }
        }

        fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_string(self.value)
        }

        fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_string(self.value)
        }

        fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_byte_buf(self.value.into_bytes())
        }

        fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_byte_buf(self.value.into_bytes())
        }

        fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            if self.value.is_empty() || self.value.eq_ignore_ascii_case("null") {
                visitor.visit_none()
            } else {
                visitor.visit_some(self)
            }
        }

        fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_unit()
        }

        fn deserialize_unit_struct<V>(
            self,
            _name: &'static str,
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_unit()
        }

        fn deserialize_newtype_struct<V>(
            self,
            _name: &'static str,
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_newtype_struct(self)
        }

        fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.value.into_deserializer().deserialize_seq(visitor)
        }

        fn deserialize_tuple<V>(self, _len: usize, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.deserialize_seq(visitor)
        }

        fn deserialize_tuple_struct<V>(
            self,
            _name: &'static str,
            _len: usize,
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.deserialize_seq(visitor)
        }

        fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.value.into_deserializer().deserialize_map(visitor)
        }

        fn deserialize_struct<V>(
            self,
            _name: &'static str,
            _fields: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.deserialize_map(visitor)
        }

        fn deserialize_enum<V>(
            self,
            _name: &'static str,
            _variants: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_enum(self.value.into_deserializer())
        }

        fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_string(self.value)
        }

        fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_unit()
        }
    }

    /// Map accessor for iterating over form fields during deserialization.
    struct FieldsMapAccess {
        fields: std::collections::btree_map::IntoIter<String, String>,
        value: Option<String>,
    }

    impl FieldsMapAccess {
        /// Create a new map accessor from a form field iterator.
        #[allow(clippy::missing_const_for_fn)]
        fn new(fields: std::collections::btree_map::IntoIter<String, String>) -> Self {
            Self {
                fields,
                value: None,
            }
        }
    }

    impl<'de> MapAccess<'de> for FieldsMapAccess {
        type Error = DeserializeError;

        fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
        where
            K: de::DeserializeSeed<'de>,
        {
            if let Some((key, value)) = self.fields.next() {
                self.value = Some(value);
                seed.deserialize(key.into_deserializer()).map(Some)
            } else {
                Ok(None)
            }
        }

        fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
        where
            V: de::DeserializeSeed<'de>,
        {
            let value = self
                .value
                .take()
                .ok_or_else(|| de::Error::custom("value is missing"))?;
            seed.deserialize(StringValueDeserializer::new(value))
        }
    }

    impl<'de> Deserializer<'de> for FormDataDeserializer {
        type Error = DeserializeError;

        fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.deserialize_map(visitor)
        }

        fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_map(FieldsMapAccess::new(self.fields))
        }

        fn deserialize_struct<V>(
            self,
            _name: &'static str,
            _fields: &'static [&'static str],
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            self.deserialize_map(visitor)
        }

        fn deserialize_bool<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize bool from form data map",
            ))
        }

        fn deserialize_i8<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize i8 from form data map",
            ))
        }

        fn deserialize_i16<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize i16 from form data map",
            ))
        }

        fn deserialize_i32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize i32 from form data map",
            ))
        }

        fn deserialize_i64<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize i64 from form data map",
            ))
        }

        fn deserialize_i128<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize i128 from form data map",
            ))
        }

        fn deserialize_u8<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize u8 from form data map",
            ))
        }

        fn deserialize_u16<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize u16 from form data map",
            ))
        }

        fn deserialize_u32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize u32 from form data map",
            ))
        }

        fn deserialize_u64<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize u64 from form data map",
            ))
        }

        fn deserialize_u128<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize u128 from form data map",
            ))
        }

        fn deserialize_f32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize f32 from form data map",
            ))
        }

        fn deserialize_f64<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize f64 from form data map",
            ))
        }

        fn deserialize_char<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize char from form data map",
            ))
        }

        fn deserialize_str<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize str from form data map",
            ))
        }

        fn deserialize_string<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize string from form data map",
            ))
        }

        fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize bytes from form data map",
            ))
        }

        fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize byte_buf from form data map",
            ))
        }

        fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_some(self)
        }

        fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_unit()
        }

        fn deserialize_unit_struct<V>(
            self,
            _name: &'static str,
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_unit()
        }

        fn deserialize_newtype_struct<V>(
            self,
            _name: &'static str,
            visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_newtype_struct(self)
        }

        fn deserialize_seq<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize seq from form data map",
            ))
        }

        fn deserialize_tuple<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize tuple from form data map",
            ))
        }

        fn deserialize_tuple_struct<V>(
            self,
            _name: &'static str,
            _len: usize,
            _visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize tuple_struct from form data map",
            ))
        }

        fn deserialize_enum<V>(
            self,
            _name: &'static str,
            _variants: &'static [&'static str],
            _visitor: V,
        ) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize enum from form data map",
            ))
        }

        fn deserialize_identifier<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            Err(de::Error::custom(
                "cannot deserialize identifier from form data map",
            ))
        }

        fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: Visitor<'de>,
        {
            visitor.visit_unit()
        }
    }
}

/// Client operating system information.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClientOs {
    /// Operating system name.
    pub name: String,
}

/// Information about the client making a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientInfo {
    /// Client operating system.
    pub os: ClientOs,
}

impl Default for ClientInfo {
    fn default() -> Self {
        DEFAULT_CLIENT_INFO.as_ref().clone()
    }
}

/// Metadata about the request context.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RequestInfo {
    /// Client making the request.
    pub client: Arc<ClientInfo>,
}

/// An HTTP request for routing.
///
/// Contains all the information needed to handle an HTTP request including
/// path, method, query parameters, headers, cookies, and body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteRequest {
    /// Request path.
    pub path: String,
    /// HTTP method.
    pub method: Method,
    /// Query string parameters.
    pub query: BTreeMap<String, String>,
    /// HTTP headers.
    pub headers: BTreeMap<String, String>,
    /// HTTP cookies.
    pub cookies: BTreeMap<String, String>,
    /// Request metadata.
    pub info: RequestInfo,
    /// Request body bytes.
    pub body: Option<Arc<Bytes>>,
}

impl RouteRequest {
    /// Create a `RouteRequest` from a path string and request info.
    ///
    /// If the path contains a query string (indicated by `?`), it will be
    /// parsed and stored in the `query` field.
    #[must_use]
    pub fn from_path(path: &str, info: RequestInfo) -> Self {
        let (path, query) = if let Some((path, query)) = path.split_once('?') {
            (path, query)
        } else {
            (path, "")
        };

        Self {
            path: path.to_owned(),
            method: Method::Get,
            query: QString::from(query).into_iter().collect(),
            headers: BTreeMap::new(),
            cookies: BTreeMap::new(),
            info,
            body: None,
        }
    }

    /// Get the Content-Type header value.
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.headers.get("content-type").map(String::as_str)
    }

    /// Parse multipart form data from the request body.
    ///
    /// # Errors
    ///
    /// * [`ParseError::MissingBody`] - The request body is missing
    /// * [`ParseError::InvalidContentType`] - The Content-Type header is missing or invalid
    /// * [`ParseError::Multipart`] - Failed to parse multipart form data
    /// * [`ParseError::InvalidContentDisposition`] - Content-Disposition header is invalid or missing
    /// * [`ParseError::ParseUtf8`] - Failed to parse form field as UTF-8
    /// * [`ParseError::IO`] - I/O error reading uploaded file
    /// * [`ParseError::CustomDeserialize`] - Failed to deserialize form data into the target type
    #[cfg(feature = "form")]
    pub fn parse_form<T: serde::de::DeserializeOwned>(&self) -> Result<T, ParseError> {
        use std::io::{Cursor, Read as _};

        use base64::engine::{Engine as _, general_purpose};
        use hyper_old::header::{ContentDisposition, ContentType, DispositionParam, Headers};
        use mime_multipart::{Node, read_multipart_body};
        use mime_old::Mime;

        fn parse_multipart_form_data(
            body: &[u8],
            content_type: &str,
        ) -> Result<BTreeMap<String, String>, ParseError> {
            fn process_nodes(
                nodes: Vec<Node>,
                map: &mut BTreeMap<String, String>,
            ) -> Result<(), ParseError> {
                for node in nodes {
                    match node {
                        Node::Part(part) => {
                            let cd = part
                                .headers
                                .get::<ContentDisposition>()
                                .ok_or(ParseError::InvalidContentDisposition)?;
                            let field_name = cd
                                .parameters
                                .iter()
                                .find_map(|param| {
                                    if let DispositionParam::Ext(key, val) = param
                                        && key.eq_ignore_ascii_case("name")
                                    {
                                        return Some(val.clone());
                                    }
                                    None
                                })
                                .ok_or(ParseError::InvalidContentDisposition)?;

                            let text = String::from_utf8(part.body)?;
                            map.insert(field_name, text);
                        }

                        Node::File(filepart) => {
                            let cd = filepart
                                .headers
                                .get::<ContentDisposition>()
                                .ok_or(ParseError::InvalidContentDisposition)?;
                            let field_name = cd
                                .parameters
                                .iter()
                                .find_map(|param| {
                                    if let DispositionParam::Ext(key, val) = param
                                        && key.eq_ignore_ascii_case("name")
                                    {
                                        return Some(val.clone());
                                    }
                                    None
                                })
                                .ok_or(ParseError::InvalidContentDisposition)?;

                            let mut f = std::fs::File::open(&filepart.path)?;
                            let mut data = Vec::new();
                            f.read_to_end(&mut data)?;

                            let b64 = general_purpose::STANDARD.encode(&data);
                            map.insert(field_name, b64);
                        }

                        Node::Multipart((_hdrs, subparts)) => {
                            process_nodes(subparts, map)?;
                        }
                    }
                }
                Ok(())
            }

            let mut headers = Headers::new();
            let mime_type: Mime = content_type
                .parse()
                .map_err(|()| ParseError::InvalidContentType)?;
            headers.set(ContentType(mime_type));

            let mut cursor = Cursor::new(body);
            let parts: Vec<Node> = read_multipart_body(&mut cursor, &headers, false)?;

            let mut map = BTreeMap::new();
            process_nodes(parts, &mut map)?;

            Ok(map)
        }

        if let Some(form) = &self.body {
            let data = parse_multipart_form_data(
                form,
                self.content_type().ok_or(ParseError::InvalidContentType)?,
            )?;
            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            T::deserialize(deserializer).map_err(|e| ParseError::CustomDeserialize(e.to_string()))
        } else {
            Err(ParseError::MissingBody)
        }
    }

    /// Parse JSON from the request body.
    ///
    /// # Errors
    ///
    /// * [`ParseError::MissingBody`] - The request body is missing
    /// * [`ParseError::SerdeJson`] - Failed to deserialize JSON data
    #[cfg(feature = "serde")]
    pub fn parse_body<T: serde::de::DeserializeOwned>(&self) -> Result<T, ParseError> {
        if let Some(body) = &self.body {
            Ok(serde_json::from_slice(body)?)
        } else {
            Err(ParseError::MissingBody)
        }
    }
}

impl From<Navigation> for RouteRequest {
    fn from(value: Navigation) -> Self {
        Self {
            path: value.0,
            method: Method::Get,
            query: BTreeMap::new(),
            headers: BTreeMap::new(),
            cookies: BTreeMap::new(),
            info: RequestInfo { client: value.1 },
            body: None,
        }
    }
}

impl From<&Navigation> for RouteRequest {
    fn from(value: &Navigation) -> Self {
        value.clone().into()
    }
}

/// A route path matcher.
///
/// Supports exact matches, multiple alternative matches, and prefix matches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoutePath {
    /// Match a single exact path.
    Literal(String),
    /// Match any of the specified paths.
    Literals(Vec<String>),
    /// Match paths that start with the specified prefix.
    LiteralPrefix(String),
}

impl RoutePath {
    /// Check if this route path matches the given path.
    #[must_use]
    pub fn matches(&self, path: &str) -> bool {
        match self {
            Self::Literal(route_path) => route_path == path,
            Self::Literals(route_paths) => route_paths.iter().any(|x| x == path),
            Self::LiteralPrefix(route_path) => path.starts_with(route_path),
        }
    }

    /// Strip the matched portion from the path.
    ///
    /// For exact matches, returns an empty string if the path matches.
    /// For prefix matches, returns the remainder after the prefix.
    /// Returns `None` if the path doesn't match.
    #[must_use]
    pub fn strip_match<'a>(&'a self, path: &'a str) -> Option<&'a str> {
        const EMPTY: &str = "";

        match self {
            Self::Literal(..) | Self::Literals(..) => {
                if self.matches(path) {
                    Some(EMPTY)
                } else {
                    None
                }
            }
            Self::LiteralPrefix(route_path) => path.strip_prefix(route_path),
        }
    }
}

impl From<&str> for RoutePath {
    fn from(value: &str) -> Self {
        Self::Literal(value.to_owned())
    }
}

impl From<&String> for RoutePath {
    fn from(value: &String) -> Self {
        Self::Literal(value.to_owned())
    }
}

impl From<&[&str; 1]> for RoutePath {
    fn from(value: &[&str; 1]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 2]> for RoutePath {
    fn from(value: &[&str; 2]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 3]> for RoutePath {
    fn from(value: &[&str; 3]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 4]> for RoutePath {
    fn from(value: &[&str; 4]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 5]> for RoutePath {
    fn from(value: &[&str; 5]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 6]> for RoutePath {
    fn from(value: &[&str; 6]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 7]> for RoutePath {
    fn from(value: &[&str; 7]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 8]> for RoutePath {
    fn from(value: &[&str; 8]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 9]> for RoutePath {
    fn from(value: &[&str; 9]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str; 10]> for RoutePath {
    fn from(value: &[&str; 10]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&str]> for RoutePath {
    fn from(value: &[&str]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<Vec<&str>> for RoutePath {
    fn from(value: Vec<&str>) -> Self {
        Self::Literals(value.into_iter().map(ToString::to_string).collect())
    }
}

impl From<String> for RoutePath {
    fn from(value: String) -> Self {
        Self::Literal(value)
    }
}

impl From<&[String]> for RoutePath {
    fn from(value: &[String]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<&[&String]> for RoutePath {
    fn from(value: &[&String]) -> Self {
        Self::Literals(value.iter().map(ToString::to_string).collect())
    }
}

impl From<Vec<String>> for RoutePath {
    fn from(value: Vec<String>) -> Self {
        Self::Literals(value)
    }
}

/// Errors that can occur during navigation.
#[derive(Debug, Error)]
pub enum NavigateError {
    /// The requested path has no registered route handler.
    #[error("Invalid path")]
    InvalidPath,
    /// The route handler returned an error.
    #[error("Handler error: {0:?}")]
    Handler(Box<dyn std::error::Error + Send + Sync>),
    /// Failed to send navigation result through channel.
    #[error("Sender error")]
    Sender,
}

/// HTTP router for handling requests and navigation.
///
/// The router manages route registration and dispatching requests to
/// appropriate handlers. Routes can be dynamic or static (with the
/// `static-routes` feature).
#[derive(Clone)]
pub struct Router {
    /// Static route handlers (enabled with `static-routes` feature).
    #[cfg(feature = "static-routes")]
    pub static_routes: Arc<RwLock<Vec<(RoutePath, RouteFunc)>>>,
    /// Dynamic route handlers.
    pub routes: Arc<RwLock<Vec<(RoutePath, RouteFunc)>>>,
    sender: Sender<Content>,
    /// Receiver for navigation content.
    pub receiver: Receiver<Content>,
}

impl std::fmt::Debug for Router {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Router")
            .field("sender", &self.sender)
            .field("receiver", &self.receiver)
            .finish_non_exhaustive()
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

/// A navigation request consisting of a path and client information.
///
/// This is a lightweight wrapper type used for programmatic navigation.
/// It contains the target path and information about the client making the request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Navigation(String, Arc<ClientInfo>);

impl From<RouteRequest> for Navigation {
    fn from(value: RouteRequest) -> Self {
        let mut query = String::new();

        for (key, value) in &value.query {
            if query.is_empty() {
                query.push('?');
            } else {
                query.push('&');
            }
            query.push_str(key);
            query.push('=');
            query.push_str(value);
        }

        Self(format!("{}{query}", value.path), value.info.client)
    }
}

impl From<&str> for RouteRequest {
    fn from(value: &str) -> Self {
        value.to_string().into()
    }
}

impl From<String> for RouteRequest {
    fn from(value: String) -> Self {
        Self {
            path: value,
            method: Method::Get,
            query: BTreeMap::new(),
            headers: BTreeMap::new(),
            cookies: BTreeMap::new(),
            info: RequestInfo::default(),
            body: None,
        }
    }
}

impl From<&String> for RouteRequest {
    fn from(value: &String) -> Self {
        value.clone().into()
    }
}

impl From<(&str, ClientInfo)> for RouteRequest {
    fn from(value: (&str, ClientInfo)) -> Self {
        (value.0.to_string(), Arc::new(value.1)).into()
    }
}

impl From<(String, ClientInfo)> for RouteRequest {
    fn from(value: (String, ClientInfo)) -> Self {
        (value.0, Arc::new(value.1)).into()
    }
}

impl From<(&String, ClientInfo)> for RouteRequest {
    fn from(value: (&String, ClientInfo)) -> Self {
        (value.0.clone(), Arc::new(value.1)).into()
    }
}

impl From<(&str, Arc<ClientInfo>)> for RouteRequest {
    fn from(value: (&str, Arc<ClientInfo>)) -> Self {
        (value.0.to_string(), value.1).into()
    }
}

impl From<(String, Arc<ClientInfo>)> for RouteRequest {
    fn from(value: (String, Arc<ClientInfo>)) -> Self {
        (value.0, RequestInfo { client: value.1 }).into()
    }
}

impl From<(&String, Arc<ClientInfo>)> for RouteRequest {
    fn from(value: (&String, Arc<ClientInfo>)) -> Self {
        (value.0.clone(), value.1).into()
    }
}

impl From<(&str, RequestInfo)> for RouteRequest {
    fn from(value: (&str, RequestInfo)) -> Self {
        (value.0.to_string(), value.1).into()
    }
}

impl From<(String, RequestInfo)> for RouteRequest {
    fn from(value: (String, RequestInfo)) -> Self {
        let (path, query) = if let Some((path, query)) = value.0.split_once('?') {
            (path.to_string(), query)
        } else {
            (value.0, "")
        };

        Self {
            path,
            method: Method::Get,
            query: QString::from(query).into_iter().collect(),
            headers: BTreeMap::new(),
            cookies: BTreeMap::new(),
            info: value.1,
            body: None,
        }
    }
}

impl From<(&String, RequestInfo)> for RouteRequest {
    fn from(value: (&String, RequestInfo)) -> Self {
        (value.0.clone(), value.1).into()
    }
}

impl From<&RouteRequest> for Navigation {
    fn from(value: &RouteRequest) -> Self {
        value.clone().into()
    }
}

impl From<&str> for Navigation {
    fn from(value: &str) -> Self {
        Self(value.to_string(), DEFAULT_CLIENT_INFO.clone())
    }
}

impl From<String> for Navigation {
    fn from(value: String) -> Self {
        Self(value, DEFAULT_CLIENT_INFO.clone())
    }
}

impl From<&String> for Navigation {
    fn from(value: &String) -> Self {
        Self(value.clone(), DEFAULT_CLIENT_INFO.clone())
    }
}

impl From<(&str, ClientInfo)> for Navigation {
    fn from(value: (&str, ClientInfo)) -> Self {
        Self(value.0.to_string(), Arc::new(value.1))
    }
}

impl From<(String, ClientInfo)> for Navigation {
    fn from(value: (String, ClientInfo)) -> Self {
        Self(value.0, Arc::new(value.1))
    }
}

impl From<(&String, ClientInfo)> for Navigation {
    fn from(value: (&String, ClientInfo)) -> Self {
        Self(value.0.clone(), Arc::new(value.1))
    }
}

impl From<(&str, Arc<ClientInfo>)> for Navigation {
    fn from(value: (&str, Arc<ClientInfo>)) -> Self {
        Self(value.0.to_string(), value.1)
    }
}

impl From<(String, Arc<ClientInfo>)> for Navigation {
    fn from(value: (String, Arc<ClientInfo>)) -> Self {
        Self(value.0, value.1)
    }
}

impl From<(&String, Arc<ClientInfo>)> for Navigation {
    fn from(value: (&String, Arc<ClientInfo>)) -> Self {
        Self(value.0.clone(), value.1)
    }
}

impl From<(&str, RequestInfo)> for Navigation {
    fn from(value: (&str, RequestInfo)) -> Self {
        Self(value.0.to_string(), value.1.client)
    }
}

impl From<(String, RequestInfo)> for Navigation {
    fn from(value: (String, RequestInfo)) -> Self {
        Self(value.0, value.1.client)
    }
}

impl From<(&String, RequestInfo)> for Navigation {
    fn from(value: (&String, RequestInfo)) -> Self {
        Self(value.0.clone(), value.1.client)
    }
}

impl Router {
    /// Create a new router with an unbounded channel for navigation events.
    #[must_use]
    pub fn new() -> Self {
        let (tx, rx) = flume::unbounded();

        Self {
            #[cfg(feature = "static-routes")]
            static_routes: Arc::new(RwLock::new(vec![])),
            routes: Arc::new(RwLock::new(vec![])),
            sender: tx,
            receiver: rx,
        }
    }

    /// Register a route with a handler that returns a `Result`.
    ///
    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    #[must_use]
    pub fn with_route_result<
        C: TryInto<Content>,
        Response: Into<Option<C>>,
        F: Future<Output = Result<Response, BoxE>> + Send + 'static,
        BoxE: Into<Box<dyn std::error::Error>>,
    >(
        self,
        route: impl Into<RoutePath>,
        handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) -> Self
    where
        C::Error: Into<Box<dyn std::error::Error>>,
    {
        self.routes
            .write()
            .unwrap()
            .push((route.into(), gen_route_func_result(handler)));
        self
    }

    /// Register a route with a handler that returns no content on success.
    ///
    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    #[must_use]
    pub fn with_no_content_result<
        F: Future<Output = Result<(), BoxE>> + Send + 'static,
        BoxE: Into<Box<dyn std::error::Error>>,
    >(
        self,
        route: impl Into<RoutePath>,
        handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) -> Self {
        self.with_route_result::<Content, Option<Content>, _, _>(route, move |req: RouteRequest| {
            let fut = handler(req);
            async move { fut.await.map(|()| None::<Content>).map_err(Into::into) }
        })
    }

    /// Register a static route with a handler that returns a `Result`.
    ///
    /// Static routes are only compiled in when the `static-routes` feature is enabled.
    ///
    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn with_static_route_result<
        C: TryInto<Content>,
        Response: Into<Option<C>>,
        F: Future<Output = Result<Response, BoxE>> + Send + 'static,
        BoxE: Into<Box<dyn std::error::Error>>,
    >(
        self,
        #[allow(unused_variables)] route: impl Into<RoutePath>,
        #[allow(unused_variables)] handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) -> Self
    where
        C::Error: Into<Box<dyn std::error::Error>>,
    {
        #[cfg(feature = "static-routes")]
        self.static_routes
            .write()
            .unwrap()
            .push((route.into(), gen_route_func_result(handler)));
        self
    }

    /// Register a route with an infallible handler.
    ///
    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    #[must_use]
    pub fn with_route<
        C: TryInto<Content>,
        Response: Into<Option<C>>,
        F: Future<Output = Response> + Send + 'static,
    >(
        self,
        route: impl Into<RoutePath>,
        handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) -> Self
    where
        C::Error: std::error::Error + 'static,
    {
        self.routes
            .write()
            .unwrap()
            .push((route.into(), gen_route_func(handler)));
        self
    }

    /// Register a static route with an infallible handler.
    ///
    /// Static routes are only compiled in when the `static-routes` feature is enabled.
    ///
    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use]
    pub fn with_static_route<
        C: TryInto<Content>,
        Response: Into<Option<C>>,
        F: Future<Output = Response> + Send + 'static,
    >(
        self,
        #[allow(unused_variables)] route: impl Into<RoutePath>,
        #[allow(unused_variables)] handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) -> Self
    where
        C::Error: std::error::Error + 'static,
    {
        #[cfg(feature = "static-routes")]
        self.static_routes
            .write()
            .unwrap()
            .push((route.into(), gen_route_func(handler)));
        self
    }

    /// Get the route handler function for a given path.
    ///
    /// Searches dynamic routes first, then static routes if enabled.
    ///
    /// # Panics
    ///
    /// * Will panic if `routes` `RwLock` is poisoned
    /// * Will panic if `static_routes` `RwLock` is poisoned (when `static-routes` feature is enabled)
    #[must_use]
    pub fn get_route_func(&self, path: &str) -> Option<RouteFunc> {
        let dyn_route = self
            .routes
            .read()
            .unwrap()
            .iter()
            .find(|(route, _)| route.matches(path))
            .cloned()
            .map(|(_, handler)| handler);

        #[cfg(feature = "static-routes")]
        if dyn_route.is_none() {
            return self
                .static_routes
                .read()
                .unwrap()
                .iter()
                .find(|(route, _)| route.matches(path))
                .cloned()
                .map(|(_, handler)| handler);
        }

        dyn_route
    }

    /// Navigate to a path and return the resulting content.
    ///
    /// # Errors
    ///
    /// * Returns [`NavigateError::InvalidPath`] if no route matches the path
    /// * Returns [`NavigateError::Handler`] if the route handler returns an error
    ///
    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    pub async fn navigate(
        &self,
        navigation: impl Into<RouteRequest>,
    ) -> Result<Option<Content>, NavigateError> {
        let req = navigation.into();

        log::debug!("navigate: method={} path={}", req.method, req.path);

        let handler = self.get_route_func(&req.path);

        Ok(if let Some(handler) = handler {
            match handler(req).await {
                Ok(view) => view,
                Err(e) => {
                    log::error!("Failed to fetch route view: {e:?}");
                    return Err(NavigateError::Handler(Box::new(std::io::Error::other(
                        e.to_string(),
                    ))));
                }
            }
        } else {
            log::warn!("Invalid navigation path={}", req.path);
            return Err(NavigateError::InvalidPath);
        })
    }

    /// Navigate to a path and send the resulting content through the channel.
    ///
    /// # Errors
    ///
    /// * Returns [`NavigateError::InvalidPath`] if no route matches the path
    /// * Returns [`NavigateError::Handler`] if the route handler returns an error
    /// * Returns [`NavigateError::Sender`] if sending through the channel fails
    ///
    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    pub async fn navigate_send(
        &self,
        navigation: impl Into<RouteRequest>,
    ) -> Result<(), NavigateError> {
        let req = navigation.into();

        log::debug!("navigate_send: method={} path={}", req.method, req.path);

        let view = {
            let handler = self.get_route_func(&req.path);

            if let Some(handler) = handler {
                match handler(req).await {
                    Ok(view) => view,
                    Err(e) => {
                        log::error!("Failed to fetch route view: {e:?}");
                        return Err(NavigateError::Handler(Box::new(std::io::Error::other(
                            e.to_string(),
                        ))));
                    }
                }
            } else {
                log::warn!("Invalid navigation path={}", req.path);
                return Err(NavigateError::InvalidPath);
            }
        };

        if let Some(view) = view {
            self.sender.send(view).map_err(|e| {
                log::error!("Failed to send: {e:?}");
                NavigateError::Sender
            })?;
        }

        Ok(())
    }

    /// Spawn a task to navigate and send the result.
    ///
    /// Uses the current async runtime handle.
    ///
    /// # Errors
    ///
    /// The returned `JoinHandle` resolves to an error if navigation fails.
    #[must_use]
    pub fn navigate_spawn(
        &self,
        navigation: impl Into<RouteRequest>,
    ) -> JoinHandle<Result<(), Box<dyn std::error::Error + Send>>> {
        let navigation = navigation.into();

        log::debug!("navigate_spawn: navigation={navigation:?}");

        self.navigate_spawn_on(&switchy_async::runtime::Handle::current(), navigation)
    }

    /// Spawn a task to navigate and send the result on a specific runtime handle.
    ///
    /// # Errors
    ///
    /// The returned `JoinHandle` resolves to an error if navigation fails.
    #[must_use]
    pub fn navigate_spawn_on(
        &self,
        handle: &switchy_async::runtime::Handle,
        navigation: impl Into<RouteRequest>,
    ) -> JoinHandle<Result<(), Box<dyn std::error::Error + Send>>> {
        let navigation = navigation.into();

        log::debug!("navigate_spawn_on: navigation={navigation:?}");

        let router = self.clone();
        handle.spawn_with_name("NativeApp navigate_spawn", async move {
            router
                .navigate_send(navigation)
                .await
                .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)
        })
    }

    /// Wait for the next navigation content from the channel.
    ///
    /// Returns `None` if the channel is closed.
    #[must_use]
    pub async fn wait_for_navigation(&self) -> Option<Content> {
        self.receiver.recv_async().await.ok()
    }

    /// Check if a dynamic route exists for the given path
    ///
    /// # Panics
    ///
    /// Will panic if `routes` `RwLock` is poisoned.
    #[must_use]
    pub fn has_route(&self, path: &str) -> bool {
        self.routes
            .read()
            .unwrap()
            .iter()
            .any(|(route, _)| route.matches(path))
    }

    /// Check if a static route exists for the given path
    ///
    /// # Panics
    ///
    /// Will panic if `static_routes` `RwLock` is poisoned.
    #[allow(clippy::missing_const_for_fn)]
    #[must_use]
    pub fn has_static_route(&self, path: &str) -> bool {
        #[cfg(feature = "static-routes")]
        {
            self.static_routes
                .read()
                .unwrap()
                .iter()
                .any(|(route, _)| route.matches(path))
        }
        #[cfg(not(feature = "static-routes"))]
        {
            let _ = path;
            false
        }
    }

    /// Add a route to an existing router (modifies in-place)
    ///
    /// # Panics
    ///
    /// Will panic if routes `RwLock` is poisoned.
    pub fn add_route_result<
        C: TryInto<Content>,
        Response: Into<Option<C>>,
        F: Future<Output = Result<Response, BoxE>> + Send + 'static,
        BoxE: Into<Box<dyn std::error::Error>>,
    >(
        &self,
        route: impl Into<RoutePath>,
        handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
    ) where
        C::Error: Into<Box<dyn std::error::Error>>,
    {
        self.routes
            .write()
            .unwrap()
            .push((route.into(), gen_route_func_result(handler)));
    }
}

/// Generate a route handler function from an infallible async handler.
///
/// Wraps the handler to convert its response into the expected [`RouteFunc`] signature.
fn gen_route_func<
    C: TryInto<Content>,
    Response: Into<Option<C>>,
    F: Future<Output = Response> + Send + 'static,
>(
    handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
) -> RouteFunc
where
    C::Error: std::error::Error + 'static,
{
    Arc::new(Box::new(move |req| {
        Box::pin({
            let handler = handler.clone();
            async move {
                let resp: Result<Option<Content>, Box<dyn std::error::Error>> = handler(req)
                    .await
                    .into()
                    .map(TryInto::try_into)
                    .transpose()
                    .map_err(|e| {
                        log::error!("Failed to handle route: {e:?}");
                        Box::new(e) as Box<dyn std::error::Error>
                    });
                resp
            }
        })
    }))
}

/// Generate a route handler function from a fallible async handler.
///
/// Wraps the handler to convert its `Result` response into the expected [`RouteFunc`] signature.
fn gen_route_func_result<
    C: TryInto<Content>,
    Response: Into<Option<C>>,
    F: Future<Output = Result<Response, BoxE>> + Send + 'static,
    BoxE: Into<Box<dyn std::error::Error>>,
>(
    handler: impl Fn(RouteRequest) -> F + Send + Sync + Clone + 'static,
) -> RouteFunc
where
    C::Error: Into<Box<dyn std::error::Error>>,
{
    Arc::new(Box::new(move |req| {
        Box::pin({
            let handler = handler.clone();
            async move {
                let resp: Result<Response, Box<dyn std::error::Error>> =
                    handler(req).await.map_err(Into::into);
                match resp.map(|x| {
                    let x: Result<Option<Content>, Box<dyn std::error::Error>> = x
                        .into()
                        .map(TryInto::try_into)
                        .transpose()
                        .map_err(Into::into);
                    x
                }) {
                    Ok(x) => match x {
                        Ok(x) => Ok(x),
                        Err(e) => Err(e),
                    },
                    Err(e) => Err(e),
                }
            }
        })
    }))
}

#[cfg(test)]
mod tests {
    #[allow(unused)]
    use super::*;

    #[cfg(feature = "form")]
    mod form_deserializer_tests {
        use super::*;
        use serde::Deserialize;
        use std::collections::BTreeMap;

        #[test]
        fn test_deserialize_primitives() {
            #[derive(Debug, Deserialize, PartialEq)]
            struct TestForm {
                age: u64,
                score: i32,
                ratio: f64,
                active: bool,
                letter: char,
            }

            let mut data = BTreeMap::new();
            data.insert("age".to_string(), "2445072108".to_string());
            data.insert("score".to_string(), "-42".to_string());
            data.insert("ratio".to_string(), "5.5".to_string());
            data.insert("active".to_string(), "true".to_string());
            data.insert("letter".to_string(), "A".to_string());

            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            let result: Result<TestForm, _> = TestForm::deserialize(deserializer);

            assert!(result.is_ok());
            let form = result.unwrap();
            assert_eq!(form.age, 2_445_072_108);
            assert_eq!(form.score, -42);
            assert!((form.ratio - 5.5).abs() < f64::EPSILON);
            assert!(form.active);
            assert_eq!(form.letter, 'A');
        }

        #[test]
        fn test_deserialize_strings() {
            #[derive(Debug, Deserialize, PartialEq)]
            struct TestForm {
                name: String,
                email: String,
            }

            let mut data = BTreeMap::new();
            data.insert("name".to_string(), "Alice".to_string());
            data.insert("email".to_string(), "alice@example.com".to_string());

            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            let result: Result<TestForm, _> = TestForm::deserialize(deserializer);

            assert!(result.is_ok());
            let form = result.unwrap();
            assert_eq!(form.name, "Alice");
            assert_eq!(form.email, "alice@example.com");
        }

        #[test]
        fn test_deserialize_options() {
            #[allow(clippy::struct_field_names)]
            #[derive(Debug, Deserialize, PartialEq)]
            struct TestForm {
                optional_field: Option<u64>,
                empty_field: Option<String>,
                null_field: Option<i32>,
                present_field: Option<String>,
            }

            let mut data = BTreeMap::new();
            data.insert("optional_field".to_string(), "123".to_string());
            data.insert("empty_field".to_string(), String::new());
            data.insert("null_field".to_string(), "null".to_string());
            data.insert("present_field".to_string(), "value".to_string());

            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            let result: Result<TestForm, _> = TestForm::deserialize(deserializer);

            assert!(result.is_ok());
            let form = result.unwrap();
            assert_eq!(form.optional_field, Some(123));
            assert_eq!(form.empty_field, None);
            assert_eq!(form.null_field, None);
            assert_eq!(form.present_field, Some("value".to_string()));
        }

        #[test]
        fn test_deserialize_all_integer_types() {
            #[allow(clippy::struct_field_names)]
            #[derive(Debug, Deserialize, PartialEq)]
            struct TestForm {
                u8_field: u8,
                u16_field: u16,
                u32_field: u32,
                u64_field: u64,
                i8_field: i8,
                i16_field: i16,
                i32_field: i32,
                i64_field: i64,
            }

            let mut data = BTreeMap::new();
            data.insert("u8_field".to_string(), "255".to_string());
            data.insert("u16_field".to_string(), "65535".to_string());
            data.insert("u32_field".to_string(), "4294967295".to_string());
            data.insert("u64_field".to_string(), "18446744073709551615".to_string());
            data.insert("i8_field".to_string(), "-128".to_string());
            data.insert("i16_field".to_string(), "-32768".to_string());
            data.insert("i32_field".to_string(), "-2147483648".to_string());
            data.insert("i64_field".to_string(), "-9223372036854775808".to_string());

            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            let result: Result<TestForm, _> = TestForm::deserialize(deserializer);

            assert!(result.is_ok());
            let form = result.unwrap();
            assert_eq!(form.u8_field, 255);
            assert_eq!(form.u16_field, 65_535);
            assert_eq!(form.u32_field, 4_294_967_295);
            assert_eq!(form.u64_field, 18_446_744_073_709_551_615);
            assert_eq!(form.i8_field, -128);
            assert_eq!(form.i16_field, -32_768);
            assert_eq!(form.i32_field, -2_147_483_648);
            assert_eq!(form.i64_field, -9_223_372_036_854_775_808);
        }

        #[test]
        fn test_deserialize_booleans() {
            #[derive(Debug, Deserialize, PartialEq)]
            struct TestForm {
                bool1: bool,
                bool2: bool,
            }

            let mut data = BTreeMap::new();
            data.insert("bool1".to_string(), "true".to_string());
            data.insert("bool2".to_string(), "false".to_string());

            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            let result: Result<TestForm, _> = TestForm::deserialize(deserializer);

            assert!(result.is_ok());
            let form = result.unwrap();
            assert!(form.bool1);
            assert!(!form.bool2);
        }

        #[test]
        fn test_deserialize_with_serde_rename() {
            #[derive(Debug, Deserialize, PartialEq)]
            struct TestForm {
                #[serde(rename = "user_age")]
                age: u64,
                #[serde(rename = "user_name")]
                name: String,
            }

            let mut data = BTreeMap::new();
            data.insert("user_age".to_string(), "30".to_string());
            data.insert("user_name".to_string(), "Bob".to_string());

            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            let result: Result<TestForm, _> = TestForm::deserialize(deserializer);

            assert!(result.is_ok());
            let form = result.unwrap();
            assert_eq!(form.age, 30);
            assert_eq!(form.name, "Bob");
        }

        #[test]
        fn test_deserialize_with_default() {
            #[derive(Debug, Deserialize, PartialEq)]
            struct TestForm {
                required: String,
                #[serde(default)]
                optional: String,
            }

            let mut data = BTreeMap::new();
            data.insert("required".to_string(), "value".to_string());

            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            let result: Result<TestForm, _> = TestForm::deserialize(deserializer);

            assert!(result.is_ok());
            let form = result.unwrap();
            assert_eq!(form.required, "value");
            assert_eq!(form.optional, "");
        }

        #[test]
        fn test_invalid_integer_format() {
            #[allow(dead_code)]
            #[derive(Debug, Deserialize)]
            struct TestForm {
                age: u64,
            }

            let mut data = BTreeMap::new();
            data.insert("age".to_string(), "not_a_number".to_string());

            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            let result: Result<TestForm, _> = TestForm::deserialize(deserializer);

            assert!(result.is_err());
        }

        #[test]
        fn test_integer_overflow() {
            #[allow(dead_code)]
            #[derive(Debug, Deserialize)]
            struct TestForm {
                small: u8,
            }

            let mut data = BTreeMap::new();
            data.insert("small".to_string(), "999999".to_string());

            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            let result: Result<TestForm, _> = TestForm::deserialize(deserializer);

            assert!(result.is_err());
        }

        #[test]
        fn test_original_error_case() {
            #[derive(Debug, Deserialize, PartialEq)]
            struct TestForm {
                id: u64,
            }

            let mut data = BTreeMap::new();
            data.insert("id".to_string(), "2445072108".to_string());

            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            let result: Result<TestForm, _> = TestForm::deserialize(deserializer);

            assert!(result.is_ok());
            let form = result.unwrap();
            assert_eq!(form.id, 2_445_072_108);
        }

        #[test]
        fn test_flatten_with_tagged_enum() {
            #[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
            #[serde(tag = "comment_type")]
            enum CommentType {
                General,
                Reply { in_reply_to: u64 },
            }

            #[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
            struct CreateComment {
                body: String,
                #[serde(flatten)]
                comment_type: CommentType,
            }

            let mut data = BTreeMap::new();
            data.insert("body".to_string(), "test comment".to_string());
            data.insert("comment_type".to_string(), "Reply".to_string());
            data.insert("in_reply_to".to_string(), "2445072108".to_string());

            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            let result: Result<CreateComment, _> = CreateComment::deserialize(deserializer);

            assert!(result.is_ok());
            let comment = result.unwrap();
            assert_eq!(comment.body, "test comment");
            assert_eq!(
                comment.comment_type,
                CommentType::Reply {
                    in_reply_to: 2_445_072_108
                }
            );
        }

        #[test]
        fn test_flatten_with_tagged_enum_general_variant() {
            #[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
            #[serde(tag = "comment_type")]
            enum CommentType {
                General,
                Reply { in_reply_to: u64 },
            }

            #[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
            struct CreateComment {
                body: String,
                #[serde(flatten)]
                comment_type: CommentType,
            }

            let mut data = BTreeMap::new();
            data.insert("body".to_string(), "test comment".to_string());
            data.insert("comment_type".to_string(), "General".to_string());

            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            let result: Result<CreateComment, _> = CreateComment::deserialize(deserializer);

            assert!(result.is_ok());
            let comment = result.unwrap();
            assert_eq!(comment.body, "test comment");
            assert_eq!(comment.comment_type, CommentType::General);
        }

        #[test]
        fn test_flatten_with_multiple_integer_fields() {
            #[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
            #[serde(tag = "action_type")]
            enum Action {
                Transfer { from: u64, to: u64, amount: u64 },
            }

            #[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
            struct Request {
                user_id: u64,
                #[serde(flatten)]
                action: Action,
            }

            let mut data = BTreeMap::new();
            data.insert("user_id".to_string(), "100".to_string());
            data.insert("action_type".to_string(), "Transfer".to_string());
            data.insert("from".to_string(), "200".to_string());
            data.insert("to".to_string(), "300".to_string());
            data.insert("amount".to_string(), "1000".to_string());

            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            let result: Result<Request, _> = Request::deserialize(deserializer);

            assert!(result.is_ok());
            let request = result.unwrap();
            assert_eq!(request.user_id, 100);
            assert_eq!(
                request.action,
                Action::Transfer {
                    from: 200,
                    to: 300,
                    amount: 1000
                }
            );
        }

        #[test]
        fn test_deserialize_any_type_inference() {
            use serde::de::Deserialize;

            let bool_true = form_deserializer::StringValueDeserializer::new("true".to_string());
            let result: Result<bool, _> = bool::deserialize(bool_true);
            assert!(result.unwrap());

            let bool_false = form_deserializer::StringValueDeserializer::new("false".to_string());
            let result: Result<bool, _> = bool::deserialize(bool_false);
            assert!(!result.unwrap());

            let number = form_deserializer::StringValueDeserializer::new("42".to_string());
            let result: Result<u64, _> = u64::deserialize(number);
            assert_eq!(result.unwrap(), 42);

            let negative = form_deserializer::StringValueDeserializer::new("-42".to_string());
            let result: Result<i64, _> = i64::deserialize(negative);
            assert_eq!(result.unwrap(), -42);

            let float_val = form_deserializer::StringValueDeserializer::new("2.5".to_string());
            let result: Result<f64, _> = f64::deserialize(float_val);
            assert!((result.unwrap() - 2.5).abs() < f64::EPSILON);

            let string_val = form_deserializer::StringValueDeserializer::new("hello".to_string());
            let result: Result<String, _> = String::deserialize(string_val);
            assert_eq!(result.unwrap(), "hello");
        }

        #[test]
        fn test_flatten_with_mixed_types() {
            #[derive(Debug, Clone, Deserialize, PartialEq)]
            #[serde(tag = "type")]
            enum Metadata {
                Numeric { count: u64, ratio: f64 },
                Text { description: String },
            }

            #[derive(Debug, Clone, Deserialize, PartialEq)]
            struct Item {
                name: String,
                active: bool,
                #[serde(flatten)]
                metadata: Metadata,
            }

            let mut data = BTreeMap::new();
            data.insert("name".to_string(), "Test Item".to_string());
            data.insert("active".to_string(), "true".to_string());
            data.insert("type".to_string(), "Numeric".to_string());
            data.insert("count".to_string(), "42".to_string());
            data.insert("ratio".to_string(), "0.75".to_string());

            let deserializer = form_deserializer::FormDataDeserializer::new(data);
            let result: Result<Item, _> = Item::deserialize(deserializer);

            assert!(result.is_ok());
            let item = result.unwrap();
            assert_eq!(item.name, "Test Item");
            assert!(item.active);
            if let Metadata::Numeric { count, ratio } = item.metadata {
                assert_eq!(count, 42);
                assert!((ratio - 0.75).abs() < f64::EPSILON);
            } else {
                panic!("Expected Numeric variant");
            }
        }
    }
}
