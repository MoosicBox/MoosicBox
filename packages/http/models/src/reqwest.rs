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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_method_to_reqwest() {
        // Test conversion from our Method to reqwest Method
        assert_eq!(reqwest::Method::from(Method::Get), reqwest::Method::GET);
        assert_eq!(reqwest::Method::from(Method::Post), reqwest::Method::POST);
        assert_eq!(reqwest::Method::from(Method::Put), reqwest::Method::PUT);
        assert_eq!(reqwest::Method::from(Method::Patch), reqwest::Method::PATCH);
        assert_eq!(
            reqwest::Method::from(Method::Delete),
            reqwest::Method::DELETE
        );
        assert_eq!(reqwest::Method::from(Method::Head), reqwest::Method::HEAD);
        assert_eq!(
            reqwest::Method::from(Method::Options),
            reqwest::Method::OPTIONS
        );
        assert_eq!(
            reqwest::Method::from(Method::Connect),
            reqwest::Method::CONNECT
        );
        assert_eq!(reqwest::Method::from(Method::Trace), reqwest::Method::TRACE);
    }

    #[test]
    fn test_status_code_from_reqwest() {
        // Test conversion from reqwest StatusCode to our StatusCode
        let reqwest_code = reqwest::StatusCode::OK;
        let our_code: StatusCode = reqwest_code.into();
        assert_eq!(our_code, StatusCode::Ok);

        let reqwest_code = reqwest::StatusCode::NOT_FOUND;
        let our_code: StatusCode = reqwest_code.into();
        assert_eq!(our_code, StatusCode::NotFound);

        let reqwest_code = reqwest::StatusCode::INTERNAL_SERVER_ERROR;
        let our_code: StatusCode = reqwest_code.into();
        assert_eq!(our_code, StatusCode::InternalServerError);
    }

    #[test]
    fn test_status_code_reqwest_conversion() {
        // Test various status code conversions
        let codes = vec![
            (reqwest::StatusCode::CONTINUE, StatusCode::Continue),
            (
                reqwest::StatusCode::SWITCHING_PROTOCOLS,
                StatusCode::SwitchingProtocols,
            ),
            (reqwest::StatusCode::OK, StatusCode::Ok),
            (reqwest::StatusCode::CREATED, StatusCode::Created),
            (reqwest::StatusCode::FOUND, StatusCode::Found),
            (reqwest::StatusCode::BAD_REQUEST, StatusCode::BadRequest),
            (reqwest::StatusCode::UNAUTHORIZED, StatusCode::Unauthorized),
            (reqwest::StatusCode::BAD_GATEWAY, StatusCode::BadGateway),
        ];

        for (reqwest_code, expected) in codes {
            let converted: StatusCode = reqwest_code.into();
            assert_eq!(converted, expected);
        }
    }
}
