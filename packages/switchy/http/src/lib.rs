//! Generic HTTP client abstraction layer.
//!
//! This crate provides a unified interface for making HTTP requests across different backend
//! implementations. It defines generic traits for HTTP clients, request builders, and responses,
//! allowing you to write code that works with multiple HTTP client libraries.
//!
//! # Features
//!
//! * `reqwest` - Enable the reqwest HTTP client backend (real network requests)
//! * `simulator` - Enable the simulator backend (no-op client for testing)
//! * `json` - Enable JSON serialization/deserialization support
//! * `stream` - Enable streaming response bodies
//!
//! # Backends
//!
//! The crate supports multiple HTTP client backends through feature flags:
//!
//! * **reqwest** - Production HTTP client using the `reqwest` crate
//! * **simulator** - No-op client that returns empty responses, useful for testing
//!
//! # Examples
//!
//! Basic usage with the reqwest backend:
//!
//! ```rust,no_run
//! # #[cfg(feature = "reqwest")]
//! # {
//! use switchy_http::{GenericClient, GenericRequestBuilder, GenericResponse};
//!
//! # async fn example() -> Result<(), switchy_http::Error> {
//! let client = switchy_http::Client::new();
//! let mut response = client.get("https://api.example.com/data").send().await?;
//! let text = response.text().await?;
//! # Ok(())
//! # }
//! # }
//! ```
//!
//! With JSON support:
//!
//! ```rust,no_run
//! # #[cfg(all(feature = "reqwest", feature = "json"))]
//! # {
//! use switchy_http::{GenericClient, GenericRequestBuilder};
//! use serde::Deserialize;
//!
//! #[derive(Deserialize)]
//! struct ApiResponse {
//!     message: String,
//! }
//!
//! # async fn example() -> Result<(), switchy_http::Error> {
//! let client = switchy_http::Client::new();
//! let response = client.get("https://api.example.com/data").send().await?;
//! let data: ApiResponse = response.json().await?;
//! # Ok(())
//! # }
//! # }
//! ```
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, marker::PhantomData};

use async_trait::async_trait;
use bytes::Bytes;
use strum::{AsRefStr, EnumString};
use switchy_http_models::{Method, StatusCode};
use thiserror::Error;

/// Re-exported HTTP models and types from `switchy_http_models`.
///
/// This module provides common HTTP types including [`models::Method`] and
/// [`models::StatusCode`] that work across different HTTP libraries.
///
/// See the [`switchy_http_models`] crate documentation for full details.
pub use switchy_http_models as models;

#[cfg(feature = "reqwest")]
pub mod reqwest;

#[cfg(feature = "simulator")]
pub mod simulator;

/// Errors that can occur when making HTTP requests.
#[derive(Debug, Error)]
pub enum Error {
    /// Failed to decode response data.
    #[error("Decode")]
    Decode,

    /// JSON deserialization error (requires `json` feature).
    #[cfg(feature = "json")]
    #[error(transparent)]
    Deserialize(#[from] serde_json::Error),

    /// Reqwest HTTP client error (requires `reqwest` feature).
    #[cfg(feature = "reqwest")]
    #[error(transparent)]
    Reqwest(#[from] ::reqwest::Error),
}

/// Common HTTP header names.
#[derive(Debug, Clone, Copy, EnumString, AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum Header {
    /// HTTP `Authorization` header.
    Authorization,
    /// HTTP `User-Agent` header.
    UserAgent,
    /// HTTP `Range` header.
    Range,
    /// HTTP `Content-Length` header.
    ContentLength,
}

/// Generic trait for building and configuring HTTP requests.
#[async_trait]
pub trait GenericRequestBuilder<R>: Send + Sync {
    /// Add a header to the request.
    fn header(&mut self, name: &str, value: &str);
    /// Add a query parameter to the request.
    fn query_param(&mut self, name: &str, value: &str);
    /// Add an optional query parameter to the request.
    fn query_param_opt(&mut self, name: &str, value: Option<&str>);
    /// Add multiple query parameters to the request.
    fn query_params(&mut self, params: &[(&str, &str)]);
    /// Set the request body.
    #[allow(unused)]
    fn body(&mut self, body: Bytes);
    /// Set the request body as a form (requires `json` feature).
    #[cfg(feature = "json")]
    fn form(&mut self, form: &serde_json::Value);
    /// Send the HTTP request.
    ///
    /// # Errors
    ///
    /// * If the request fails to send
    async fn send(&mut self) -> Result<R, Error>;
}

/// Generic trait for building HTTP clients.
pub trait GenericClientBuilder<RB, C: GenericClient<RB>>: Send + Sync {
    /// Build the HTTP client.
    ///
    /// # Errors
    ///
    /// * If the `Client` fails to build
    fn build(self) -> Result<C, Error>;
}

/// Generic trait for HTTP clients.
pub trait GenericClient<RB>: Send + Sync {
    /// Create a GET request builder.
    fn get(&self, url: &str) -> RB {
        self.request(Method::Get, url)
    }

    /// Create a POST request builder.
    fn post(&self, url: &str) -> RB {
        self.request(Method::Post, url)
    }

    /// Create a PUT request builder.
    fn put(&self, url: &str) -> RB {
        self.request(Method::Put, url)
    }

    /// Create a PATCH request builder.
    fn patch(&self, url: &str) -> RB {
        self.request(Method::Patch, url)
    }

    /// Create a DELETE request builder.
    fn delete(&self, url: &str) -> RB {
        self.request(Method::Delete, url)
    }

    /// Create a HEAD request builder.
    fn head(&self, url: &str) -> RB {
        self.request(Method::Head, url)
    }

    /// Create an OPTIONS request builder.
    fn options(&self, url: &str) -> RB {
        self.request(Method::Options, url)
    }

    /// Create a request builder with the specified HTTP method.
    fn request(&self, method: Method, url: &str) -> RB;
}

/// Generic trait for HTTP responses.
#[async_trait]
pub trait GenericResponse: Send + Sync {
    /// Get the HTTP status code of the response.
    fn status(&self) -> StatusCode;
    /// Get the response headers.
    fn headers(&mut self) -> &BTreeMap<String, String>;
    /// Get the response body as text.
    ///
    /// # Errors
    ///
    /// * If the response body cannot be decoded as text
    async fn text(&mut self) -> Result<String, Error>;
    /// Get the response body as bytes.
    ///
    /// # Errors
    ///
    /// * If the response body cannot be read
    async fn bytes(&mut self) -> Result<Bytes, Error>;
    /// Get the response body as a stream of bytes (requires `stream` feature).
    #[cfg(feature = "stream")]
    fn bytes_stream(
        &mut self,
    ) -> std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, Error>> + Send>>;
}

/// Wrapper type for generic request builders.
///
/// This wrapper provides a unified interface for building HTTP requests across different
/// backend implementations. It implements builder pattern methods for configuring requests.
///
/// Most users should use the type aliases exported by this crate (`ReqwestRequestBuilder`,
/// `SimulatorRequestBuilder`, etc.) rather than using this wrapper directly.
pub struct RequestBuilderWrapper<R, B: GenericRequestBuilder<R>>(
    pub(crate) B,
    pub(crate) PhantomData<R>,
);

/// Wrapper type for generic HTTP clients.
///
/// This wrapper provides a unified interface for creating HTTP requests across different
/// backend implementations. It provides convenience methods for common HTTP methods like
/// GET, POST, PUT, etc.
///
/// Most users should use the type aliases exported by this crate (`ReqwestClient`,
/// `SimulatorClient`, etc.) rather than using this wrapper directly.
pub struct ClientWrapper<RB, T: GenericClient<RB>>(pub(crate) T, pub(crate) PhantomData<RB>);

/// Wrapper type for generic client builders.
///
/// This wrapper provides a unified interface for building HTTP clients across different
/// backend implementations. Use the `build()` method to construct a configured client.
///
/// Most users should use the type aliases exported by this crate (`ReqwestClientBuilder`,
/// `SimulatorClientBuilder`, etc.) rather than using this wrapper directly.
pub struct ClientBuilderWrapper<RB, C: GenericClient<RB>, T: GenericClientBuilder<RB, C>>(
    pub(crate) T,
    PhantomData<RB>,
    PhantomData<C>,
);

/// Wrapper type for generic HTTP responses.
///
/// This wrapper provides a unified interface for accessing HTTP response data across different
/// backend implementations. It provides methods for reading response status, headers, and body
/// in various formats (text, bytes, JSON, streams).
///
/// Most users should use the type aliases exported by this crate (`ReqwestResponse`,
/// `SimulatorResponse`, etc.) rather than using this wrapper directly.
pub struct ResponseWrapper<T: GenericResponse>(pub(crate) T);

#[allow(unused)]
macro_rules! impl_http {
    ($module:ident, $local_module:ident $(,)?) => {
        paste::paste! {
            pub use [< impl_ $module >]::*;
        }

        mod $local_module {
            use crate::*;

            paste::paste! {
                pub type [< $module:camel Response >] = ResponseWrapper<$module::Response>;
                type ModuleResponse = [< $module:camel Response >];

                pub type [< $module:camel RequestBuilder >] = RequestBuilderWrapper<ModuleResponse, $module::RequestBuilder>;
                type ModuleRequestBuilder = [< $module:camel RequestBuilder >];

                pub type [< $module:camel Client >] = ClientWrapper<ModuleRequestBuilder, $module::Client>;
                type ModuleClient = [< $module:camel Client >];

                pub type [< $module:camel ClientBuilder >] = ClientBuilderWrapper<ModuleRequestBuilder, ModuleClient, $module::ClientBuilder>;
                type ModuleClientBuilder = [< $module:camel ClientBuilder >];
            }

            impl ModuleRequestBuilder {
                /// Add a header to the request.
                #[must_use]
                pub fn header(mut self, name: &str, value: &str) -> Self {
                    self.0.header(name, value);
                    self
                }

                /// Add a query parameter to the request.
                #[must_use]
                pub fn query_param(mut self, name: &str, value: &str) -> Self {
                    self.0.query_param(name, value);
                    self
                }

                /// Add an optional query parameter to the request.
                #[must_use]
                pub fn query_param_opt(mut self, name: &str, value: Option<&str>) -> Self {
                    self.0.query_param_opt(name, value);
                    self
                }

                /// Add multiple query parameters to the request.
                #[must_use]
                pub fn query_params(mut self, params: &[(&str, &str)]) -> Self {
                    self.0.query_params(params);
                    self
                }

                /// Set the request body.
                #[must_use]
                pub fn body(mut self, body: Bytes) -> Self {
                    self.0.body(body);
                    self
                }

                /// Send the HTTP request.
                ///
                /// # Errors
                ///
                /// * If there was an error while sending request, redirect loop was
                ///   detected or redirect limit was exhausted.
                pub async fn send(mut self) -> Result<ModuleResponse, Error> {
                    self.0.send().await
                }
            }

            #[async_trait]
            impl GenericRequestBuilder<ModuleResponse> for ModuleRequestBuilder {
                fn header(&mut self, name: &str, value: &str) {
                    self.0.header(name, value);
                }

                fn query_param(&mut self, name: &str, value: &str) {
                    self.0.query_param(name, value);
                }

                fn query_param_opt(&mut self, name: &str, value: Option<&str>) {
                    self.0.query_param_opt(name, value);
                }

                fn query_params(&mut self, params: &[(&str, &str)]) {
                    self.0.query_params(params);
                }

                fn body(&mut self, body: Bytes) {
                    self.0.body(body);
                }

                #[cfg(feature = "json")]
                fn form(&mut self, form: &serde_json::Value) {
                    self.0.form(form);
                }

                async fn send(&mut self) -> Result<ModuleResponse, Error> {
                    self.0.send().await
                }
            }

            #[cfg(feature = "json")]
            impl ModuleRequestBuilder {
                /// Set the request body as JSON.
                ///
                /// # Panics
                ///
                /// * If the `serde_json` serialization to bytes fails
                #[must_use]
                pub fn json<T: serde::Serialize + ?Sized>(mut self, body: &T) -> Self {
                    let mut bytes: Vec<u8> = Vec::new();
                    serde_json::to_writer(&mut bytes, body).unwrap();
                    <Self as GenericRequestBuilder<ModuleResponse>>::body(&mut self, bytes.into());
                    self
                }

                /// Set the request body as form data.
                ///
                /// # Panics
                ///
                /// * If the `serde_json` serialization to bytes fails
                #[must_use]
                pub fn form<T: serde::Serialize + ?Sized>(mut self, form: &T) -> Self {
                    let value = serde_json::to_value(form).unwrap();
                    <Self as GenericRequestBuilder<ModuleResponse>>::form(&mut self, &value);
                    self
                }
            }

            #[async_trait]
            impl GenericResponse for ModuleResponse {
                fn status(&self) -> StatusCode {
                    self.0.status()
                }

                fn headers(&mut self) -> &BTreeMap<String, String> {
                    self.0.headers()
                }

                async fn text(&mut self) -> Result<String, Error> {
                    self.0.text().await
                }

                async fn bytes(&mut self) -> Result<Bytes, Error> {
                    self.0.bytes().await
                }

                #[cfg(feature = "stream")]
                fn bytes_stream(
                    &mut self,
                ) -> std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, Error>> + Send>>
                {
                    self.0.bytes_stream()
                }
            }

            impl ModuleResponse {
                /// Get the HTTP status code of the response.
                #[must_use]
                pub fn status(&self) -> StatusCode {
                    <Self as GenericResponse>::status(self)
                }

                /// Get the response headers.
                #[must_use]
                pub fn headers(&mut self) -> &BTreeMap<String, String> {
                    <Self as GenericResponse>::headers(self)
                }

                /// Get the response body as text.
                ///
                /// # Errors
                ///
                /// * If the text response fails
                pub async fn text(mut self) -> Result<String, Error> {
                    <Self as GenericResponse>::text(&mut self).await
                }

                /// Get the response body as bytes.
                ///
                /// # Errors
                ///
                /// * If the bytes response fails
                pub async fn bytes(mut self) -> Result<Bytes, Error> {
                    <Self as GenericResponse>::bytes(&mut self).await
                }
            }

            impl GenericClientBuilder<ModuleRequestBuilder, ModuleClient> for ModuleClientBuilder {
                fn build(self) -> Result<ModuleClient, Error> {
                    self.0.build()
                }
            }

            impl ModuleClientBuilder {
                /// Build the configured HTTP client.
                ///
                /// # Errors
                ///
                /// * If the `Client` fails to build
                pub fn build(self) -> Result<ModuleClient, Error> {
                    <Self as GenericClientBuilder<ModuleRequestBuilder, ModuleClient>>::build(self)
                }
            }

            impl ModuleResponse {
                /// Get the response body as a stream of bytes.
                ///
                /// # Errors
                ///
                /// * If the `bytes_stream` response fails
                #[cfg(feature = "stream")]
                pub fn bytes_stream(
                    mut self,
                ) -> impl futures_core::Stream<Item = Result<Bytes, Error>> {
                    <Self as GenericResponse>::bytes_stream(&mut self)
                }
            }

            impl ModuleResponse {
                /// Deserialize the response body as JSON.
                ///
                /// # Errors
                ///
                /// * If the json response fails
                #[cfg(feature = "json")]
                pub async fn json<T: serde::de::DeserializeOwned>(mut self) -> Result<T, Error> {
                    let bytes = <Self as GenericResponse>::bytes(&mut self).await?;
                    Ok(serde_json::from_slice(&bytes)?)
                }
            }

            impl Default for ModuleClient {
                fn default() -> Self {
                    Self::new()
                }
            }

            impl ModuleClient {
                /// Create a new HTTP client with default configuration.
                ///
                /// # Panics
                ///
                /// * If the empty `ClientBuilder` somehow fails to build
                #[must_use]
                pub fn new() -> Self {
                    Self::builder().0.build().unwrap()
                }

                /// Create a new client builder for configuring the HTTP client.
                #[must_use]
                pub const fn builder() -> ModuleClientBuilder {
                    ModuleClientBuilder::new()
                }

                /// Create a GET request builder for the specified URL.
                #[must_use]
                pub fn get(&self, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::get(self, url)
                }

                /// Create a POST request builder for the specified URL.
                #[must_use]
                pub fn post(&self, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::post(self, url)
                }

                /// Create a PUT request builder for the specified URL.
                #[must_use]
                pub fn put(&self, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::put(self, url)
                }

                /// Create a PATCH request builder for the specified URL.
                #[must_use]
                pub fn patch(&self, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::patch(self, url)
                }

                /// Create a DELETE request builder for the specified URL.
                #[must_use]
                pub fn delete(&self, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::delete(self, url)
                }

                /// Create a HEAD request builder for the specified URL.
                #[must_use]
                pub fn head(&self, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::head(self, url)
                }

                /// Create an OPTIONS request builder for the specified URL.
                #[must_use]
                pub fn options(&self, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::options(self, url)
                }

                /// Create a request builder with the specified HTTP method and URL.
                #[must_use]
                pub fn request(&self, method: Method, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::request(self, method, url)
                }
            }

            impl Default for ModuleClientBuilder {
                fn default() -> Self {
                    Self::new()
                }
            }

            impl GenericClient<ModuleRequestBuilder> for ModuleClient {
                fn request(&self, method: Method, url: &str) -> ModuleRequestBuilder {
                    self.0.request(method, url)
                }
            }
        }
    };
}

#[cfg(feature = "simulator")]
impl_http!(simulator, impl_simulator);

#[cfg(feature = "reqwest")]
impl_http!(reqwest, impl_reqwest);

#[allow(unused)]
macro_rules! impl_gen_types {
    ($module:ident $(,)?) => {
        paste::paste! {
            pub type RequestBuilder = [< $module:camel RequestBuilder >];
            pub type Client = [< $module:camel Client >];
            pub type Response = [< $module:camel Response >];
        }
    };
}

#[cfg(feature = "simulator")]
impl_gen_types!(simulator);

#[cfg(all(not(feature = "simulator"), feature = "reqwest"))]
impl_gen_types!(reqwest);

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_header_as_ref_str() {
        assert_eq!(Header::Authorization.as_ref(), "authorization");
        assert_eq!(Header::UserAgent.as_ref(), "user-agent");
        assert_eq!(Header::Range.as_ref(), "range");
        assert_eq!(Header::ContentLength.as_ref(), "content-length");
    }

    #[test_log::test]
    fn test_error_decode_display() {
        let error = Error::Decode;
        assert_eq!(error.to_string(), "Decode");
    }

    #[cfg(feature = "json")]
    #[test_log::test]
    fn test_error_deserialize_display() {
        let json_error = serde_json::from_str::<serde_json::Value>("invalid json");
        assert!(json_error.is_err());
        let error = Error::from(json_error.unwrap_err());
        assert!(error.to_string().contains("expected"));
    }
}
