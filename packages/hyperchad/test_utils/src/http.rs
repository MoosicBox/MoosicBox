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
    use pretty_assertions::assert_eq;
    use std::time::Duration;

    #[test]
    fn test_http_request_step_get() {
        let request = HttpRequestStep::get("https://api.example.com/users");
        match request.method {
            HttpMethod::Get => {}
            _ => panic!("Expected GET method"),
        }
        assert_eq!(request.url, "https://api.example.com/users");
        assert!(request.body.is_none());
        assert!(request.expected_status.is_none());
    }

    #[test]
    fn test_http_request_step_post() {
        let request = HttpRequestStep::post("/api/login");
        match request.method {
            HttpMethod::Post => {}
            _ => panic!("Expected POST method"),
        }
        assert_eq!(request.url, "/api/login");
    }

    #[test]
    fn test_http_request_step_put() {
        let request = HttpRequestStep::put("/api/users/1");
        match request.method {
            HttpMethod::Put => {}
            _ => panic!("Expected PUT method"),
        }
        assert_eq!(request.url, "/api/users/1");
    }

    #[test]
    fn test_http_request_step_delete() {
        let request = HttpRequestStep::delete("/api/users/1");
        match request.method {
            HttpMethod::Delete => {}
            _ => panic!("Expected DELETE method"),
        }
        assert_eq!(request.url, "/api/users/1");
    }

    #[test]
    fn test_http_request_step_with_header() {
        let request = HttpRequestStep::get("/api/data")
            .with_header("Authorization", "Bearer token123")
            .with_header("Accept", "application/json");

        assert_eq!(request.headers.len(), 2);
        assert_eq!(
            request.headers.get("Authorization"),
            Some(&"Bearer token123".to_string())
        );
        assert_eq!(
            request.headers.get("Accept"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn test_http_request_step_with_headers() {
        let mut headers = BTreeMap::new();
        headers.insert("X-Custom-Header".to_string(), "value1".to_string());
        headers.insert("X-Another-Header".to_string(), "value2".to_string());

        let request = HttpRequestStep::get("/api/data").with_headers(headers);
        assert_eq!(request.headers.len(), 2);
    }

    #[test]
    fn test_http_request_step_with_body() {
        let body = RequestBody::Text("test data".to_string());
        let request = HttpRequestStep::post("/api/data").with_body(body);

        assert!(request.body.is_some());
        match request.body.unwrap() {
            RequestBody::Text(text) => assert_eq!(text, "test data"),
            _ => panic!("Expected Text body"),
        }
    }

    #[test]
    fn test_http_request_step_json() {
        let json_value = serde_json::json!({"name": "John", "age": 30});
        let request = HttpRequestStep::post("/api/users").json(json_value.clone());

        assert!(request.body.is_some());
        match request.body.unwrap() {
            RequestBody::Json(value) => assert_eq!(value, json_value),
            _ => panic!("Expected JSON body"),
        }
        assert_eq!(
            request.headers.get("content-type"),
            Some(&"application/json".to_string())
        );
    }

    #[test]
    fn test_http_request_step_text() {
        let request = HttpRequestStep::post("/api/log").text("Log message");

        assert!(request.body.is_some());
        match request.body.unwrap() {
            RequestBody::Text(text) => assert_eq!(text, "Log message"),
            _ => panic!("Expected Text body"),
        }
        assert_eq!(
            request.headers.get("content-type"),
            Some(&"text/plain".to_string())
        );
    }

    #[test]
    fn test_http_request_step_form() {
        let mut form_data = BTreeMap::new();
        form_data.insert("username".to_string(), "testuser".to_string());
        form_data.insert("password".to_string(), "secret".to_string());

        let request = HttpRequestStep::post("/api/login").form(form_data.clone());

        assert!(request.body.is_some());
        match request.body.unwrap() {
            RequestBody::Form(data) => assert_eq!(data, form_data),
            _ => panic!("Expected Form body"),
        }
        assert_eq!(
            request.headers.get("content-type"),
            Some(&"application/x-www-form-urlencoded".to_string())
        );
    }

    #[test]
    fn test_http_request_step_expect_status() {
        let request = HttpRequestStep::get("/api/data").expect_status(200);
        assert_eq!(request.expected_status, Some(200));
    }

    #[test]
    fn test_http_request_step_with_timeout() {
        let timeout = Duration::from_secs(30);
        let request = HttpRequestStep::get("/api/data").with_timeout(timeout);
        assert_eq!(request.timeout, Some(timeout));
    }

    #[test]
    fn test_http_request_step_description() {
        let request = HttpRequestStep::get("https://api.example.com/users");
        assert_eq!(request.description(), "GET https://api.example.com/users");

        let request = HttpRequestStep::post("/api/login");
        assert_eq!(request.description(), "POST /api/login");
    }

    #[test]
    fn test_http_request_builder_chaining() {
        let request = HttpRequestStep::post("/api/users")
            .json(serde_json::json!({"name": "Alice"}))
            .with_header("Authorization", "Bearer token")
            .expect_status(201)
            .with_timeout(Duration::from_secs(10));

        assert!(request.body.is_some());
        assert_eq!(request.headers.len(), 2); // content-type + Authorization
        assert_eq!(request.expected_status, Some(201));
        assert_eq!(request.timeout, Some(Duration::from_secs(10)));
    }

    #[test]
    fn test_http_method_display() {
        assert_eq!(format!("{}", HttpMethod::Get), "GET");
        assert_eq!(format!("{}", HttpMethod::Post), "POST");
        assert_eq!(format!("{}", HttpMethod::Put), "PUT");
        assert_eq!(format!("{}", HttpMethod::Delete), "DELETE");
        assert_eq!(format!("{}", HttpMethod::Patch), "PATCH");
        assert_eq!(format!("{}", HttpMethod::Head), "HEAD");
        assert_eq!(format!("{}", HttpMethod::Options), "OPTIONS");
    }

    #[test]
    fn test_http_method_serialization() {
        let method = HttpMethod::Post;
        let json = serde_json::to_string(&method).unwrap();
        let deserialized: HttpMethod = serde_json::from_str(&json).unwrap();
        assert_eq!(format!("{deserialized}"), "POST");
    }

    #[test]
    fn test_request_body_serialization() {
        let body = RequestBody::Text("test".to_string());
        let json = serde_json::to_string(&body).unwrap();
        let deserialized: RequestBody = serde_json::from_str(&json).unwrap();
        match deserialized {
            RequestBody::Text(text) => assert_eq!(text, "test"),
            _ => panic!("Expected Text variant"),
        }
    }

    #[test]
    fn test_request_body_json_serialization() {
        let json_value = serde_json::json!({"key": "value"});
        let body = RequestBody::Json(json_value.clone());
        let json = serde_json::to_string(&body).unwrap();
        let deserialized: RequestBody = serde_json::from_str(&json).unwrap();
        match deserialized {
            RequestBody::Json(value) => assert_eq!(value, json_value),
            _ => panic!("Expected JSON variant"),
        }
    }

    #[test]
    fn test_request_body_form_serialization() {
        let mut form = BTreeMap::new();
        form.insert("key".to_string(), "value".to_string());
        let body = RequestBody::Form(form.clone());
        let json = serde_json::to_string(&body).unwrap();
        let deserialized: RequestBody = serde_json::from_str(&json).unwrap();
        match deserialized {
            RequestBody::Form(data) => assert_eq!(data, form),
            _ => panic!("Expected Form variant"),
        }
    }

    #[test]
    fn test_request_body_binary_serialization() {
        let binary_data = vec![1, 2, 3, 4, 5];
        let body = RequestBody::Binary(binary_data.clone());
        let json = serde_json::to_string(&body).unwrap();
        let deserialized: RequestBody = serde_json::from_str(&json).unwrap();
        match deserialized {
            RequestBody::Binary(data) => assert_eq!(data, binary_data),
            _ => panic!("Expected Binary variant"),
        }
    }

    #[test]
    fn test_http_request_step_serialization() {
        let request = HttpRequestStep::post("/api/data")
            .json(serde_json::json!({"test": true}))
            .expect_status(201);

        let json = serde_json::to_string(&request).unwrap();
        let deserialized: HttpRequestStep = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.url, "/api/data");
        assert_eq!(deserialized.expected_status, Some(201));
    }
}
