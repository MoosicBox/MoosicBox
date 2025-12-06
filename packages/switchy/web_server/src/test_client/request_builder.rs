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
//! use switchy_web_server::test_client::{ConcreteTestClient, TestClient};
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
