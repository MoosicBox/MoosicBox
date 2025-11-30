//! HTTP request testing utilities.
//!
//! This module provides types for building and executing HTTP requests in test
//! scenarios, with support for different methods, headers, and body formats.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// An HTTP request step for testing API endpoints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequestStep {
    /// The HTTP method.
    pub method: HttpMethod,
    /// The target URL.
    pub url: String,
    /// Request headers.
    pub headers: BTreeMap<String, String>,
    /// Optional request body.
    pub body: Option<RequestBody>,
    /// Expected HTTP status code.
    pub expected_status: Option<u16>,
    /// Request timeout.
    pub timeout: Option<std::time::Duration>,
}

impl HttpRequestStep {
    /// Creates a GET request.
    #[must_use]
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Get,
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            expected_status: None,
            timeout: None,
        }
    }

    /// Creates a POST request.
    #[must_use]
    pub fn post(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Post,
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            expected_status: None,
            timeout: None,
        }
    }

    /// Creates a PUT request.
    #[must_use]
    pub fn put(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Put,
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            expected_status: None,
            timeout: None,
        }
    }

    /// Creates a DELETE request.
    #[must_use]
    pub fn delete(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Delete,
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            expected_status: None,
            timeout: None,
        }
    }

    /// Adds a header to the request.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    /// Adds multiple headers to the request.
    #[must_use]
    pub fn with_headers(mut self, headers: BTreeMap<String, String>) -> Self {
        self.headers.extend(headers);
        self
    }

    /// Sets the request body.
    #[must_use]
    pub fn with_body(mut self, body: RequestBody) -> Self {
        self.body = Some(body);
        self
    }

    /// Sets a JSON request body and content-type header.
    #[must_use]
    pub fn json(mut self, value: serde_json::Value) -> Self {
        self.body = Some(RequestBody::Json(value));
        self.headers
            .insert("content-type".to_string(), "application/json".to_string());
        self
    }

    /// Sets a plain text request body and content-type header.
    #[must_use]
    pub fn text(mut self, text: impl Into<String>) -> Self {
        self.body = Some(RequestBody::Text(text.into()));
        self.headers
            .insert("content-type".to_string(), "text/plain".to_string());
        self
    }

    /// Sets a form-encoded request body and content-type header.
    #[must_use]
    pub fn form(mut self, data: BTreeMap<String, String>) -> Self {
        self.body = Some(RequestBody::Form(data));
        self.headers.insert(
            "content-type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        );
        self
    }

    /// Sets the expected HTTP status code for validation.
    #[must_use]
    pub const fn expect_status(mut self, status: u16) -> Self {
        self.expected_status = Some(status);
        self
    }

    /// Sets the request timeout duration.
    #[must_use]
    pub const fn with_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Returns a human-readable description of this HTTP request.
    #[must_use]
    pub fn description(&self) -> String {
        format!("{} {}", self.method, self.url)
    }
}

/// HTTP request method.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HttpMethod {
    /// GET method.
    Get,
    /// POST method.
    Post,
    /// PUT method.
    Put,
    /// DELETE method.
    Delete,
    /// PATCH method.
    Patch,
    /// HEAD method.
    Head,
    /// OPTIONS method.
    Options,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Get => write!(f, "GET"),
            Self::Post => write!(f, "POST"),
            Self::Put => write!(f, "PUT"),
            Self::Delete => write!(f, "DELETE"),
            Self::Patch => write!(f, "PATCH"),
            Self::Head => write!(f, "HEAD"),
            Self::Options => write!(f, "OPTIONS"),
        }
    }
}

/// HTTP request body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestBody {
    /// Plain text body.
    Text(String),
    /// JSON body.
    Json(serde_json::Value),
    /// Form-encoded body.
    Form(BTreeMap<String, String>),
    /// Binary body.
    Binary(Vec<u8>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // HttpRequestStep builder tests
    #[test_log::test]
    fn http_request_get() {
        let request = HttpRequestStep::get("/api/users");
        assert!(matches!(request.method, HttpMethod::Get));
        assert_eq!(request.url, "/api/users");
        assert!(request.headers.is_empty());
        assert!(request.body.is_none());
    }

    #[test_log::test]
    fn http_request_post() {
        let request = HttpRequestStep::post("/api/users");
        assert!(matches!(request.method, HttpMethod::Post));
        assert_eq!(request.url, "/api/users");
    }

    #[test_log::test]
    fn http_request_put() {
        let request = HttpRequestStep::put("/api/users/1");
        assert!(matches!(request.method, HttpMethod::Put));
        assert_eq!(request.url, "/api/users/1");
    }

    #[test_log::test]
    fn http_request_delete() {
        let request = HttpRequestStep::delete("/api/users/1");
        assert!(matches!(request.method, HttpMethod::Delete));
        assert_eq!(request.url, "/api/users/1");
    }

    #[test_log::test]
    fn http_request_with_header() {
        let request = HttpRequestStep::get("/api").with_header("Authorization", "Bearer token123");

        assert_eq!(
            request.headers.get("Authorization").unwrap(),
            "Bearer token123"
        );
    }

    #[test_log::test]
    fn http_request_with_multiple_headers() {
        let request = HttpRequestStep::get("/api")
            .with_header("X-Custom-Header", "value1")
            .with_header("X-Another-Header", "value2");

        assert_eq!(request.headers.len(), 2);
        assert_eq!(request.headers.get("X-Custom-Header").unwrap(), "value1");
        assert_eq!(request.headers.get("X-Another-Header").unwrap(), "value2");
    }

    #[test_log::test]
    fn http_request_with_headers_map() {
        let mut headers = BTreeMap::new();
        headers.insert("Accept".to_string(), "application/json".to_string());
        headers.insert("X-Api-Key".to_string(), "secret".to_string());

        let request = HttpRequestStep::get("/api").with_headers(headers);

        assert_eq!(request.headers.len(), 2);
        assert_eq!(request.headers.get("Accept").unwrap(), "application/json");
        assert_eq!(request.headers.get("X-Api-Key").unwrap(), "secret");
    }

    #[test_log::test]
    fn http_request_json_body_sets_content_type() {
        let json_value = serde_json::json!({"name": "test", "value": 42});
        let request = HttpRequestStep::post("/api/data").json(json_value.clone());

        assert_eq!(
            request.headers.get("content-type").unwrap(),
            "application/json"
        );
        if let Some(RequestBody::Json(body)) = &request.body {
            assert_eq!(*body, json_value);
        } else {
            panic!("Expected JSON body");
        }
    }

    #[test_log::test]
    fn http_request_text_body_sets_content_type() {
        let request = HttpRequestStep::post("/api/data").text("Hello, World!");

        assert_eq!(request.headers.get("content-type").unwrap(), "text/plain");
        if let Some(RequestBody::Text(body)) = &request.body {
            assert_eq!(body, "Hello, World!");
        } else {
            panic!("Expected Text body");
        }
    }

    #[test_log::test]
    fn http_request_form_body_sets_content_type() {
        let mut form_data = BTreeMap::new();
        form_data.insert("username".to_string(), "john".to_string());
        form_data.insert("password".to_string(), "secret".to_string());

        let request = HttpRequestStep::post("/api/login").form(form_data);

        assert_eq!(
            request.headers.get("content-type").unwrap(),
            "application/x-www-form-urlencoded"
        );
        if let Some(RequestBody::Form(body)) = &request.body {
            assert_eq!(body.get("username").unwrap(), "john");
            assert_eq!(body.get("password").unwrap(), "secret");
        } else {
            panic!("Expected Form body");
        }
    }

    #[test_log::test]
    fn http_request_with_body() {
        let request =
            HttpRequestStep::post("/api/upload").with_body(RequestBody::Binary(vec![1, 2, 3, 4]));

        if let Some(RequestBody::Binary(body)) = &request.body {
            assert_eq!(body, &vec![1, 2, 3, 4]);
        } else {
            panic!("Expected Binary body");
        }
    }

    #[test_log::test]
    fn http_request_expect_status() {
        let request = HttpRequestStep::get("/api/health").expect_status(200);
        assert_eq!(request.expected_status, Some(200));
    }

    #[test_log::test]
    fn http_request_with_timeout() {
        let request = HttpRequestStep::get("/api/slow").with_timeout(Duration::from_secs(30));
        assert_eq!(request.timeout, Some(Duration::from_secs(30)));
    }

    #[test_log::test]
    fn http_request_description_get() {
        let request = HttpRequestStep::get("/api/users");
        assert_eq!(request.description(), "GET /api/users");
    }

    #[test_log::test]
    fn http_request_description_post() {
        let request = HttpRequestStep::post("/api/users");
        assert_eq!(request.description(), "POST /api/users");
    }

    #[test_log::test]
    fn http_request_chained_builder() {
        let request = HttpRequestStep::post("/api/data")
            .with_header("Authorization", "Bearer token")
            .json(serde_json::json!({"key": "value"}))
            .expect_status(201)
            .with_timeout(Duration::from_secs(10));

        assert!(matches!(request.method, HttpMethod::Post));
        assert_eq!(request.url, "/api/data");
        assert_eq!(
            request.headers.get("Authorization").unwrap(),
            "Bearer token"
        );
        assert_eq!(
            request.headers.get("content-type").unwrap(),
            "application/json"
        );
        assert!(request.body.is_some());
        assert_eq!(request.expected_status, Some(201));
        assert_eq!(request.timeout, Some(Duration::from_secs(10)));
    }
}
