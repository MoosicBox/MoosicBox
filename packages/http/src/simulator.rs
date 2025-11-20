//! Simulator HTTP client backend implementation.
//!
//! This module provides a no-op HTTP client backend that returns empty responses without making
//! any network requests. It is useful for testing and development environments where you want to
//! avoid real network calls.
//!
//! All requests succeed immediately and return empty/default responses:
//!
//! * Status: 200 OK
//! * Headers: Empty
//! * Body: Empty
//!
//! This module is only available when the `simulator` feature is enabled.
//!
//! # Usage
//!
//! Use the exported types from the parent crate (`switchy_http::Client`, etc.) rather than
//! accessing this module directly. The parent crate automatically selects the appropriate
//! backend based on enabled features.

use std::{collections::BTreeMap, marker::PhantomData};

use async_trait::async_trait;
use bytes::Bytes;

use crate::{
    Error, GenericClient, GenericClientBuilder, GenericRequestBuilder, GenericResponse, Method,
    StatusCode,
};

/// Simulator HTTP client.
///
/// This client provides a no-op implementation that doesn't make any real network requests.
/// All requests succeed immediately and return empty responses with a 200 OK status.
#[derive(Default)]
pub struct Client;

impl Client {
    /// Create a new simulator HTTP client.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl GenericClient<crate::SimulatorRequestBuilder> for Client {
    fn request(&self, _method: Method, _url: &str) -> crate::SimulatorRequestBuilder {
        crate::RequestBuilderWrapper(RequestBuilder, PhantomData)
    }
}

/// Builder for constructing a simulator HTTP client.
///
/// This builder always succeeds when building a client since the simulator
/// requires no configuration or initialization.
pub struct ClientBuilder;

impl crate::SimulatorClientBuilder {
    /// Create a new client builder for the simulator HTTP client.
    #[must_use]
    pub const fn new() -> Self {
        Self(ClientBuilder, PhantomData, PhantomData)
    }
}

impl GenericClientBuilder<crate::SimulatorRequestBuilder, crate::SimulatorClient>
    for ClientBuilder
{
    fn build(self) -> Result<crate::SimulatorClient, Error> {
        Ok(crate::ClientWrapper(Client, PhantomData))
    }
}

/// Request builder for simulator HTTP client.
///
/// This builder ignores all configuration (headers, query parameters, body) and
/// always returns an empty successful response when sent.
pub struct RequestBuilder;

#[async_trait]
impl GenericRequestBuilder<crate::SimulatorResponse> for RequestBuilder {
    fn header(&mut self, _name: &str, _value: &str) {}

    fn query_param(&mut self, _name: &str, _value: &str) {}

    fn query_param_opt(&mut self, _name: &str, _value: Option<&str>) {}

    fn query_params(&mut self, _params: &[(&str, &str)]) {}

    fn body(&mut self, _body: Bytes) {}

    #[cfg(feature = "json")]
    fn form(&mut self, _form: &serde_json::Value) {}

    async fn send(&mut self) -> Result<crate::SimulatorResponse, Error> {
        Ok(crate::ResponseWrapper(Response::default()))
    }
}

/// HTTP response from simulator client.
///
/// This response always returns a 200 OK status with empty headers and body.
#[derive(Default)]
pub struct Response {
    headers: BTreeMap<String, String>,
}

#[async_trait]
impl GenericResponse for Response {
    fn status(&self) -> StatusCode {
        StatusCode::Ok
    }

    fn headers(&mut self) -> &BTreeMap<String, String> {
        &self.headers
    }

    async fn text(&mut self) -> Result<String, Error> {
        Ok(String::new())
    }

    async fn bytes(&mut self) -> Result<Bytes, Error> {
        Ok(Bytes::new())
    }

    #[cfg(feature = "stream")]
    fn bytes_stream(
        &mut self,
    ) -> std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, Error>> + Send>> {
        Box::pin(futures_util::stream::empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_simulator_client_builder_succeeds() {
        let builder = ClientBuilder;
        let result =
            GenericClientBuilder::<crate::SimulatorRequestBuilder, crate::SimulatorClient>::build(
                builder,
            );
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_simulator_response_returns_ok_status() {
        let response = Response::default();
        assert_eq!(response.status(), StatusCode::Ok);
    }

    #[test_log::test]
    fn test_simulator_response_returns_empty_headers() {
        let mut response = Response::default();
        let headers = response.headers();
        assert!(headers.is_empty());
    }

    #[test_log::test]
    fn test_simulator_client_creates_request_builder() {
        let client = Client::new();
        let _builder = client.request(Method::Get, "http://example.com");
        // If we get here without panic, the test passes
    }
}
