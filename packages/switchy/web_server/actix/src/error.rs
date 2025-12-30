//! Error conversion utilities for Actix Web backend.
//!
//! This module provides functions and traits to convert between `switchy_web_server::Error`
//! and `actix_web::Error`. These utilities enable seamless error handling when using the
//! Actix Web backend with the `switchy_web_server` framework.

use actix_web::{Error, error};
use switchy_http_models::{StatusCode, TryFromU16StatusCodeError};

/// Converts a `switchy_web_server::Error` to an `actix_web::Error`.
///
/// This function provides the same functionality as the `From` implementation
/// in the main crate, but as a standalone function to work around orphan rules.
#[must_use]
pub fn into_actix_error(value: switchy_web_server::Error) -> Error {
    match value {
        switchy_web_server::Error::Http {
            status_code,
            source,
        } => match status_code {
            StatusCode::BadRequest => error::ErrorBadRequest(source),
            StatusCode::Unauthorized => error::ErrorUnauthorized(source),
            StatusCode::PaymentRequired => error::ErrorPaymentRequired(source),
            StatusCode::Forbidden => error::ErrorForbidden(source),
            StatusCode::NotFound => error::ErrorNotFound(source),
            StatusCode::MethodNotAllowed => error::ErrorMethodNotAllowed(source),
            StatusCode::NotAcceptable => error::ErrorNotAcceptable(source),
            StatusCode::ProxyAuthenticationRequired => {
                error::ErrorProxyAuthenticationRequired(source)
            }
            StatusCode::RequestTimeout => error::ErrorRequestTimeout(source),
            StatusCode::Conflict => error::ErrorConflict(source),
            StatusCode::Gone => error::ErrorGone(source),
            StatusCode::LengthRequired => error::ErrorLengthRequired(source),
            StatusCode::PreconditionFailed => error::ErrorPreconditionFailed(source),
            StatusCode::ContentTooLarge => error::ErrorPayloadTooLarge(source),
            StatusCode::URITooLong => error::ErrorUriTooLong(source),
            StatusCode::UnsupportedMediaType => error::ErrorUnsupportedMediaType(source),
            StatusCode::RangeNotSatisfiable => error::ErrorRangeNotSatisfiable(source),
            StatusCode::ExpectationFailed => error::ErrorExpectationFailed(source),
            StatusCode::ImATeapot => error::ErrorImATeapot(source),
            StatusCode::MisdirectedRequest => error::ErrorMisdirectedRequest(source),
            StatusCode::UncompressableContent => error::ErrorUnprocessableEntity(source),
            StatusCode::Locked => error::ErrorLocked(source),
            StatusCode::FailedDependency => error::ErrorFailedDependency(source),
            StatusCode::UpgradeRequired => error::ErrorUpgradeRequired(source),
            StatusCode::PreconditionRequired => error::ErrorPreconditionRequired(source),
            StatusCode::TooManyRequests => error::ErrorTooManyRequests(source),
            StatusCode::RequestHeaderFieldsTooLarge => {
                error::ErrorRequestHeaderFieldsTooLarge(source)
            }
            StatusCode::UnavailableForLegalReasons => {
                error::ErrorUnavailableForLegalReasons(source)
            }
            StatusCode::Continue
            | StatusCode::SwitchingProtocols
            | StatusCode::Processing
            | StatusCode::EarlyHints
            | StatusCode::Ok
            | StatusCode::Created
            | StatusCode::Accepted
            | StatusCode::NonAuthoritativeInformation
            | StatusCode::NoContent
            | StatusCode::ResetContent
            | StatusCode::PartialContent
            | StatusCode::MultiStatus
            | StatusCode::AlreadyReported
            | StatusCode::IMUsed
            | StatusCode::MultipleChoices
            | StatusCode::MovedPermanently
            | StatusCode::Found
            | StatusCode::SeeOther
            | StatusCode::NotModified
            | StatusCode::UseProxy
            | StatusCode::TemporaryRedirect
            | StatusCode::PermanentRedirect
            | StatusCode::TooEarly
            | StatusCode::InternalServerError => error::ErrorInternalServerError(source),
            StatusCode::NotImplemented => error::ErrorNotImplemented(source),
            StatusCode::BadGateway => error::ErrorBadGateway(source),
            StatusCode::ServiceUnavailable => error::ErrorServiceUnavailable(source),
            StatusCode::GatewayTimeout => error::ErrorGatewayTimeout(source),
            StatusCode::HTTPVersionNotSupported => error::ErrorHttpVersionNotSupported(source),
            StatusCode::VariantAlsoNegotiates => error::ErrorVariantAlsoNegotiates(source),
            StatusCode::InsufficientStorage => error::ErrorInsufficientStorage(source),
            StatusCode::LoopDetected => error::ErrorLoopDetected(source),
            StatusCode::NotExtended => error::ErrorNotExtended(source),
            StatusCode::NetworkAuthenticationRequired => {
                error::ErrorNetworkAuthenticationRequired(source)
            }
        },
    }
}

/// Attempts to convert an `actix_web::Error` to a `switchy_web_server::Error`.
///
/// # Errors
///
/// Returns `TryFromU16StatusCodeError` if the status code conversion fails.
pub fn try_from_actix_error(
    value: &Error,
) -> Result<switchy_web_server::Error, TryFromU16StatusCodeError> {
    let status_code = StatusCode::try_from_u16(value.error_response().status().as_u16())?;
    let error_message = format!("Actix error: {value}");
    Ok(switchy_web_server::Error::from_http_status_code(
        status_code,
        std::io::Error::other(error_message),
    ))
}

/// Extension trait for converting `switchy_web_server::Error` to `actix_web::Error`.
pub trait IntoActixError {
    /// Converts this error into an Actix web error.
    ///
    /// This method maps the HTTP status code to the appropriate Actix error
    /// constructor, preserving the original error message.
    fn into_actix_error(self) -> Error;
}

impl IntoActixError for switchy_web_server::Error {
    fn into_actix_error(self) -> Error {
        into_actix_error(self)
    }
}

/// Extension trait for converting `actix_web::Error` to `switchy_web_server::Error`.
pub trait TryIntoWebServerError {
    /// Attempts to convert this Actix error to a `switchy_web_server::Error`.
    ///
    /// This method extracts the HTTP status code from the Actix error response
    /// and constructs a framework-agnostic error.
    ///
    /// # Errors
    ///
    /// Returns `TryFromU16StatusCodeError` if the HTTP status code cannot be
    /// converted to a known `StatusCode` variant.
    fn try_into_web_server_error(
        self,
    ) -> Result<switchy_web_server::Error, TryFromU16StatusCodeError>;
}

impl TryIntoWebServerError for Error {
    fn try_into_web_server_error(
        self,
    ) -> Result<switchy_web_server::Error, TryFromU16StatusCodeError> {
        try_from_actix_error(&self)
    }
}
