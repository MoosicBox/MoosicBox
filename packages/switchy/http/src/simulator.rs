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

    /// Test the complete request/response flow through the macro-generated HTTP client.
    /// This exercises the full integration: client creation, request builder configuration,
    /// sending requests, and consuming responses.
    #[test_log::test(switchy_async::test)]
    async fn test_simulator_full_request_response_flow() {
        let client = crate::SimulatorClient::new();

        // Build and send a request with multiple configurations
        let response = client
            .get("http://example.com/test")
            .header("Authorization", "Bearer token")
            .query_param("key", "value")
            .query_param_opt("optional", Some("present"))
            .query_param_opt("missing", None)
            .query_params(&[("page", "1"), ("limit", "10")])
            .send()
            .await
            .unwrap();

        // Verify response properties
        assert_eq!(response.status(), StatusCode::Ok);

        // Verify response body consumption works
        let text = response.text().await.unwrap();
        assert!(text.is_empty());
    }

    /// Test that all HTTP method convenience methods are correctly wired through the macro.
    #[test_log::test(switchy_async::test)]
    async fn test_simulator_all_http_methods() {
        let client = crate::SimulatorClient::new();

        // Test all HTTP method convenience methods work through the macro-generated client
        let get = client.get("http://example.com").send().await.unwrap();
        assert_eq!(get.status(), StatusCode::Ok);

        let post = client.post("http://example.com").send().await.unwrap();
        assert_eq!(post.status(), StatusCode::Ok);

        let put = client.put("http://example.com").send().await.unwrap();
        assert_eq!(put.status(), StatusCode::Ok);

        let patch = client.patch("http://example.com").send().await.unwrap();
        assert_eq!(patch.status(), StatusCode::Ok);

        let delete = client.delete("http://example.com").send().await.unwrap();
        assert_eq!(delete.status(), StatusCode::Ok);

        let head = client.head("http://example.com").send().await.unwrap();
        assert_eq!(head.status(), StatusCode::Ok);

        let options = client.options("http://example.com").send().await.unwrap();
        assert_eq!(options.status(), StatusCode::Ok);
    }

    /// Test that the JSON serialization path works correctly through the macro-generated client.
    /// This exercises the `json()` method which serializes via `serde_json`.
    #[cfg(feature = "json")]
    #[test_log::test(switchy_async::test)]
    async fn test_simulator_json_body_serialization() {
        #[derive(serde::Serialize)]
        struct TestPayload {
            name: String,
            value: i32,
        }

        let client = crate::SimulatorClient::new();

        let payload = TestPayload {
            name: "test".to_string(),
            value: 42,
        };

        // Verify the JSON serialization path doesn't panic
        let response = client
            .post("http://example.com/api")
            .json(&payload)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::Ok);
    }

    /// Test that the form serialization path works correctly through the macro-generated client.
    /// This exercises the `form()` method which serializes via `serde_json::to_value`.
    #[cfg(feature = "json")]
    #[test_log::test(switchy_async::test)]
    async fn test_simulator_form_body_serialization() {
        #[derive(serde::Serialize)]
        struct FormData {
            username: String,
            password: String,
        }

        let client = crate::SimulatorClient::new();

        let form = FormData {
            username: "user".to_string(),
            password: "pass".to_string(),
        };

        // Verify the form serialization path doesn't panic
        let response = client
            .post("http://example.com/login")
            .form(&form)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::Ok);
    }

    /// Test the `bytes_stream` response method works correctly through the macro-generated response.
    #[cfg(feature = "stream")]
    #[test_log::test(switchy_async::test)]
    async fn test_simulator_bytes_stream_consumption() {
        use futures_util::StreamExt;

        let client = crate::SimulatorClient::new();
        let response = client.get("http://example.com").send().await.unwrap();

        // Test consuming the response as a stream
        let mut stream = response.bytes_stream();
        let chunks: Vec<_> = stream.by_ref().collect().await;
        assert!(chunks.is_empty());
    }

    /// Test that `Response::bytes()` returns empty bytes for simulator.
    /// This verifies the `bytes()` code path in `GenericResponse` impl is working.
    #[test_log::test(switchy_async::test)]
    async fn test_simulator_response_bytes() {
        use crate::GenericResponse;

        let mut response = Response::default();
        let bytes = response.bytes().await.unwrap();
        assert!(bytes.is_empty());
    }

    /// Test that `Response::text()` returns empty string for simulator.
    /// This verifies the `text()` code path in `GenericResponse` impl is working.
    #[test_log::test(switchy_async::test)]
    async fn test_simulator_response_text() {
        use crate::GenericResponse;

        let mut response = Response::default();
        let text = response.text().await.unwrap();
        assert!(text.is_empty());
    }

    /// Test that the underlying `RequestBuilder::body()` method accepts raw bytes.
    /// This exercises the body path separately from the JSON serialization path.
    #[test_log::test(switchy_async::test)]
    async fn test_simulator_request_raw_body() {
        let client = crate::SimulatorClient::new();

        let body_bytes = Bytes::from_static(b"raw request body content");

        let response = client
            .post("http://example.com/upload")
            .body(body_bytes)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::Ok);
    }

    /// Test that `SimulatorClient::default()` produces a working client.
    /// This exercises the Default impl generated by the macro.
    #[test_log::test(switchy_async::test)]
    async fn test_simulator_client_default() {
        let client = crate::SimulatorClient::default();

        let response = client.get("http://example.com").send().await.unwrap();
        assert_eq!(response.status(), StatusCode::Ok);
    }

    /// Test that headers can be retrieved through the macro-generated wrapper method.
    #[test_log::test(switchy_async::test)]
    async fn test_simulator_response_headers_through_wrapper() {
        let client = crate::SimulatorClient::new();

        let mut response = client.get("http://example.com").send().await.unwrap();

        // Access headers through the wrapper's headers() method
        let headers = response.headers();
        assert!(headers.is_empty());
    }

    /// Test the `bytes_stream` on the `GenericResponse` trait impl directly.
    #[cfg(feature = "stream")]
    #[test_log::test(switchy_async::test)]
    async fn test_simulator_response_bytes_stream_trait() {
        use crate::GenericResponse;
        use futures_util::StreamExt;

        let mut response = Response::default();

        let mut stream = response.bytes_stream();
        let chunks: Vec<_> = stream.by_ref().collect().await;
        assert!(chunks.is_empty());
    }

    /// Test the `Client::request()` method with various HTTP methods.
    /// This verifies the method parameter is correctly passed through.
    #[test_log::test(switchy_async::test)]
    async fn test_simulator_client_request_with_different_methods() {
        let client = Client::new();

        // Test that request() works with different method types
        let methods = [
            Method::Get,
            Method::Post,
            Method::Put,
            Method::Patch,
            Method::Delete,
            Method::Head,
            Method::Options,
        ];

        for method in methods {
            let _builder = client.request(method, "http://example.com");
            // If we reach here without panic, the test passes
        }
    }

    /// Test JSON deserialization of response body.
    /// This exercises the `json()` method on the Response wrapper.
    #[cfg(feature = "json")]
    #[test_log::test(switchy_async::test)]
    async fn test_simulator_response_json_deserialization_empty() {
        let client = crate::SimulatorClient::new();

        let response = client.get("http://example.com/api").send().await.unwrap();

        // The simulator returns empty bytes, so deserializing any non-empty type will fail.
        // But deserializing to an empty structure or Option should work with appropriate JSON.
        // Since simulator returns empty bytes (not valid JSON), this should fail.
        let result: Result<serde_json::Value, _> = response.json().await;
        assert!(result.is_err());
    }
}
