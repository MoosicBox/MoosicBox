use std::collections::BTreeMap;

#[cfg(feature = "serde")]
use serde::Serialize;

#[cfg(all(feature = "actix", not(feature = "simulator")))]
pub mod actix;
pub mod request_builder;
pub mod response;
#[cfg(any(feature = "simulator", not(feature = "actix")))]
pub mod simulator;

pub use request_builder::TestRequestBuilder;
pub use response::{TestResponse, TestResponseExt};

/// Unified test client abstraction for both Actix and Simulator backends
pub trait TestClient {
    /// Error type for test client operations
    type Error: std::error::Error + Send + Sync + 'static;

    /// Send a GET request to the specified path
    fn get(&self, path: &str) -> TestRequestBuilder<'_, Self>;

    /// Send a POST request to the specified path
    fn post(&self, path: &str) -> TestRequestBuilder<'_, Self>;

    /// Send a PUT request to the specified path
    fn put(&self, path: &str) -> TestRequestBuilder<'_, Self>;

    /// Send a DELETE request to the specified path
    fn delete(&self, path: &str) -> TestRequestBuilder<'_, Self>;

    /// Execute a request with the given method, path, headers, and body
    ///
    /// # Errors
    /// * Returns error if the request cannot be executed
    /// * Returns error if the response cannot be parsed
    fn execute_request(
        &self,
        method: &str,
        path: &str,
        headers: &BTreeMap<String, String>,
        body: Option<&[u8]>,
    ) -> Result<TestResponse, Self::Error>;
}

/// HTTP method enumeration for test requests
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

impl HttpMethod {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Delete => "DELETE",
            Self::Patch => "PATCH",
            Self::Head => "HEAD",
            Self::Options => "OPTIONS",
        }
    }
}

/// Request body types for test requests
#[derive(Debug, Clone)]
pub enum RequestBody {
    /// Raw bytes
    Bytes(Vec<u8>),
    /// JSON serializable data
    #[cfg(feature = "serde")]
    Json(serde_json::Value),
    /// Form data
    Form(BTreeMap<String, String>),
    /// Plain text
    Text(String),
}

impl RequestBody {
    /// Convert the request body to bytes and content type
    ///
    /// # Errors
    /// * Returns error if JSON serialization fails
    #[cfg(feature = "serde")]
    pub fn to_bytes_and_content_type(&self) -> Result<(Vec<u8>, String), serde_json::Error> {
        match self {
            Self::Bytes(bytes) => Ok((bytes.clone(), "application/octet-stream".to_string())),
            Self::Json(value) => {
                let bytes = serde_json::to_vec(value)?;
                Ok((bytes, "application/json".to_string()))
            }
            Self::Form(form) => {
                let encoded = form
                    .iter()
                    .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
                    .collect::<Vec<_>>()
                    .join("&");
                Ok((
                    encoded.into_bytes(),
                    "application/x-www-form-urlencoded".to_string(),
                ))
            }
            Self::Text(text) => Ok((text.as_bytes().to_vec(), "text/plain".to_string())),
        }
    }

    /// Convert the request body to bytes and content type (without serde support)
    ///
    /// # Errors
    /// * This version never returns errors since JSON is not supported
    #[cfg(not(feature = "serde"))]
    pub fn to_bytes_and_content_type(&self) -> Result<(Vec<u8>, String), std::convert::Infallible> {
        match self {
            Self::Bytes(bytes) => Ok((bytes.clone(), "application/octet-stream".to_string())),
            Self::Form(form) => {
                let encoded = form
                    .iter()
                    .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
                    .collect::<Vec<_>>()
                    .join("&");
                Ok((
                    encoded.into_bytes(),
                    "application/x-www-form-urlencoded".to_string(),
                ))
            }
            Self::Text(text) => Ok((text.as_bytes().to_vec(), "text/plain".to_string())),
        }
    }

    /// Create a JSON request body from a serializable value
    ///
    /// # Errors
    /// * Returns error if JSON serialization fails
    #[cfg(feature = "serde")]
    pub fn json<T: Serialize>(value: &T) -> Result<Self, serde_json::Error> {
        let json_value = serde_json::to_value(value)?;
        Ok(Self::Json(json_value))
    }

    /// Create a form request body from key-value pairs
    #[must_use]
    pub fn form<K: Into<String>, V: Into<String>>(data: impl IntoIterator<Item = (K, V)>) -> Self {
        let form = data
            .into_iter()
            .map(|(k, v)| (k.into(), v.into()))
            .collect();
        Self::Form(form)
    }
}
