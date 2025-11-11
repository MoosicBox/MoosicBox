//! HTTP models and types for Switchy.
//!
//! This crate provides common HTTP types including methods and status codes that work
//! across different HTTP libraries. It includes optional conversions for popular frameworks
//! like `actix-web` and `reqwest`.
//!
//! # Features
//!
//! * `actix` - Enables conversions to/from `actix-web` types
//! * `reqwest` - Enables conversions to/from `reqwest` types
//! * `serde` - Enables serialization/deserialization support
//!
//! # Example
//!
//! ```rust
//! use switchy_http_models::{Method, StatusCode};
//!
//! let method = Method::Get;
//! assert_eq!(method.to_string(), "GET");
//!
//! let status = StatusCode::Ok;
//! assert_eq!(status.as_u16(), 200);
//! assert!(status.is_success());
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "actix")]
pub mod actix;
#[cfg(feature = "reqwest")]
pub mod reqwest;

use std::str::FromStr;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use strum::{AsRefStr, EnumString};

/// HTTP request method.
///
/// Represents standard HTTP methods as defined in RFC 7231 and RFC 5789.
#[derive(Debug, Clone, Copy, AsRefStr, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "SCREAMING_SNAKE_CASE"))]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Method {
    /// GET method - requests a representation of the specified resource.
    Get,
    /// POST method - submits data to be processed to a specified resource.
    Post,
    /// PUT method - replaces all current representations of the target resource.
    Put,
    /// PATCH method - applies partial modifications to a resource.
    Patch,
    /// DELETE method - deletes the specified resource.
    Delete,
    /// HEAD method - identical to GET but without the response body.
    Head,
    /// OPTIONS method - describes the communication options for the target resource.
    Options,
    /// CONNECT method - establishes a tunnel to the server identified by the target resource.
    Connect,
    /// TRACE method - performs a message loop-back test along the path to the target resource.
    Trace,
}

/// Error returned when parsing an invalid HTTP method string.
#[derive(Debug, thiserror::Error)]
pub struct InvalidMethod;

impl std::fmt::Display for InvalidMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Invalid HTTP method")
    }
}

impl FromStr for Method {
    type Err = InvalidMethod;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "GET" | "Get" | "get" => Self::Get,
            "POST" | "Post" | "post" => Self::Post,
            "PUT" | "Put" | "put" => Self::Put,
            "PATCH" | "Patch" | "patch" => Self::Patch,
            "DELETE" | "Delete" | "delete" => Self::Delete,
            "HEAD" | "Head" | "head" => Self::Head,
            "OPTIONS" | "Options" | "options" => Self::Options,
            "CONNECT" | "Connect" | "connect" => Self::Connect,
            "TRACE" | "Trace" | "trace" => Self::Trace,
            _ => return Err(InvalidMethod),
        })
    }
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

/// HTTP status code.
///
/// Represents standard HTTP status codes as defined in various RFCs.
///
/// See: <https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Status>
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, EnumString, AsRefStr)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "SCREAMING_SNAKE_CASE"))]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum StatusCode {
    /// 100 Continue - initial part of request received, client should continue.
    Continue,
    /// 101 Switching Protocols - server is switching protocols as requested.
    SwitchingProtocols,
    /// 102 Processing - request is being processed but no response is available yet.
    Processing,
    /// 103 Early Hints - hints to help client start preloading resources.
    EarlyHints,
    /// 200 OK - request succeeded.
    Ok,
    /// 201 Created - request succeeded and a new resource was created.
    Created,
    /// 202 Accepted - request accepted for processing but not yet completed.
    Accepted,
    /// 203 Non-Authoritative Information - transformed version of 200 OK from transforming proxy.
    NonAuthoritativeInformation,
    /// 204 No Content - request succeeded but no content to return.
    NoContent,
    /// 205 Reset Content - request succeeded, client should reset document view.
    ResetContent,
    /// 206 Partial Content - partial resource returned due to Range header.
    PartialContent,
    /// 207 Multi-Status - multiple status codes for multiple operations.
    MultiStatus,
    /// 208 Already Reported - members already enumerated in previous response.
    AlreadyReported,
    /// 226 IM Used - instance manipulations have been applied.
    IMUsed,
    /// 300 Multiple Choices - multiple possible responses, client should choose one.
    MultipleChoices,
    /// 301 Moved Permanently - resource has been permanently moved to new URL.
    MovedPermanently,
    /// 302 Found - resource temporarily located at different URI.
    Found,
    /// 303 See Other - response can be found at different URI using GET.
    SeeOther,
    /// 304 Not Modified - resource has not been modified since last request.
    NotModified,
    /// 305 Use Proxy - resource must be accessed through specified proxy.
    UseProxy,
    /// 307 Temporary Redirect - resource temporarily at different URI, method unchanged.
    TemporaryRedirect,
    /// 308 Permanent Redirect - resource permanently at different URI, method unchanged.
    PermanentRedirect,
    /// 400 Bad Request - server cannot process request due to client error.
    BadRequest,
    /// 401 Unauthorized - authentication is required and has failed or not been provided.
    Unauthorized,
    /// 402 Payment Required - reserved for future use.
    PaymentRequired,
    /// 403 Forbidden - client does not have access rights to the content.
    Forbidden,
    /// 404 Not Found - server cannot find requested resource.
    NotFound,
    /// 405 Method Not Allowed - request method not supported for this resource.
    MethodNotAllowed,
    /// 406 Not Acceptable - resource not available in format acceptable to client.
    NotAcceptable,
    /// 407 Proxy Authentication Required - client must authenticate with proxy.
    ProxyAuthenticationRequired,
    /// 408 Request Timeout - server timed out waiting for request.
    RequestTimeout,
    /// 409 Conflict - request conflicts with current state of server.
    Conflict,
    /// 410 Gone - requested resource is no longer available and will not be available again.
    Gone,
    /// 411 Length Required - Content-Length header is required.
    LengthRequired,
    /// 412 Precondition Failed - precondition in request headers evaluated to false.
    PreconditionFailed,
    /// 413 Content Too Large - request entity is larger than server is willing to process.
    ContentTooLarge,
    /// 414 URI Too Long - URI is longer than server is willing to interpret.
    URITooLong,
    /// 415 Unsupported Media Type - media type of request data is not supported.
    UnsupportedMediaType,
    /// 416 Range Not Satisfiable - range specified in Range header cannot be fulfilled.
    RangeNotSatisfiable,
    /// 417 Expectation Failed - expectation in Expect header cannot be met.
    ExpectationFailed,
    /// 418 I'm a teapot - server refuses to brew coffee because it is a teapot.
    ImATeapot,
    /// 421 Misdirected Request - request was directed at server unable to produce response.
    MisdirectedRequest,
    /// 422 Unprocessable Content - request well-formed but unable to be processed.
    UncompressableContent,
    /// 423 Locked - resource being accessed is locked.
    Locked,
    /// 424 Failed Dependency - request failed due to failure of previous request.
    FailedDependency,
    /// 425 Too Early - server unwilling to risk processing request that might be replayed.
    TooEarly,
    /// 426 Upgrade Required - client should switch to different protocol.
    UpgradeRequired,
    /// 428 Precondition Required - origin server requires request to be conditional.
    PreconditionRequired,
    /// 429 Too Many Requests - client has sent too many requests in given time.
    TooManyRequests,
    /// 431 Request Header Fields Too Large - request header fields are too large.
    RequestHeaderFieldsTooLarge,
    /// 451 Unavailable For Legal Reasons - resource unavailable for legal reasons.
    UnavailableForLegalReasons,
    /// 500 Internal Server Error - server encountered unexpected condition.
    InternalServerError,
    /// 501 Not Implemented - server does not support functionality required to fulfill request.
    NotImplemented,
    /// 502 Bad Gateway - server received invalid response from upstream server.
    BadGateway,
    /// 503 Service Unavailable - server is not ready to handle request.
    ServiceUnavailable,
    /// 504 Gateway Timeout - server did not receive timely response from upstream server.
    GatewayTimeout,
    /// 505 HTTP Version Not Supported - HTTP version not supported by server.
    HTTPVersionNotSupported,
    /// 506 Variant Also Negotiates - server has internal configuration error.
    VariantAlsoNegotiates,
    /// 507 Insufficient Storage - server unable to store representation needed to complete request.
    InsufficientStorage,
    /// 508 Loop Detected - server detected infinite loop while processing request.
    LoopDetected,
    /// 510 Not Extended - further extensions to request are required.
    NotExtended,
    /// 511 Network Authentication Required - client needs to authenticate to gain network access.
    NetworkAuthenticationRequired,
}

impl From<StatusCode> for u16 {
    fn from(value: StatusCode) -> Self {
        match value {
            StatusCode::Continue => 100,
            StatusCode::SwitchingProtocols => 101,
            StatusCode::Processing => 102,
            StatusCode::EarlyHints => 103,
            StatusCode::Ok => 200,
            StatusCode::Created => 201,
            StatusCode::Accepted => 202,
            StatusCode::NonAuthoritativeInformation => 203,
            StatusCode::NoContent => 204,
            StatusCode::ResetContent => 205,
            StatusCode::PartialContent => 206,
            StatusCode::MultiStatus => 207,
            StatusCode::AlreadyReported => 208,
            StatusCode::IMUsed => 226,
            StatusCode::MultipleChoices => 300,
            StatusCode::MovedPermanently => 301,
            StatusCode::Found => 302,
            StatusCode::SeeOther => 303,
            StatusCode::NotModified => 304,
            StatusCode::UseProxy => 305,
            StatusCode::TemporaryRedirect => 307,
            StatusCode::PermanentRedirect => 308,
            StatusCode::BadRequest => 400,
            StatusCode::Unauthorized => 401,
            StatusCode::PaymentRequired => 402,
            StatusCode::Forbidden => 403,
            StatusCode::NotFound => 404,
            StatusCode::MethodNotAllowed => 405,
            StatusCode::NotAcceptable => 406,
            StatusCode::ProxyAuthenticationRequired => 407,
            StatusCode::RequestTimeout => 408,
            StatusCode::Conflict => 409,
            StatusCode::Gone => 410,
            StatusCode::LengthRequired => 411,
            StatusCode::PreconditionFailed => 412,
            StatusCode::ContentTooLarge => 413,
            StatusCode::URITooLong => 414,
            StatusCode::UnsupportedMediaType => 415,
            StatusCode::RangeNotSatisfiable => 416,
            StatusCode::ExpectationFailed => 417,
            StatusCode::ImATeapot => 418,
            StatusCode::MisdirectedRequest => 421,
            StatusCode::UncompressableContent => 422,
            StatusCode::Locked => 423,
            StatusCode::FailedDependency => 424,
            StatusCode::TooEarly => 425,
            StatusCode::UpgradeRequired => 426,
            StatusCode::PreconditionRequired => 428,
            StatusCode::TooManyRequests => 429,
            StatusCode::RequestHeaderFieldsTooLarge => 431,
            StatusCode::UnavailableForLegalReasons => 451,
            StatusCode::InternalServerError => 500,
            StatusCode::NotImplemented => 501,
            StatusCode::BadGateway => 502,
            StatusCode::ServiceUnavailable => 503,
            StatusCode::GatewayTimeout => 504,
            StatusCode::HTTPVersionNotSupported => 505,
            StatusCode::VariantAlsoNegotiates => 506,
            StatusCode::InsufficientStorage => 507,
            StatusCode::LoopDetected => 508,
            StatusCode::NotExtended => 510,
            StatusCode::NetworkAuthenticationRequired => 511,
        }
    }
}

/// Error returned when converting an invalid u16 to a status code.
#[derive(Debug, thiserror::Error)]
#[error("TryFromU16StatusCodeError")]
pub struct TryFromU16StatusCodeError;

impl TryFrom<u16> for StatusCode {
    type Error = TryFromU16StatusCodeError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Ok(match value {
            100 => Self::Continue,
            101 => Self::SwitchingProtocols,
            102 => Self::Processing,
            103 => Self::EarlyHints,
            200 => Self::Ok,
            201 => Self::Created,
            202 => Self::Accepted,
            203 => Self::NonAuthoritativeInformation,
            204 => Self::NoContent,
            205 => Self::ResetContent,
            206 => Self::PartialContent,
            207 => Self::MultiStatus,
            208 => Self::AlreadyReported,
            226 => Self::IMUsed,
            300 => Self::MultipleChoices,
            301 => Self::MovedPermanently,
            302 => Self::Found,
            303 => Self::SeeOther,
            304 => Self::NotModified,
            305 => Self::UseProxy,
            307 => Self::TemporaryRedirect,
            308 => Self::PermanentRedirect,
            400 => Self::BadRequest,
            401 => Self::Unauthorized,
            402 => Self::PaymentRequired,
            403 => Self::Forbidden,
            404 => Self::NotFound,
            405 => Self::MethodNotAllowed,
            406 => Self::NotAcceptable,
            407 => Self::ProxyAuthenticationRequired,
            408 => Self::RequestTimeout,
            409 => Self::Conflict,
            410 => Self::Gone,
            411 => Self::LengthRequired,
            412 => Self::PreconditionFailed,
            413 => Self::ContentTooLarge,
            414 => Self::URITooLong,
            415 => Self::UnsupportedMediaType,
            416 => Self::RangeNotSatisfiable,
            417 => Self::ExpectationFailed,
            418 => Self::ImATeapot,
            421 => Self::MisdirectedRequest,
            422 => Self::UncompressableContent,
            423 => Self::Locked,
            424 => Self::FailedDependency,
            425 => Self::TooEarly,
            426 => Self::UpgradeRequired,
            428 => Self::PreconditionRequired,
            429 => Self::TooManyRequests,
            431 => Self::RequestHeaderFieldsTooLarge,
            451 => Self::UnavailableForLegalReasons,
            500 => Self::InternalServerError,
            501 => Self::NotImplemented,
            502 => Self::BadGateway,
            503 => Self::ServiceUnavailable,
            504 => Self::GatewayTimeout,
            505 => Self::HTTPVersionNotSupported,
            506 => Self::VariantAlsoNegotiates,
            507 => Self::InsufficientStorage,
            508 => Self::LoopDetected,
            510 => Self::NotExtended,
            511 => Self::NetworkAuthenticationRequired,
            _ => {
                return Err(TryFromU16StatusCodeError);
            }
        })
    }
}

impl StatusCode {
    /// Converts the status code to its numeric u16 representation.
    #[must_use]
    pub fn into_u16(self) -> u16 {
        self.into()
    }

    /// Returns the numeric u16 representation of the status code.
    #[must_use]
    pub fn as_u16(&self) -> u16 {
        (*self).into_u16()
    }

    /// Attempts to create a `StatusCode` from a `u16` value.
    ///
    /// # Errors
    ///
    /// Returns `TryFromU16StatusCodeError` if:
    /// * The `u16` value does not correspond to a valid HTTP status code
    pub fn try_from_u16(code: u16) -> Result<Self, TryFromU16StatusCodeError> {
        code.try_into()
    }

    /// Creates a `StatusCode` from a `u16` value.
    ///
    /// # Panics
    ///
    /// Panics if the `u16` value does not correspond to a valid HTTP status code.
    #[must_use]
    pub fn from_u16(code: u16) -> Self {
        Self::try_from_u16(code).unwrap()
    }
}

impl StatusCode {
    /// Check if status is within 100-199.
    #[inline]
    #[must_use]
    pub fn is_informational(&self) -> bool {
        (100..200).contains(&self.as_u16())
    }

    /// Check if status is within 200-299.
    #[inline]
    #[must_use]
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.as_u16())
    }

    /// Check if status is within 300-399.
    #[inline]
    #[must_use]
    pub fn is_redirection(&self) -> bool {
        (300..400).contains(&self.as_u16())
    }

    /// Check if status is within 400-499.
    #[inline]
    #[must_use]
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.as_u16())
    }

    /// Check if status is within 500-599.
    #[inline]
    #[must_use]
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.as_u16())
    }
}

impl std::fmt::Display for StatusCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}
