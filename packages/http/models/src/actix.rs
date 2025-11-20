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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_code_to_actix() {
        // Test conversion from our StatusCode to actix StatusCode
        let our_code = crate::StatusCode::Ok;
        let actix_code: StatusCode = our_code.into();
        assert_eq!(actix_code, StatusCode::OK);

        let our_code = crate::StatusCode::NotFound;
        let actix_code: StatusCode = our_code.into();
        assert_eq!(actix_code, StatusCode::NOT_FOUND);

        let our_code = crate::StatusCode::InternalServerError;
        let actix_code: StatusCode = our_code.into();
        assert_eq!(actix_code, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_status_code_from_actix() {
        // Test conversion from actix StatusCode to our StatusCode
        let actix_code = StatusCode::OK;
        let our_code: crate::StatusCode = actix_code.into();
        assert_eq!(our_code, crate::StatusCode::Ok);

        let actix_code = StatusCode::CREATED;
        let our_code: crate::StatusCode = actix_code.into();
        assert_eq!(our_code, crate::StatusCode::Created);

        let actix_code = StatusCode::BAD_REQUEST;
        let our_code: crate::StatusCode = actix_code.into();
        assert_eq!(our_code, crate::StatusCode::BadRequest);
    }

    #[test]
    fn test_status_code_round_trip_actix() {
        // Test that converting to actix and back preserves the value
        let codes = vec![
            crate::StatusCode::Ok,
            crate::StatusCode::Created,
            crate::StatusCode::MovedPermanently,
            crate::StatusCode::NotFound,
            crate::StatusCode::InternalServerError,
        ];

        for code in codes {
            let actix_code: StatusCode = code.into();
            let converted: crate::StatusCode = actix_code.into();
            assert_eq!(code, converted);
        }
    }
}
