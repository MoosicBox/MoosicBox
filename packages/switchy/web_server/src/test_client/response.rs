use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::de::DeserializeOwned;

/// Unified test response wrapper for both Actix and Simulator backends
#[derive(Debug, Clone)]
pub struct TestResponse {
    /// HTTP status code (e.g., 200, 404, 500)
    pub status: u16,
    /// Response headers as key-value pairs
    pub headers: BTreeMap<String, String>,
    /// Response body as raw bytes
    pub body: Vec<u8>,
}

impl TestResponse {
    /// Create a new test response
    #[must_use]
    pub const fn new(status: u16, headers: BTreeMap<String, String>, body: Vec<u8>) -> Self {
        Self {
            status,
            headers,
            body,
        }
    }

    /// Get the response status code
    #[must_use]
    pub const fn status(&self) -> u16 {
        self.status
    }

    /// Get response headers
    #[must_use]
    pub const fn headers(&self) -> &BTreeMap<String, String> {
        &self.headers
    }

    /// Get a specific header value
    #[must_use]
    pub fn header(&self, name: &str) -> Option<&String> {
        self.headers.get(name)
    }

    /// Get the response body as bytes
    #[must_use]
    pub fn body(&self) -> &[u8] {
        &self.body
    }

    /// Get the response body as a UTF-8 string
    ///
    /// # Errors
    /// * Returns error if the body is not valid UTF-8
    pub fn text(&self) -> Result<String, std::str::Utf8Error> {
        std::str::from_utf8(&self.body).map(ToString::to_string)
    }

    /// Parse the response body as JSON
    ///
    /// # Errors
    /// * Returns error if JSON parsing fails
    #[cfg(feature = "serde")]
    pub fn json<T: DeserializeOwned>(&self) -> Result<T, serde_json::Error> {
        serde_json::from_slice(&self.body)
    }

    /// Check if the response status is successful (2xx)
    #[must_use]
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Check if the response status is a client error (4xx)
    #[must_use]
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.status)
    }

    /// Check if the response status is a server error (5xx)
    #[must_use]
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.status)
    }

    /// Check if the response status is a redirection (3xx)
    #[must_use]
    pub fn is_redirection(&self) -> bool {
        (300..400).contains(&self.status)
    }
}

/// Extension trait for `TestResponse` with assertion helpers
pub trait TestResponseExt {
    /// Assert that the response has the expected status code
    ///
    /// # Panics
    /// * Panics if the status code doesn't match
    fn assert_status(&self, expected: u16) -> &Self;

    /// Assert that the response is successful (2xx)
    ///
    /// # Panics
    /// * Panics if the status is not in the 2xx range
    fn assert_success(&self) -> &Self;

    /// Assert that the response is a client error (4xx)
    ///
    /// # Panics
    /// * Panics if the status is not in the 4xx range
    fn assert_client_error(&self) -> &Self;

    /// Assert that the response is a server error (5xx)
    ///
    /// # Panics
    /// * Panics if the status is not in the 5xx range
    fn assert_server_error(&self) -> &Self;

    /// Assert that the response has a specific header
    ///
    /// # Panics
    /// * Panics if the header is not present
    fn assert_header(&self, name: &str, expected: &str) -> &Self;

    /// Assert that the response has a specific header (case-insensitive)
    ///
    /// # Panics
    /// * Panics if the header is not present
    fn assert_header_contains(&self, name: &str, expected: &str) -> &Self;

    /// Assert that the response body contains the expected text
    ///
    /// # Panics
    /// * Panics if the body doesn't contain the expected text
    /// * Panics if the body is not valid UTF-8
    fn assert_text_contains(&self, expected: &str) -> &Self;

    /// Assert that the response body equals the expected text
    ///
    /// # Panics
    /// * Panics if the body doesn't equal the expected text
    /// * Panics if the body is not valid UTF-8
    fn assert_text_equals(&self, expected: &str) -> &Self;

    /// Assert that the response body can be parsed as JSON and equals the expected value
    ///
    /// # Panics
    /// * Panics if JSON parsing fails
    /// * Panics if the parsed JSON doesn't equal the expected value
    #[cfg(feature = "serde")]
    fn assert_json_equals<T: DeserializeOwned + PartialEq + std::fmt::Debug>(
        &self,
        expected: &T,
    ) -> &Self;

    /// Assert that the response body can be parsed as JSON and contains the expected fields
    ///
    /// # Panics
    /// * Panics if JSON parsing fails
    /// * Panics if the expected fields are not present
    #[cfg(feature = "serde")]
    fn assert_json_contains(&self, expected: &serde_json::Value) -> &Self;
}

impl TestResponseExt for TestResponse {
    fn assert_status(&self, expected: u16) -> &Self {
        assert_eq!(
            self.status, expected,
            "Expected status {expected}, got {}",
            self.status
        );
        self
    }

    fn assert_success(&self) -> &Self {
        assert!(
            self.is_success(),
            "Expected successful status (2xx), got {}",
            self.status
        );
        self
    }

    fn assert_client_error(&self) -> &Self {
        assert!(
            self.is_client_error(),
            "Expected client error status (4xx), got {}",
            self.status
        );
        self
    }

    fn assert_server_error(&self) -> &Self {
        assert!(
            self.is_server_error(),
            "Expected server error status (5xx), got {}",
            self.status
        );
        self
    }

    fn assert_header(&self, name: &str, expected: &str) -> &Self {
        let actual = self
            .header(name)
            .unwrap_or_else(|| panic!("Header '{name}' not found"));
        assert_eq!(
            actual, expected,
            "Expected header '{name}' to be '{expected}', got '{actual}'"
        );
        self
    }

    fn assert_header_contains(&self, name: &str, expected: &str) -> &Self {
        let actual = self
            .header(name)
            .unwrap_or_else(|| panic!("Header '{name}' not found"));
        assert!(
            actual.to_lowercase().contains(&expected.to_lowercase()),
            "Expected header '{name}' to contain '{expected}', got '{actual}'"
        );
        self
    }

    fn assert_text_contains(&self, expected: &str) -> &Self {
        let text = self.text().expect("Response body is not valid UTF-8");
        assert!(
            text.contains(expected),
            "Expected response body to contain '{expected}', got: {text}"
        );
        self
    }

    fn assert_text_equals(&self, expected: &str) -> &Self {
        let text = self.text().expect("Response body is not valid UTF-8");
        assert_eq!(
            text, expected,
            "Expected response body to equal '{expected}', got: {text}"
        );
        self
    }

    #[cfg(feature = "serde")]
    fn assert_json_equals<T: DeserializeOwned + PartialEq + std::fmt::Debug>(
        &self,
        expected: &T,
    ) -> &Self {
        let actual: T = self.json().expect("Failed to parse response body as JSON");
        assert_eq!(
            &actual, expected,
            "Expected JSON to equal {expected:?}, got {actual:?}"
        );
        self
    }

    #[cfg(feature = "serde")]
    fn assert_json_contains(&self, expected: &serde_json::Value) -> &Self {
        fn contains_value(actual: &serde_json::Value, expected: &serde_json::Value) -> bool {
            match (actual, expected) {
                (
                    serde_json::Value::Object(actual_obj),
                    serde_json::Value::Object(expected_obj),
                ) => expected_obj.iter().all(|(key, expected_val)| {
                    actual_obj
                        .get(key)
                        .is_some_and(|actual_val| contains_value(actual_val, expected_val))
                }),
                (actual_val, expected_val) => actual_val == expected_val,
            }
        }

        let actual: serde_json::Value = self.json().expect("Failed to parse response body as JSON");

        assert!(
            contains_value(&actual, expected),
            "Expected JSON to contain {expected}, got {actual}"
        );
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_response(status: u16, body: &[u8]) -> TestResponse {
        TestResponse::new(status, BTreeMap::new(), body.to_vec())
    }

    fn create_response_with_headers(
        status: u16,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> TestResponse {
        let headers = headers
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        TestResponse::new(status, headers, body.to_vec())
    }

    #[test_log::test]
    fn test_response_status_classification_success() {
        assert!(create_response(200, b"").is_success());
        assert!(create_response(201, b"").is_success());
        assert!(create_response(204, b"").is_success());
        assert!(create_response(299, b"").is_success());

        assert!(!create_response(199, b"").is_success());
        assert!(!create_response(300, b"").is_success());
    }

    #[test_log::test]
    fn test_response_status_classification_client_error() {
        assert!(create_response(400, b"").is_client_error());
        assert!(create_response(401, b"").is_client_error());
        assert!(create_response(404, b"").is_client_error());
        assert!(create_response(499, b"").is_client_error());

        assert!(!create_response(399, b"").is_client_error());
        assert!(!create_response(500, b"").is_client_error());
    }

    #[test_log::test]
    fn test_response_status_classification_server_error() {
        assert!(create_response(500, b"").is_server_error());
        assert!(create_response(502, b"").is_server_error());
        assert!(create_response(503, b"").is_server_error());
        assert!(create_response(599, b"").is_server_error());

        assert!(!create_response(499, b"").is_server_error());
        assert!(!create_response(600, b"").is_server_error());
    }

    #[test_log::test]
    fn test_response_status_classification_redirection() {
        assert!(create_response(301, b"").is_redirection());
        assert!(create_response(302, b"").is_redirection());
        assert!(create_response(307, b"").is_redirection());
        assert!(create_response(399, b"").is_redirection());

        assert!(!create_response(299, b"").is_redirection());
        assert!(!create_response(400, b"").is_redirection());
    }

    #[test_log::test]
    fn test_response_text_valid_utf8() {
        let response = create_response(200, b"Hello, World!");
        let text = response.text().unwrap();
        assert_eq!(text, "Hello, World!");
    }

    #[test_log::test]
    fn test_response_text_invalid_utf8() {
        // Invalid UTF-8 sequence
        let invalid_utf8 = vec![0xff, 0xfe, 0x00, 0x01];
        let response = create_response(200, &invalid_utf8);
        let result = response.text();
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_response_header_retrieval() {
        let response = create_response_with_headers(
            200,
            &[
                ("Content-Type", "application/json"),
                ("X-Custom", "custom-value"),
            ],
            b"{}",
        );

        assert_eq!(
            response.header("Content-Type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(
            response.header("X-Custom"),
            Some(&"custom-value".to_string())
        );
        assert_eq!(response.header("X-Missing"), None);
    }

    #[test_log::test]
    #[cfg(feature = "serde")]
    fn test_response_json_parsing_valid() {
        let json_body = r#"{"name": "test", "value": 123}"#;
        let response = create_response(200, json_body.as_bytes());

        let parsed: serde_json::Value = response.json().unwrap();
        assert_eq!(parsed["name"], "test");
        assert_eq!(parsed["value"], 123);
    }

    #[test_log::test]
    #[cfg(feature = "serde")]
    fn test_response_json_parsing_invalid() {
        let invalid_json = r#"{"name": invalid}"#;
        let response = create_response(200, invalid_json.as_bytes());

        let result: Result<serde_json::Value, _> = response.json();
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_assert_status() {
        let response = create_response(200, b"OK");
        response.assert_status(200);
    }

    #[test_log::test]
    #[should_panic(expected = "Expected status 404, got 200")]
    fn test_assert_status_fails() {
        let response = create_response(200, b"OK");
        response.assert_status(404);
    }

    #[test_log::test]
    fn test_assert_success() {
        create_response(200, b"").assert_success();
        create_response(201, b"").assert_success();
    }

    #[test_log::test]
    #[should_panic(expected = "Expected successful status (2xx), got 404")]
    fn test_assert_success_fails() {
        create_response(404, b"").assert_success();
    }

    #[test_log::test]
    fn test_assert_client_error() {
        create_response(400, b"").assert_client_error();
        create_response(404, b"").assert_client_error();
    }

    #[test_log::test]
    #[should_panic(expected = "Expected client error status (4xx), got 200")]
    fn test_assert_client_error_fails() {
        create_response(200, b"").assert_client_error();
    }

    #[test_log::test]
    fn test_assert_server_error() {
        create_response(500, b"").assert_server_error();
        create_response(503, b"").assert_server_error();
    }

    #[test_log::test]
    #[should_panic(expected = "Expected server error status (5xx), got 200")]
    fn test_assert_server_error_fails() {
        create_response(200, b"").assert_server_error();
    }

    #[test_log::test]
    fn test_assert_header() {
        let response =
            create_response_with_headers(200, &[("Content-Type", "application/json")], b"{}");
        response.assert_header("Content-Type", "application/json");
    }

    #[test_log::test]
    #[should_panic(expected = "Header 'X-Missing' not found")]
    fn test_assert_header_missing() {
        let response = create_response(200, b"");
        response.assert_header("X-Missing", "value");
    }

    #[test_log::test]
    fn test_assert_header_contains() {
        let response = create_response_with_headers(
            200,
            &[("Content-Type", "application/json; charset=utf-8")],
            b"{}",
        );
        response.assert_header_contains("Content-Type", "json");
    }

    #[test_log::test]
    fn test_assert_text_contains() {
        let response = create_response(200, b"Hello, World!");
        response.assert_text_contains("World");
    }

    #[test_log::test]
    #[should_panic(expected = "Expected response body to contain 'missing'")]
    fn test_assert_text_contains_fails() {
        let response = create_response(200, b"Hello, World!");
        response.assert_text_contains("missing");
    }

    #[test_log::test]
    fn test_assert_text_equals() {
        let response = create_response(200, b"exact match");
        response.assert_text_equals("exact match");
    }

    #[test_log::test]
    #[should_panic(expected = "Expected response body to equal 'different'")]
    fn test_assert_text_equals_fails() {
        let response = create_response(200, b"actual");
        response.assert_text_equals("different");
    }

    #[test_log::test]
    #[cfg(feature = "serde")]
    fn test_assert_json_equals() {
        let json_body = r#"{"name":"test","value":123}"#;
        let response = create_response(200, json_body.as_bytes());

        let expected: serde_json::Value = serde_json::json!({"name": "test", "value": 123});
        response.assert_json_equals(&expected);
    }

    #[test_log::test]
    #[cfg(feature = "serde")]
    fn test_assert_json_contains() {
        let json_body = r#"{"name":"test","value":123,"extra":"ignored"}"#;
        let response = create_response(200, json_body.as_bytes());

        let expected = serde_json::json!({"name": "test"});
        response.assert_json_contains(&expected);
    }

    #[test_log::test]
    #[cfg(feature = "serde")]
    fn test_assert_json_contains_nested() {
        let json_body = r#"{"user":{"name":"test","age":30},"active":true}"#;
        let response = create_response(200, json_body.as_bytes());

        // Test that nested objects work correctly
        let expected = serde_json::json!({"user": {"name": "test"}});
        response.assert_json_contains(&expected);
    }
}
