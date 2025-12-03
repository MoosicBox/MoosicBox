//! Fluent request builder for constructing test HTTP requests.
//!
//! This module provides [`TestRequestBuilder`], a builder pattern implementation for
//! constructing HTTP requests in tests. It supports setting headers, body content,
//! query parameters, and various authentication methods.
//!
//! # Overview
//!
//! The request builder is typically obtained from a `TestClient` method like `get()`,
//! `post()`, etc. It provides a fluent API for configuring all aspects of an HTTP request
//! before sending it.
//!
//! # Examples
//!
//! ```rust,ignore
//! use moosicbox_web_server::test_client::{ConcreteTestClient, TestClient};
//!
//! let client = ConcreteTestClient::new_with_test_routes();
//!
//! // Simple GET request
//! let response = client.get("/api/users").send().unwrap();
//!
//! // POST with JSON body and headers
//! let response = client
//!     .post("/api/users")
//!     .header("X-Custom", "value")
//!     .json(&serde_json::json!({"name": "John"}))
//!     .send()
//!     .unwrap();
//!
//! // GET with query parameters
//! let response = client
//!     .get("/api/search")
//!     .query([("q", "rust"), ("limit", "10")])
//!     .send()
//!     .unwrap();
//! ```

use std::collections::BTreeMap;

use base64::Engine;
#[cfg(feature = "serde")]
use serde::Serialize;

use super::{HttpMethod, RequestBody, TestClient, TestResponse};

/// Fluent request builder for test clients
pub struct TestRequestBuilder<'a, T: TestClient + ?Sized> {
    client: &'a T,
    method: HttpMethod,
    path: String,
    headers: BTreeMap<String, String>,
    body: Option<RequestBody>,
}

impl<'a, T: TestClient + ?Sized> TestRequestBuilder<'a, T> {
    /// Create a new request builder
    #[must_use]
    pub const fn new(client: &'a T, method: HttpMethod, path: String) -> Self {
        Self {
            client,
            method,
            path,
            headers: BTreeMap::new(),
            body: None,
        }
    }

    /// Add a header to the request
    #[must_use]
    pub fn header<K: Into<String>, V: Into<String>>(mut self, key: K, value: V) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    /// Add multiple headers to the request
    #[must_use]
    pub fn headers<K: Into<String>, V: Into<String>>(
        mut self,
        headers: impl IntoIterator<Item = (K, V)>,
    ) -> Self {
        for (key, value) in headers {
            self.headers.insert(key.into(), value.into());
        }
        self
    }

    /// Set the Content-Type header
    #[must_use]
    pub fn content_type(self, content_type: &str) -> Self {
        self.header("content-type", content_type)
    }

    /// Set the Authorization header
    #[must_use]
    pub fn authorization(self, auth: &str) -> Self {
        self.header("authorization", auth)
    }

    /// Set the Authorization header with Bearer token
    #[must_use]
    pub fn bearer_token(self, token: &str) -> Self {
        self.header("authorization", format!("Bearer {token}"))
    }

    /// Set the User-Agent header
    #[must_use]
    pub fn user_agent(self, user_agent: &str) -> Self {
        self.header("user-agent", user_agent)
    }

    /// Set a raw byte body
    #[must_use]
    pub fn body_bytes(mut self, body: Vec<u8>) -> Self {
        self.body = Some(RequestBody::Bytes(body));
        self
    }

    /// Set a JSON body from a serializable value
    ///
    /// # Panics
    /// * Panics if JSON serialization fails
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn json<S: Serialize>(mut self, value: &S) -> Self {
        self.body = Some(RequestBody::json(value).expect("Failed to serialize JSON"));
        self
    }

    /// Set a form body from key-value pairs
    #[must_use]
    pub fn form<K: Into<String>, V: Into<String>>(
        mut self,
        data: impl IntoIterator<Item = (K, V)>,
    ) -> Self {
        self.body = Some(RequestBody::form(data));
        self
    }

    /// Set a plain text body
    #[must_use]
    pub fn text(mut self, text: String) -> Self {
        self.body = Some(RequestBody::Text(text));
        self
    }

    /// Execute the request and return the response
    ///
    /// # Errors
    /// * Returns error if the request execution fails
    ///
    /// # Panics
    /// * Panics if request body serialization fails
    pub fn send(mut self) -> Result<TestResponse, T::Error> {
        // Set content-type based on body if not already set
        if let Some(ref body) = self.body
            && !self.headers.contains_key("content-type")
        {
            let (_, content_type) = body
                .to_bytes_and_content_type()
                .expect("Failed to serialize request body");
            self.headers
                .insert("content-type".to_string(), content_type);
        }

        // Convert body to bytes
        let body_bytes = self.body.as_ref().map(|body| {
            let (bytes, _) = body
                .to_bytes_and_content_type()
                .expect("Failed to serialize request body");
            bytes
        });

        self.client.execute_request(
            self.method.as_str(),
            &self.path,
            &self.headers,
            body_bytes.as_deref(),
        )
    }
}

/// Convenience methods for common request patterns
impl<T: TestClient + ?Sized> TestRequestBuilder<'_, T> {
    /// Create a GET request with query parameters
    #[must_use]
    pub fn query<K: Into<String>, V: Into<String>>(
        mut self,
        params: impl IntoIterator<Item = (K, V)>,
    ) -> Self {
        let query_string = params
            .into_iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    urlencoding::encode(&k.into()),
                    urlencoding::encode(&v.into())
                )
            })
            .collect::<Vec<_>>()
            .join("&");

        if !query_string.is_empty() {
            if self.path.contains('?') {
                self.path.push('&');
            } else {
                self.path.push('?');
            }
            self.path.push_str(&query_string);
        }

        self
    }

    /// Create a JSON POST request
    #[cfg(feature = "serde")]
    #[must_use]
    pub fn json_post<S: Serialize>(self, value: &S) -> Self {
        self.content_type("application/json").json(value)
    }

    /// Create a form POST request
    #[must_use]
    pub fn form_post<K: Into<String>, V: Into<String>>(
        self,
        data: impl IntoIterator<Item = (K, V)>,
    ) -> Self {
        self.content_type("application/x-www-form-urlencoded")
            .form(data)
    }

    /// Create a request with basic authentication
    #[must_use]
    pub fn basic_auth(self, username: &str, password: Option<&str>) -> Self {
        let credentials =
            password.map_or_else(|| username.to_string(), |pwd| format!("{username}:{pwd}"));
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        self.authorization(&format!("Basic {encoded}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulator::SimulatorWebServer;
    use crate::test_client::simulator_impl::SimulatorTestClient;

    fn create_test_client() -> SimulatorTestClient {
        use std::sync::Arc;
        SimulatorTestClient::new(SimulatorWebServer {
            scopes: Vec::new(),
            routes: std::collections::BTreeMap::new(),
            state: Arc::new(std::sync::RwLock::new(
                crate::extractors::state::StateContainer::new(),
            )),
        })
    }

    #[test_log::test]
    fn test_query_appends_to_path_without_existing_query() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Get, "/api/test".to_string())
            .query([("page", "1"), ("limit", "10")]);

        // The path should now have a query string
        assert!(builder.path.contains('?'));
        assert!(builder.path.contains("page=1"));
        assert!(builder.path.contains("limit=10"));
    }

    #[test_log::test]
    fn test_query_appends_to_path_with_existing_query() {
        let client = create_test_client();
        let builder =
            TestRequestBuilder::new(&client, HttpMethod::Get, "/api/test?sort=desc".to_string())
                .query([("page", "1")]);

        // Should append with & instead of ?
        assert!(builder.path.contains("?sort=desc"));
        assert!(builder.path.contains("&page=1"));
        assert!(!builder.path.contains("?page=1"));
    }

    #[test_log::test]
    fn test_query_encodes_special_characters() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Get, "/api/test".to_string())
            .query([("search", "hello world"), ("email", "test@example.com")]);

        // Special characters should be URL encoded
        assert!(builder.path.contains("search=hello%20world"));
        assert!(builder.path.contains("email=test%40example.com"));
    }

    #[test_log::test]
    fn test_query_with_empty_params_does_not_modify_path() {
        let client = create_test_client();
        let empty_params: Vec<(&str, &str)> = vec![];
        let builder = TestRequestBuilder::new(&client, HttpMethod::Get, "/api/test".to_string())
            .query(empty_params);

        // Path should remain unchanged
        assert_eq!(builder.path, "/api/test");
    }

    #[test_log::test]
    fn test_basic_auth_with_password() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Get, "/api/test".to_string())
            .basic_auth("username", Some("password"));

        // Should set authorization header with Base64 encoded credentials
        let auth_header = builder.headers.get("authorization").unwrap();
        assert!(auth_header.starts_with("Basic "));

        // Decode and verify the credentials
        let encoded_part = auth_header.strip_prefix("Basic ").unwrap();
        let decoded = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(encoded_part)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(decoded, "username:password");
    }

    #[test_log::test]
    fn test_basic_auth_without_password() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Get, "/api/test".to_string())
            .basic_auth("username", None);

        // Should set authorization header with just username
        let auth_header = builder.headers.get("authorization").unwrap();
        let encoded_part = auth_header.strip_prefix("Basic ").unwrap();
        let decoded = String::from_utf8(
            base64::engine::general_purpose::STANDARD
                .decode(encoded_part)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(decoded, "username");
    }

    #[test_log::test]
    fn test_bearer_token() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Get, "/api/test".to_string())
            .bearer_token("my_jwt_token_12345");

        let auth_header = builder.headers.get("authorization").unwrap();
        assert_eq!(auth_header, "Bearer my_jwt_token_12345");
    }

    #[test_log::test]
    fn test_content_type() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Post, "/api/test".to_string())
            .content_type("application/json");

        assert_eq!(
            builder.headers.get("content-type"),
            Some(&"application/json".to_string())
        );
    }

    #[test_log::test]
    fn test_authorization() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Get, "/api/test".to_string())
            .authorization("Custom auth scheme");

        assert_eq!(
            builder.headers.get("authorization"),
            Some(&"Custom auth scheme".to_string())
        );
    }

    #[test_log::test]
    fn test_user_agent() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Get, "/api/test".to_string())
            .user_agent("TestClient/1.0");

        assert_eq!(
            builder.headers.get("user-agent"),
            Some(&"TestClient/1.0".to_string())
        );
    }

    #[test_log::test]
    fn test_multiple_headers() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Get, "/api/test".to_string())
            .headers([
                ("X-Custom-1", "value1"),
                ("X-Custom-2", "value2"),
                ("Accept-Language", "en-US"),
            ]);

        assert_eq!(
            builder.headers.get("X-Custom-1"),
            Some(&"value1".to_string())
        );
        assert_eq!(
            builder.headers.get("X-Custom-2"),
            Some(&"value2".to_string())
        );
        assert_eq!(
            builder.headers.get("Accept-Language"),
            Some(&"en-US".to_string())
        );
    }

    #[test_log::test]
    fn test_single_header() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Get, "/api/test".to_string())
            .header("X-Request-Id", "req-12345");

        assert_eq!(
            builder.headers.get("X-Request-Id"),
            Some(&"req-12345".to_string())
        );
    }

    #[test_log::test]
    fn test_body_bytes() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Post, "/api/test".to_string())
            .body_bytes(vec![1, 2, 3, 4, 5]);

        assert!(builder.body.is_some());
        match builder.body.unwrap() {
            RequestBody::Bytes(bytes) => {
                assert_eq!(bytes, vec![1, 2, 3, 4, 5]);
            }
            _ => panic!("Expected Bytes body"),
        }
    }

    #[test_log::test]
    fn test_text_body() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Post, "/api/test".to_string())
            .text("Hello, World!".to_string());

        assert!(builder.body.is_some());
        match builder.body.unwrap() {
            RequestBody::Text(text) => {
                assert_eq!(text, "Hello, World!");
            }
            _ => panic!("Expected Text body"),
        }
    }

    #[test_log::test]
    fn test_form_body() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Post, "/api/test".to_string())
            .form([("username", "john"), ("password", "secret")]);

        assert!(builder.body.is_some());
        match builder.body.unwrap() {
            RequestBody::Form(form) => {
                assert_eq!(form.get("username"), Some(&"john".to_string()));
                assert_eq!(form.get("password"), Some(&"secret".to_string()));
            }
            _ => panic!("Expected Form body"),
        }
    }

    #[test_log::test]
    fn test_form_post_convenience() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Post, "/api/test".to_string())
            .form_post([("field", "value")]);

        // Should set content type
        assert_eq!(
            builder.headers.get("content-type"),
            Some(&"application/x-www-form-urlencoded".to_string())
        );

        // And have form body
        assert!(builder.body.is_some());
    }

    #[test_log::test]
    #[cfg(feature = "serde")]
    fn test_json_body() {
        let client = create_test_client();
        let data = serde_json::json!({"name": "test", "value": 123});
        let builder =
            TestRequestBuilder::new(&client, HttpMethod::Post, "/api/test".to_string()).json(&data);

        assert!(builder.body.is_some());
        match builder.body.unwrap() {
            RequestBody::Json(json_value) => {
                assert_eq!(json_value["name"], "test");
                assert_eq!(json_value["value"], 123);
            }
            _ => panic!("Expected Json body"),
        }
    }

    #[test_log::test]
    #[cfg(feature = "serde")]
    fn test_json_post_convenience() {
        let client = create_test_client();
        let data = serde_json::json!({"key": "value"});
        let builder = TestRequestBuilder::new(&client, HttpMethod::Post, "/api/test".to_string())
            .json_post(&data);

        // Should set content type
        assert_eq!(
            builder.headers.get("content-type"),
            Some(&"application/json".to_string())
        );

        // And have JSON body
        assert!(builder.body.is_some());
    }

    #[test_log::test]
    fn test_builder_chaining() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Get, "/api/test".to_string())
            .header("X-Custom", "value")
            .bearer_token("token123")
            .user_agent("TestAgent/1.0")
            .query([("page", "1")]);

        assert_eq!(builder.headers.get("X-Custom"), Some(&"value".to_string()));
        assert_eq!(
            builder.headers.get("authorization"),
            Some(&"Bearer token123".to_string())
        );
        assert_eq!(
            builder.headers.get("user-agent"),
            Some(&"TestAgent/1.0".to_string())
        );
        assert!(builder.path.contains("page=1"));
    }

    #[test_log::test]
    fn test_method_preserved() {
        let client = create_test_client();

        let get_builder =
            TestRequestBuilder::new(&client, HttpMethod::Get, "/api/test".to_string());
        assert_eq!(get_builder.method, HttpMethod::Get);

        let post_builder =
            TestRequestBuilder::new(&client, HttpMethod::Post, "/api/test".to_string());
        assert_eq!(post_builder.method, HttpMethod::Post);

        let put_builder =
            TestRequestBuilder::new(&client, HttpMethod::Put, "/api/test".to_string());
        assert_eq!(put_builder.method, HttpMethod::Put);

        let delete_builder =
            TestRequestBuilder::new(&client, HttpMethod::Delete, "/api/test".to_string());
        assert_eq!(delete_builder.method, HttpMethod::Delete);
    }

    #[test_log::test]
    fn test_query_multiple_chained_calls() {
        let client = create_test_client();
        let builder = TestRequestBuilder::new(&client, HttpMethod::Get, "/api/test".to_string())
            .query([("page", "1")])
            .query([("limit", "10")])
            .query([("sort", "name")]);

        // All query params should be present, each appended with &
        assert!(builder.path.contains("page=1"));
        assert!(builder.path.contains("limit=10"));
        assert!(builder.path.contains("sort=name"));

        // Should only have one ?
        assert_eq!(builder.path.matches('?').count(), 1);
    }
}
