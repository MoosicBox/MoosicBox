//! Conversions to and from `reqwest` HTTP types.
//!
//! This module provides `From` implementations to convert between this crate's
//! [`Method`] and [`StatusCode`] types and their `reqwest` equivalents.

use crate::{Method, StatusCode};

/// Converts this crate's `Method` into `reqwest`'s `Method`.
impl From<Method> for reqwest::Method {
    fn from(value: Method) -> Self {
        match value {
            Method::Get => Self::GET,
            Method::Post => Self::POST,
            Method::Put => Self::PUT,
            Method::Patch => Self::PATCH,
            Method::Delete => Self::DELETE,
            Method::Head => Self::HEAD,
            Method::Options => Self::OPTIONS,
            Method::Connect => Self::CONNECT,
            Method::Trace => Self::TRACE,
        }
    }
}

/// Converts `reqwest`'s `StatusCode` into this crate's `StatusCode`.
///
/// # Panics
///
/// Panics if the status code value is not recognized (this should never happen for
/// valid reqwest status codes).
#[allow(clippy::fallible_impl_from)]
impl From<reqwest::StatusCode> for StatusCode {
    fn from(value: reqwest::StatusCode) -> Self {
        Self::from_u16(value.as_u16())
    }
}
