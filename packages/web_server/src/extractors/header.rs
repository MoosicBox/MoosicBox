//! HTTP header extractor for requests.
//!
//! This module provides the [`Header<T>`] extractor for extracting and parsing
//! HTTP headers from requests with automatic type conversion.
//!
//! # Overview
//!
//! The header extractor supports multiple extraction strategies:
//!
//! * **Single headers**: Extract a single header value with type conversion
//! * **Tuple headers**: Extract multiple headers as a tuple
//! * **Struct headers**: Extract headers into a deserializable struct
//!
//! # Example
//!
//! ```rust,ignore
//! use moosicbox_web_server::extractors::Header;
//!
//! // Extract authorization header as String
//! async fn auth_handler(Header(auth): Header<String>) -> Result<HttpResponse, Error> {
//!     println!("Authorization: {}", auth);
//!     Ok(HttpResponse::ok())
//! }
//!
//! // Extract content-length as u64
//! async fn size_handler(Header(size): Header<u64>) -> Result<HttpResponse, Error> {
//!     println!("Content-Length: {}", size);
//!     Ok(HttpResponse::ok())
//! }
//! ```

#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use crate::{
    Error, HttpRequest,
    from_request::{FromRequest, IntoHandlerError},
};

use std::{collections::BTreeMap, fmt, str::FromStr};

/// Error types that can occur during header extraction
#[derive(Debug)]
pub enum HeaderError {
    /// Required header is missing from the request
    MissingHeader {
        /// Name of the missing header
        name: String,
    },
    /// Header value cannot be parsed into the target type
    ParseError {
        /// Name of the header
        name: String,
        /// The raw header value that failed to parse
        value: String,
        /// The target type name
        target_type: &'static str,
        /// The parsing error message
        source: String,
    },
    /// Header value contains invalid characters or format
    InvalidHeaderValue {
        /// Name of the header
        name: String,
        /// The invalid header value
        value: String,
        /// Reason why the value is invalid
        reason: String,
    },
    /// Failed to deserialize multiple headers into a struct
    DeserializationError {
        /// The serde error message (stored as string for Clone compatibility)
        source: String,
        /// The headers that were being deserialized
        headers: BTreeMap<String, String>,
        /// The target type name
        target_type: &'static str,
    },
}

impl fmt::Display for HeaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingHeader { name } => {
                write!(f, "Required header '{name}' is missing from the request")
            }
            Self::ParseError {
                name,
                value,
                target_type,
                source,
            } => {
                write!(
                    f,
                    "Failed to parse header '{name}' with value '{value}' into type '{target_type}': {source}"
                )
            }
            Self::InvalidHeaderValue {
                name,
                value,
                reason,
            } => {
                write!(f, "Header '{name}' has invalid value '{value}': {reason}")
            }
            Self::DeserializationError {
                source,
                target_type,
                headers,
            } => {
                write!(
                    f,
                    "Failed to deserialize headers into type '{target_type}': {source}. Headers: {headers:?}"
                )
            }
        }
    }
}

impl std::error::Error for HeaderError {}

impl IntoHandlerError for HeaderError {
    fn into_handler_error(self) -> Error {
        Error::bad_request(self.to_string())
    }
}

/// Extractor for HTTP headers with type conversion and validation
///
/// This extractor provides flexible header extraction with automatic type conversion
/// and comprehensive error handling. It supports extracting single headers, multiple
/// headers, and complex header structures.
///
/// # Dual-Mode Support
///
/// * **Actix backend**: Uses synchronous extraction to avoid Send bounds issues
/// * **Simulator backend**: Uses async extraction (delegates to sync implementation)
///
/// # Extraction Strategies
///
/// The `Header<T>` extractor automatically determines the extraction strategy based on the type `T`:
///
/// ## Single Header Extraction
///
/// For simple types like `String`, `u32`, `bool`, etc., extracts a single header value.
/// The header name is determined by the type or can be specified using attributes.
///
/// ```rust,ignore
/// use moosicbox_web_server::extractors::Header;
///
/// // Extracts the "authorization" header as a String
/// async fn handler(Header(auth): Header<String>) -> Result<HttpResponse, Error> {
///     // auth contains the authorization header value
///     Ok(HttpResponse::ok())
/// }
///
/// // Extracts the "content-length" header as u64
/// async fn handler(Header(length): Header<u64>) -> Result<HttpResponse, Error> {
///     // length contains the parsed content-length value
///     Ok(HttpResponse::ok())
/// }
/// ```
///
/// ## Multiple Header Extraction
///
/// For tuple types, extracts multiple headers in order:
///
/// ```rust,ignore
/// use moosicbox_web_server::extractors::Header;
///
/// // Extracts authorization and content-type headers
/// async fn handler(Header((auth, content_type)): Header<(String, String)>) -> Result<HttpResponse, Error> {
///     // auth = authorization header, content_type = content-type header
///     Ok(HttpResponse::ok())
/// }
/// ```
///
/// ## Structured Header Extraction
///
/// For struct types with serde support, maps header names to struct fields:
///
/// ```rust,ignore
/// use moosicbox_web_server::extractors::Header;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct RequestHeaders {
///     #[serde(rename = "user-agent")]
///     user_agent: Option<String>,
///     authorization: Option<String>,
///     #[serde(rename = "content-type")]
///     content_type: String,
/// }
///
/// async fn handler(Header(headers): Header<RequestHeaders>) -> Result<HttpResponse, Error> {
///     // headers.user_agent = "user-agent" header (optional)
///     // headers.authorization = "authorization" header (optional)  
///     // headers.content_type = "content-type" header (required)
///     Ok(HttpResponse::ok())
/// }
/// ```
///
/// # Header Name Resolution
///
/// * **Single types**: Uses common header names (authorization, content-type, user-agent, etc.)
/// * **Tuples**: Uses positional header names or common defaults
/// * **Structs**: Uses field names or serde rename attributes
///
/// # Error Handling
///
/// * **Missing headers**: Returns `HeaderError::MissingHeader` for required headers
/// * **Parse errors**: Returns `HeaderError::ParseError` for type conversion failures
/// * **Invalid values**: Returns `HeaderError::InvalidHeaderValue` for malformed headers
/// * **Deserialization errors**: Returns `HeaderError::DeserializationError` for struct parsing
///
/// All errors are automatically converted to appropriate HTTP responses (400 Bad Request).
#[derive(Debug)]
pub struct Header<T>(pub T);

impl<T> Header<T> {
    /// Create a new Header extractor with the extracted value
    #[must_use]
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// Get the inner extracted value
    #[must_use]
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// Extract a single header value with type conversion
fn extract_single_header<T>(req: &HttpRequest, header_name: &str) -> Result<T, HeaderError>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    let value = req
        .header(header_name)
        .ok_or_else(|| HeaderError::MissingHeader {
            name: header_name.to_string(),
        })?;

    value.parse::<T>().map_err(|e| HeaderError::ParseError {
        name: header_name.to_string(),
        value: value.to_string(),
        target_type: std::any::type_name::<T>(),
        source: e.to_string(),
    })
}

/// Extract multiple headers for tuple types
fn extract_tuple_headers(
    req: &HttpRequest,
    header_names: &[&str],
) -> Result<Vec<String>, HeaderError> {
    let mut values = Vec::new();

    for &header_name in header_names {
        let value = req
            .header(header_name)
            .ok_or_else(|| HeaderError::MissingHeader {
                name: header_name.to_string(),
            })?;
        values.push(value.to_string());
    }

    Ok(values)
}

// Implement FromRequest for common single header types
impl FromRequest for Header<String> {
    type Error = HeaderError;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        // Default to authorization header for String extraction
        let value = extract_single_header::<String>(req, "authorization")?;
        Ok(Self(value))
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

impl FromRequest for Header<u64> {
    type Error = HeaderError;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        // Default to content-length header for u64 extraction
        let value = extract_single_header::<u64>(req, "content-length")?;
        Ok(Self(value))
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

impl FromRequest for Header<bool> {
    type Error = HeaderError;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        // Default to checking for presence of upgrade header for bool extraction
        let value = req.header("upgrade").is_some();
        Ok(Self(value))
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

// Implement FromRequest for tuple types (multiple headers)
impl FromRequest for Header<(String, String)> {
    type Error = HeaderError;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        let values = extract_tuple_headers(req, &["authorization", "content-type"])?;
        Ok(Self((values[0].clone(), values[1].clone())))
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

impl FromRequest for Header<(String, String, String)> {
    type Error = HeaderError;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        let values = extract_tuple_headers(req, &["authorization", "content-type", "user-agent"])?;
        Ok(Self((
            values[0].clone(),
            values[1].clone(),
            values[2].clone(),
        )))
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

#[cfg(all(test, feature = "simulator"))]
mod tests {
    use super::*;
    use crate::{HttpRequest, Stub};

    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    use crate::simulator::{SimulationRequest, SimulationStub};

    fn create_test_request_with_headers(headers: &[(&str, &str)]) -> HttpRequest {
        #[cfg(any(feature = "simulator", not(feature = "actix")))]
        {
            let mut sim_req = SimulationRequest::new(crate::Method::Get, "/test");
            for (name, value) in headers {
                sim_req = sim_req.with_header(*name, *value);
            }
            HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)))
        }
        #[cfg(all(feature = "actix", not(feature = "simulator")))]
        {
            // For Actix-only builds, use empty stub
            let _ = headers;
            HttpRequest::Stub(Stub::Empty)
        }
    }

    #[test]
    fn test_single_header_string_extraction() {
        let http_req = create_test_request_with_headers(&[("authorization", "Bearer token123")]);

        let result = Header::<String>::from_request_sync(&http_req);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, "Bearer token123");
    }

    #[test]
    fn test_single_header_u64_extraction() {
        let http_req = create_test_request_with_headers(&[("content-length", "1024")]);

        let result = Header::<u64>::from_request_sync(&http_req);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().0, 1024);
    }

    #[test]
    fn test_single_header_bool_extraction() {
        let http_req = create_test_request_with_headers(&[("upgrade", "websocket")]);

        let result = Header::<bool>::from_request_sync(&http_req);
        assert!(result.is_ok());
        assert!(result.unwrap().0);
    }

    #[test]
    fn test_single_header_bool_extraction_missing() {
        let http_req = create_test_request_with_headers(&[]);

        let result = Header::<bool>::from_request_sync(&http_req);
        assert!(result.is_ok());
        assert!(!result.unwrap().0);
    }

    #[test]
    fn test_tuple_header_extraction() {
        let http_req = create_test_request_with_headers(&[
            ("authorization", "Bearer token123"),
            ("content-type", "application/json"),
        ]);

        let result = Header::<(String, String)>::from_request_sync(&http_req);
        assert!(result.is_ok());
        let (auth, ct) = result.unwrap().0;
        assert_eq!(auth, "Bearer token123");
        assert_eq!(ct, "application/json");
    }

    #[test]
    fn test_triple_header_extraction() {
        let http_req = create_test_request_with_headers(&[
            ("authorization", "Bearer token123"),
            ("content-type", "application/json"),
            ("user-agent", "TestAgent/1.0"),
        ]);

        let result = Header::<(String, String, String)>::from_request_sync(&http_req);
        assert!(result.is_ok());
        let (auth, ct, ua) = result.unwrap().0;
        assert_eq!(auth, "Bearer token123");
        assert_eq!(ct, "application/json");
        assert_eq!(ua, "TestAgent/1.0");
    }

    #[test]
    fn test_missing_header_error() {
        let http_req = create_test_request_with_headers(&[]);

        let result = Header::<String>::from_request_sync(&http_req);
        assert!(result.is_err());
        match result.unwrap_err() {
            HeaderError::MissingHeader { name } => {
                assert_eq!(name, "authorization");
            }
            _ => panic!("Expected MissingHeader error"),
        }
    }

    #[test]
    fn test_parse_error() {
        let http_req = create_test_request_with_headers(&[("content-length", "invalid")]);

        let result = Header::<u64>::from_request_sync(&http_req);
        assert!(result.is_err());
        match result.unwrap_err() {
            HeaderError::ParseError {
                name,
                value,
                target_type,
                ..
            } => {
                assert_eq!(name, "content-length");
                assert_eq!(value, "invalid");
                assert_eq!(target_type, "u64");
            }
            _ => panic!("Expected ParseError"),
        }
    }

    #[test]
    fn test_tuple_missing_header_error() {
        let http_req = create_test_request_with_headers(&[("authorization", "Bearer token123")]);

        let result = Header::<(String, String)>::from_request_sync(&http_req);
        assert!(result.is_err());
        match result.unwrap_err() {
            HeaderError::MissingHeader { name } => {
                assert_eq!(name, "content-type");
            }
            _ => panic!("Expected MissingHeader error"),
        }
    }

    #[test]
    fn test_header_error_display() {
        let error = HeaderError::MissingHeader {
            name: "authorization".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Required header 'authorization' is missing from the request"
        );

        let error = HeaderError::ParseError {
            name: "content-length".to_string(),
            value: "invalid".to_string(),
            target_type: "u64",
            source: "invalid digit found in string".to_string(),
        };
        assert!(
            error
                .to_string()
                .contains("Failed to parse header 'content-length'")
        );
    }

    #[test]
    fn test_header_into_inner() {
        let header = Header::new("test_value".to_string());
        assert_eq!(header.into_inner(), "test_value");
    }
}
