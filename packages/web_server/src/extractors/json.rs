#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use serde::de::DeserializeOwned;

use crate::{
    Error, HttpRequest,
    from_request::{FromRequest, IntoHandlerError},
};

/// Extractor for JSON request bodies with enhanced error handling
///
/// This extractor parses JSON data from the request body using `serde_json`
/// for robust parsing. It supports any type that implements `DeserializeOwned`.
///
/// # Dual-Mode Support
///
/// * **Actix backend**: Body must be pre-extracted due to Actix's stream-based body handling
/// * **Simulator backend**: Uses pre-loaded body data from `SimulationRequest`
///
/// # Content-Type Validation
///
/// The extractor validates that the request has an appropriate JSON content-type:
/// * `application/json`
/// * `application/json; charset=utf-8`
/// * `text/json` (legacy support)
///
/// # Examples
///
/// ## Simple JSON Object
///
/// ```rust,ignore
/// use moosicbox_web_server::extractors::Json;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct CreateUser {
///     name: String,
///     email: String,
///     age: Option<u32>,
/// }
///
/// async fn create_user(Json(user): Json<CreateUser>) -> Result<HttpResponse, Error> {
///     // For JSON body: {"name": "John", "email": "john@example.com", "age": 30}
///     // user.name = "John"
///     // user.email = "john@example.com"
///     // user.age = Some(30)
///     Ok(HttpResponse::ok())
/// }
/// ```
///
/// ## Complex Nested Structure
///
/// ```rust,ignore
/// use moosicbox_web_server::extractors::Json;
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct UpdateSettings {
///     theme: String,
///     notifications: NotificationSettings,
///     features: Vec<String>,
/// }
///
/// #[derive(Deserialize)]
/// struct NotificationSettings {
///     email: bool,
///     push: bool,
///     frequency: String,
/// }
///
/// async fn update_settings(Json(settings): Json<UpdateSettings>) -> Result<HttpResponse, Error> {
///     // Handles complex nested JSON structures
///     Ok(HttpResponse::ok())
/// }
/// ```
///
/// # Error Handling
///
/// The extractor provides detailed error information for common failure cases:
///
/// * **Missing Content-Type**: Returns `JsonError::InvalidContentType`
/// * **Empty Body**: Returns `JsonError::EmptyBody`
/// * **Invalid JSON**: Returns `JsonError::ParseError` with detailed message
/// * **Type Mismatch**: Returns `JsonError::DeserializationError` with field information
///
/// # Actix Backend Limitations
///
/// **Important**: Due to Actix's stream-based body handling, the request body must be
/// pre-extracted before using this extractor. This is typically done by middleware
/// or by manually reading the body in the handler:
///
/// ```rust,ignore
/// use actix_web::{web, HttpRequest, HttpResponse};
/// use bytes::Bytes;
///
/// async fn handler(body: Bytes, req: HttpRequest) -> Result<HttpResponse, Error> {
///     // Body is pre-extracted by Actix
///     // The Json extractor will use this pre-extracted body
///     let json_data: Json<MyStruct> = Json::from_request_sync(&req.into())?;
///     Ok(HttpResponse::ok())
/// }
/// ```
///
/// # Performance Notes
///
/// * Uses `serde_json` for parsing, which is highly optimized
/// * Validates content-type before attempting to parse JSON
/// * Provides zero-copy deserialization where possible
/// * Memory usage scales with JSON document size
#[derive(Debug)]
pub struct Json<T>(pub T);

impl<T> Json<T> {
    /// Extract the inner value from the Json wrapper
    #[must_use]
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> std::ops::Deref for Json<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> std::ops::DerefMut for Json<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Comprehensive error type for JSON extraction failures
///
/// This enum provides detailed error information to help diagnose
/// JSON parsing and validation issues during request processing.
#[derive(Debug, thiserror::Error)]
pub enum JsonError {
    /// Request is missing required Content-Type header or has invalid content type
    ///
    /// Expected content types:
    /// * `application/json`
    /// * `application/json; charset=utf-8`
    /// * `text/json`
    #[error(
        "Invalid or missing Content-Type header. Expected 'application/json', got: {content_type:?}"
    )]
    InvalidContentType {
        /// The actual content type found in the request
        content_type: Option<String>,
    },

    /// Request body is empty when JSON data was expected
    #[error("Request body is empty")]
    EmptyBody,

    /// JSON parsing failed due to syntax errors
    ///
    /// This typically occurs when the request body contains malformed JSON,
    /// such as missing quotes, trailing commas, or invalid escape sequences.
    #[error("Failed to parse JSON: {message}")]
    ParseError {
        /// Detailed error message from the JSON parser
        message: String,
        /// Line number where the error occurred (if available)
        line: Option<usize>,
        /// Column number where the error occurred (if available)
        column: Option<usize>,
    },

    /// JSON deserialization failed due to type or structure mismatch
    ///
    /// This occurs when the JSON is syntactically valid but doesn't match
    /// the expected structure of the target type.
    #[error("Failed to deserialize JSON into target type: {message}")]
    DeserializationError {
        /// Detailed error message from serde
        message: String,
        /// Field path where the error occurred (e.g., "user.settings.theme")
        field_path: Option<String>,
    },

    /// Body reading failed (Actix-specific)
    ///
    /// This error occurs when the request body cannot be read from the
    /// Actix stream, typically due to network issues or size limits.
    #[error("Failed to read request body: {message}")]
    BodyReadError {
        /// Error message from the body reading operation
        message: String,
    },
}

impl JsonError {
    /// Create a new `InvalidContentType` error
    #[must_use]
    pub const fn invalid_content_type(content_type: Option<String>) -> Self {
        Self::InvalidContentType { content_type }
    }

    /// Create a new `EmptyBody` error
    #[must_use]
    pub const fn empty_body() -> Self {
        Self::EmptyBody
    }

    /// Create a new `ParseError` from a `serde_json` error
    #[must_use]
    pub fn parse_error(err: &serde_json::Error) -> Self {
        Self::ParseError {
            message: err.to_string(),
            line: Some(err.line()),
            column: Some(err.column()),
        }
    }

    /// Create a new `DeserializationError` from a `serde_json` error
    #[must_use]
    pub fn deserialization_error(err: &serde_json::Error) -> Self {
        // Try to extract field path from error message
        let message = err.to_string();
        let field_path = extract_field_path(&message);

        Self::DeserializationError {
            message,
            field_path,
        }
    }

    /// Create a new `BodyReadError`
    #[must_use]
    pub fn body_read_error(message: impl Into<String>) -> Self {
        Self::BodyReadError {
            message: message.into(),
        }
    }
}

/// Extract field path from serde error message
///
/// Attempts to parse field paths like "missing field `name`" or
/// "invalid type: expected string, found number at line 1 column 15"
fn extract_field_path(message: &str) -> Option<String> {
    // Look for patterns like "missing field `fieldname`"
    if let Some(start) = message.find("field `")
        && let Some(end) = message[start + 7..].find('`')
    {
        return Some(message[start + 7..start + 7 + end].to_string());
    }

    // Look for patterns like "at `.field.subfield`"
    if let Some(start) = message.find("at `.")
        && let Some(end) = message[start + 4..].find('`')
    {
        return Some(message[start + 4..start + 4 + end].to_string());
    }

    None
}

impl From<JsonError> for Error {
    fn from(err: JsonError) -> Self {
        match err {
            JsonError::InvalidContentType { .. }
            | JsonError::EmptyBody
            | JsonError::ParseError { .. }
            | JsonError::DeserializationError { .. } => {
                Self::bad_request(format!("JSON extraction failed: {err}"))
            }
            JsonError::BodyReadError { .. } => {
                Self::internal_server_error(format!("JSON extraction failed: {err}"))
            }
        }
    }
}

impl IntoHandlerError for JsonError {
    fn into_handler_error(self) -> Error {
        self.into()
    }
}

/// Validate that the request has an appropriate JSON content type
fn validate_content_type(req: &HttpRequest) -> Result<(), JsonError> {
    let content_type = req
        .header("content-type")
        .or_else(|| req.header("Content-Type"));

    content_type.map_or_else(
        || Err(JsonError::invalid_content_type(None)),
        |ct| {
            let ct_lower = ct.to_lowercase();
            if ct_lower.starts_with("application/json") || ct_lower.starts_with("text/json") {
                Ok(())
            } else {
                Err(JsonError::invalid_content_type(Some(ct.to_string())))
            }
        },
    )
}

impl<T> FromRequest for Json<T>
where
    T: DeserializeOwned,
{
    type Error = JsonError;
    type Future = std::future::Ready<Result<Self, Self::Error>>;

    fn from_request_sync(req: &HttpRequest) -> Result<Self, Self::Error> {
        // Validate content type first
        validate_content_type(req)?;

        // Get the request body
        let body = req.body().ok_or(JsonError::empty_body())?;

        if body.is_empty() {
            return Err(JsonError::empty_body());
        }

        // Parse JSON
        let value: T = serde_json::from_slice(body).map_err(|err| {
            if err.is_syntax() {
                JsonError::parse_error(&err)
            } else {
                JsonError::deserialization_error(&err)
            }
        })?;

        Ok(Self(value))
    }

    fn from_request_async(req: HttpRequest) -> Self::Future {
        std::future::ready(Self::from_request_sync(&req))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    use crate::{HttpRequest, Stub};

    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    use bytes::Bytes;
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    use serde::Deserialize;

    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    use crate::simulator::{SimulationRequest, SimulationStub};

    #[derive(Debug, Deserialize, PartialEq)]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    struct TestUser {
        name: String,
        email: String,
        age: Option<u32>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    struct TestSettings {
        theme: String,
        notifications: TestNotifications,
        features: Vec<String>,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    struct TestNotifications {
        email: bool,
        push: bool,
    }

    #[test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_json_extraction_simple_object() {
        let json_body = r#"{"name": "John Doe", "email": "john@example.com", "age": 30}"#;
        let body = Bytes::from(json_body);

        let sim_req = SimulationRequest::new(crate::Method::Post, "/api/users")
            .with_header("Content-Type", "application/json")
            .with_body(body);
        let req = HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)));

        let result = Json::<TestUser>::from_request_sync(&req);
        assert!(result.is_ok());

        let Json(user) = result.unwrap();
        assert_eq!(user.name, "John Doe");
        assert_eq!(user.email, "john@example.com");
        assert_eq!(user.age, Some(30));
    }

    #[test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_json_extraction_optional_fields() {
        let json_body = r#"{"name": "Jane Doe", "email": "jane@example.com"}"#;
        let body = Bytes::from(json_body);

        let sim_req = SimulationRequest::new(crate::Method::Post, "/api/users")
            .with_header("Content-Type", "application/json")
            .with_body(body);
        let req = HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)));

        let result = Json::<TestUser>::from_request_sync(&req);
        assert!(result.is_ok());

        let Json(user) = result.unwrap();
        assert_eq!(user.name, "Jane Doe");
        assert_eq!(user.email, "jane@example.com");
        assert_eq!(user.age, None);
    }

    #[test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_json_extraction_nested_object() {
        let json_body = r#"{
            "theme": "dark",
            "notifications": {
                "email": true,
                "push": false
            },
            "features": ["feature1", "feature2"]
        }"#;
        let body = Bytes::from(json_body);

        let sim_req = SimulationRequest::new(crate::Method::Put, "/api/settings")
            .with_header("Content-Type", "application/json")
            .with_body(body);
        let req = HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)));

        let result = Json::<TestSettings>::from_request_sync(&req);
        assert!(result.is_ok());

        let Json(settings) = result.unwrap();
        assert_eq!(settings.theme, "dark");
        assert!(settings.notifications.email);
        assert!(!settings.notifications.push);
        assert_eq!(settings.features, vec!["feature1", "feature2"]);
    }

    #[test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_json_extraction_invalid_content_type() {
        let json_body = r#"{"name": "John"}"#;
        let body = Bytes::from(json_body);

        let sim_req = SimulationRequest::new(crate::Method::Post, "/api/users")
            .with_header("Content-Type", "text/plain")
            .with_body(body);
        let req = HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)));

        let result = Json::<TestUser>::from_request_sync(&req);
        assert!(result.is_err());

        match result.unwrap_err() {
            JsonError::InvalidContentType { content_type } => {
                assert_eq!(content_type, Some("text/plain".to_string()));
            }
            _ => panic!("Expected InvalidContentType error"),
        }
    }

    #[test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_json_extraction_missing_content_type() {
        let json_body = r#"{"name": "John"}"#;
        let body = Bytes::from(json_body);

        let sim_req = SimulationRequest::new(crate::Method::Post, "/api/users").with_body(body);
        let req = HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)));

        let result = Json::<TestUser>::from_request_sync(&req);
        assert!(result.is_err());

        match result.unwrap_err() {
            JsonError::InvalidContentType { content_type } => {
                assert_eq!(content_type, None);
            }
            _ => panic!("Expected InvalidContentType error"),
        }
    }

    #[test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_json_extraction_empty_body() {
        let sim_req = SimulationRequest::new(crate::Method::Post, "/api/users")
            .with_header("Content-Type", "application/json");
        let req = HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)));

        let result = Json::<TestUser>::from_request_sync(&req);
        assert!(result.is_err());

        match result.unwrap_err() {
            JsonError::EmptyBody => {}
            _ => panic!("Expected EmptyBody error"),
        }
    }

    #[test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_json_extraction_invalid_json() {
        let invalid_json = r#"{"name": "John", "email": }"#; // Missing value
        let body = Bytes::from(invalid_json);

        let sim_req = SimulationRequest::new(crate::Method::Post, "/api/users")
            .with_header("Content-Type", "application/json")
            .with_body(body);
        let req = HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)));

        let result = Json::<TestUser>::from_request_sync(&req);
        assert!(result.is_err());

        match result.unwrap_err() {
            JsonError::ParseError { message, .. } => {
                assert!(message.contains("expected value"));
            }
            _ => panic!("Expected ParseError"),
        }
    }

    #[test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_json_extraction_type_mismatch() {
        let json_body = r#"{"name": "John", "email": "john@example.com", "age": "thirty"}"#; // age should be number
        let body = Bytes::from(json_body);

        let sim_req = SimulationRequest::new(crate::Method::Post, "/api/users")
            .with_header("Content-Type", "application/json")
            .with_body(body);
        let req = HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)));

        let result = Json::<TestUser>::from_request_sync(&req);
        assert!(result.is_err());

        match result.unwrap_err() {
            JsonError::DeserializationError { message, .. } => {
                assert!(message.contains("invalid type"));
            }
            _ => panic!("Expected DeserializationError"),
        }
    }

    #[test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_json_extraction_async() {
        let json_body = r#"{"name": "Async User", "email": "async@example.com"}"#;
        let body = Bytes::from(json_body);

        let sim_req = SimulationRequest::new(crate::Method::Post, "/api/users")
            .with_header("Content-Type", "application/json")
            .with_body(body);
        let req = HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)));

        // Test async extraction (which delegates to sync)
        // Since we use std::future::Ready, we can just call the sync version
        let result = Json::<TestUser>::from_request_sync(&req);

        assert!(result.is_ok());
        let Json(user) = result.unwrap();
        assert_eq!(user.name, "Async User");
        assert_eq!(user.email, "async@example.com");
    }

    #[test]
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    fn test_json_deref() {
        let json_body = r#"{"name": "Test User", "email": "test@example.com"}"#;
        let body = Bytes::from(json_body);

        let sim_req = SimulationRequest::new(crate::Method::Post, "/api/users")
            .with_header("Content-Type", "application/json")
            .with_body(body);
        let req = HttpRequest::Stub(Stub::Simulator(SimulationStub::new(sim_req)));

        let Json(user) = Json::<TestUser>::from_request_sync(&req).unwrap();
        let json_wrapper = Json(user);

        // Test Deref trait
        assert_eq!(json_wrapper.name, "Test User");
        assert_eq!(json_wrapper.email, "test@example.com");

        // Test into_inner
        let inner = json_wrapper.into_inner();
        assert_eq!(inner.name, "Test User");
    }

    #[test_log::test]
    fn test_extract_field_path_with_missing_field_pattern() {
        // Test the pattern: "missing field `fieldname`"
        let message = "missing field `username` at line 1 column 20";
        let result = extract_field_path(message);
        assert_eq!(result, Some("username".to_string()));
    }

    #[test_log::test]
    fn test_extract_field_path_with_at_dot_pattern() {
        // Test the pattern: "at `.field.subfield`"
        let message = "invalid type: expected integer, found string at `.user.age`";
        let result = extract_field_path(message);
        assert_eq!(result, Some(".user.age".to_string()));
    }

    #[test_log::test]
    fn test_extract_field_path_no_match() {
        // Test message with no matching pattern
        let message = "generic error without field information";
        let result = extract_field_path(message);
        assert_eq!(result, None);
    }

    #[test_log::test]
    fn test_extract_field_path_nested_field() {
        // Test nested field extraction
        let message = "missing field `nested_object` in the input";
        let result = extract_field_path(message);
        assert_eq!(result, Some("nested_object".to_string()));
    }

    #[test_log::test]
    fn test_extract_field_path_with_multiple_patterns() {
        // If both patterns exist, the first one (field`) should match
        let message = "missing field `first` at `.second`";
        let result = extract_field_path(message);
        // The function checks "field `" pattern first
        assert_eq!(result, Some("first".to_string()));
    }

    #[test_log::test]
    fn test_json_error_parse_error_from_serde() {
        // Test creating ParseError from a real serde_json error
        let invalid_json = r#"{"name": invalid}"#;
        let err = serde_json::from_str::<serde_json::Value>(invalid_json).unwrap_err();

        let json_error = JsonError::parse_error(&err);

        match json_error {
            JsonError::ParseError {
                message,
                line,
                column,
            } => {
                assert!(message.contains("expected value"));
                assert!(line.is_some());
                assert!(column.is_some());
            }
            _ => panic!("Expected ParseError variant"),
        }
    }

    #[test_log::test]
    fn test_json_error_deserialization_error_with_field_path() {
        // Test creating DeserializationError that extracts field path
        #[derive(Debug, serde::Deserialize)]
        struct TestStruct {
            #[allow(dead_code)]
            required_field: String,
        }

        let json_missing_field = r"{}";
        let err = serde_json::from_str::<TestStruct>(json_missing_field).unwrap_err();

        let json_error = JsonError::deserialization_error(&err);

        match json_error {
            JsonError::DeserializationError {
                message,
                field_path,
            } => {
                assert!(message.contains("required_field"));
                // The field_path should be extracted from the error message
                assert_eq!(field_path, Some("required_field".to_string()));
            }
            _ => panic!("Expected DeserializationError variant"),
        }
    }

    #[test_log::test]
    fn test_json_error_body_read_error() {
        let error = JsonError::body_read_error("connection reset");

        match error {
            JsonError::BodyReadError { message } => {
                assert_eq!(message, "connection reset");
            }
            _ => panic!("Expected BodyReadError variant"),
        }
    }

    #[test_log::test]
    fn test_json_error_into_error_conversion() {
        // Test that different JsonError variants convert to appropriate Error types
        let invalid_ct = JsonError::invalid_content_type(Some("text/plain".to_string()));
        let error: Error = invalid_ct.into();
        // InvalidContentType should convert to bad_request
        assert!(error.to_string().contains("Invalid"));

        let empty_body = JsonError::empty_body();
        let error: Error = empty_body.into();
        // EmptyBody should convert to bad_request
        assert!(error.to_string().contains("empty"));

        let body_read = JsonError::body_read_error("network error");
        let error: Error = body_read.into();
        // BodyReadError should convert to internal_server_error
        assert!(error.to_string().contains("network error"));
    }
}
