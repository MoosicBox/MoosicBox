//! Conversions to and from `actix-web` HTTP types.
//!
//! This module provides `From` implementations to convert between this crate's
//! [`StatusCode`](crate::StatusCode) and `actix-web`'s status code types.

use actix_web::http::StatusCode;

/// Converts this crate's `StatusCode` into `actix-web`'s `StatusCode`.
///
/// # Panics
///
/// Panics if the status code cannot be converted (this should never happen for valid status codes).
#[allow(clippy::fallible_impl_from)]
impl From<crate::StatusCode> for StatusCode {
    fn from(value: crate::StatusCode) -> Self {
        Self::from_u16(value.into()).unwrap()
    }
}

/// Converts `actix-web`'s `StatusCode` into this crate's `StatusCode`.
///
/// # Panics
///
/// Panics if the status code value is not recognized (this should never happen for
/// valid actix-web status codes).
impl From<StatusCode> for crate::StatusCode {
    fn from(value: StatusCode) -> Self {
        Self::from_u16(value.into())
    }
}
