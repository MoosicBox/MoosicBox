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
