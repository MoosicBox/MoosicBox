//! Reqwest HTTP client backend implementation.
//!
//! This module provides a production-ready HTTP client backend using the `reqwest` crate.
//! It implements the generic HTTP traits defined in the parent module, allowing real network
//! requests to be made.
//!
//! This module is only available when the `reqwest` feature is enabled.
//!
//! # Usage
//!
//! Use the exported types from the parent crate (`switchy_http::Client`, etc.) rather than
//! accessing this module directly. The parent crate automatically selects the appropriate
//! backend based on enabled features.

use std::{collections::BTreeMap, marker::PhantomData, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;

use crate::{
    Error, GenericClient, GenericClientBuilder, GenericRequestBuilder, GenericResponse, Method,
    StatusCode,
};

/// Reqwest HTTP client wrapper.
///
/// This client wraps a `reqwest::Client` to provide the generic HTTP client interface
/// for making real network requests.
pub struct Client(reqwest::Client);

impl Client {
    /// Create a new reqwest HTTP client wrapper.
    #[must_use]
    pub const fn new(client: reqwest::Client) -> Self {
        Self(client)
    }
}

impl GenericClient<crate::ReqwestRequestBuilder> for Client {
    fn request(&self, method: Method, url: &str) -> crate::ReqwestRequestBuilder {
        crate::RequestBuilderWrapper(
            RequestBuilder(Some(self.0.request(method.into(), url))),
            PhantomData,
        )
    }
}

/// Builder for constructing a reqwest HTTP client.
///
/// Options left unset keep `reqwest`'s defaults (no timeouts, no `User-Agent`).
#[derive(Debug, Default)]
pub struct ClientBuilder {
    timeout: Option<Duration>,
    connect_timeout: Option<Duration>,
    user_agent: Option<String>,
}

impl ClientBuilder {
    const fn empty() -> Self {
        Self {
            timeout: None,
            connect_timeout: None,
            user_agent: None,
        }
    }
}

impl crate::ReqwestClientBuilder {
    /// Create a new client builder for configuring the reqwest HTTP client.
    #[must_use]
    pub const fn new() -> Self {
        Self(ClientBuilder::empty(), PhantomData, PhantomData)
    }
}

impl GenericClientBuilder<crate::ReqwestRequestBuilder, crate::ReqwestClient> for ClientBuilder {
    fn timeout(&mut self, timeout: Duration) {
        self.timeout = Some(timeout);
    }

    fn connect_timeout(&mut self, timeout: Duration) {
        self.connect_timeout = Some(timeout);
    }

    fn user_agent(&mut self, user_agent: &str) {
        self.user_agent = Some(user_agent.to_owned());
    }

    fn build(self) -> Result<crate::ReqwestClient, Error> {
        let mut builder = reqwest::Client::builder();
        if let Some(timeout) = self.timeout {
            builder = builder.timeout(timeout);
        }
        if let Some(timeout) = self.connect_timeout {
            builder = builder.connect_timeout(timeout);
        }
        if let Some(user_agent) = self.user_agent {
            builder = builder.user_agent(user_agent);
        }
        Ok(crate::ClientWrapper(Client(builder.build()?), PhantomData))
    }
}

/// Request builder for reqwest HTTP client.
///
/// This builder wraps a `reqwest::RequestBuilder` and provides methods for configuring
/// HTTP requests before sending them.
pub struct RequestBuilder(Option<reqwest::RequestBuilder>);

#[async_trait]
impl GenericRequestBuilder<crate::ReqwestResponse> for RequestBuilder {
    fn header(&mut self, name: &str, value: &str) {
        let builder = self.0.take().unwrap();
        self.0 = Some(builder.header(name, value));
    }

    fn query_param(&mut self, name: &str, value: &str) {
        let builder = self.0.take().unwrap();
        self.0 = Some(builder.query(&[(name, value)]));
    }

    fn query_param_opt(&mut self, name: &str, value: Option<&str>) {
        if let Some(value) = value {
            self.query_param(name, value);
        }
    }

    fn query_params(&mut self, params: &[(&str, &str)]) {
        for (key, value) in params {
            self.query_param(key, value);
        }
    }

    fn body(&mut self, body: Bytes) {
        let builder = self.0.take().unwrap();
        self.0 = Some(builder.body(body));
    }

    fn timeout(&mut self, timeout: Duration) {
        let builder = self.0.take().unwrap();
        self.0 = Some(builder.timeout(timeout));
    }

    #[cfg(feature = "json")]
    fn form(&mut self, form: &serde_json::Value) {
        let builder = self.0.take().unwrap();
        self.0 = Some(builder.form(form));
    }

    async fn send(&mut self) -> Result<crate::ReqwestResponse, Error> {
        let builder = self.0.take().unwrap();
        Ok(crate::ResponseWrapper(Response {
            headers: None,
            inner: Some(builder.send().await?),
        }))
    }
}

/// HTTP response from reqwest client.
///
/// This response wraps a `reqwest::Response` and provides methods for accessing
/// the response status, headers, and body in various formats.
pub struct Response {
    headers: Option<BTreeMap<String, String>>,
    inner: Option<reqwest::Response>,
}

#[async_trait]
impl GenericResponse for Response {
    fn status(&self) -> StatusCode {
        self.inner.as_ref().unwrap().status().into()
    }

    fn headers(&mut self) -> &BTreeMap<String, String> {
        if self.headers.is_none() {
            self.headers = Some(headers_to_btree(self.inner.as_ref().unwrap().headers()));
        }

        self.headers.as_ref().unwrap()
    }

    async fn text(&mut self) -> Result<String, Error> {
        let response = self.inner.take().unwrap();
        Ok(response.text().await?)
    }

    async fn bytes(&mut self) -> Result<Bytes, Error> {
        let response = self.inner.take().unwrap();
        Ok(response.bytes().await?)
    }

    #[cfg(feature = "stream")]
    fn bytes_stream(
        &mut self,
    ) -> std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, Error>> + Send>> {
        use futures_util::TryStreamExt as _;

        let response = self.inner.take().unwrap();
        Box::pin(response.bytes_stream().map_err(Into::into))
    }
}

fn headers_to_btree(value: &reqwest::header::HeaderMap) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();

    for (key, value) in value {
        if let Ok(value) = value.to_str() {
            headers.insert(key.to_string(), value.to_string());
        }
    }

    headers
}

#[cfg(test)]
mod tests {
    use reqwest::header::HeaderValue;

    use super::*;

    #[test_log::test]
    fn test_headers_to_btree_empty() {
        let header_map = reqwest::header::HeaderMap::new();
        let result = headers_to_btree(&header_map);
        assert!(result.is_empty());
    }

    #[test_log::test]
    fn test_headers_to_btree_single_header() {
        let mut header_map = reqwest::header::HeaderMap::new();
        header_map.insert("content-type", "application/json".parse().unwrap());
        let result = headers_to_btree(&header_map);
        assert_eq!(result.len(), 1);
        assert_eq!(
            result.get("content-type"),
            Some(&"application/json".to_string())
        );
    }

    #[test_log::test]
    fn test_headers_to_btree_multiple_headers() {
        let mut header_map = reqwest::header::HeaderMap::new();
        header_map.insert("content-type", "application/json".parse().unwrap());
        header_map.insert("authorization", "Bearer token".parse().unwrap());
        let result = headers_to_btree(&header_map);
        assert_eq!(result.len(), 2);
        assert_eq!(
            result.get("content-type"),
            Some(&"application/json".to_string())
        );
        assert_eq!(
            result.get("authorization"),
            Some(&"Bearer token".to_string())
        );
    }

    #[test_log::test]
    fn test_headers_to_btree_sorted_order() {
        let mut header_map = reqwest::header::HeaderMap::new();
        header_map.insert("zebra", "value1".parse().unwrap());
        header_map.insert("alpha", "value2".parse().unwrap());
        header_map.insert("middle", "value3".parse().unwrap());
        let result = headers_to_btree(&header_map);
        let keys: Vec<&String> = result.keys().collect();
        assert_eq!(keys, vec!["alpha", "middle", "zebra"]);
    }

    #[test_log::test]
    fn test_headers_to_btree_skips_non_utf8_values() {
        let mut header_map = reqwest::header::HeaderMap::new();
        // Add a valid UTF-8 header
        header_map.insert("valid", "utf8-value".parse().unwrap());
        // Add a header with non-UTF8 bytes (to_str() will return Err)
        let non_utf8_value = HeaderValue::from_bytes(&[0x80, 0x81, 0x82]).unwrap();
        header_map.insert("invalid", non_utf8_value);
        // Add another valid header
        header_map.insert("also-valid", "another-value".parse().unwrap());

        let result = headers_to_btree(&header_map);

        // Only the valid UTF-8 headers should be present
        assert_eq!(result.len(), 2);
        assert_eq!(result.get("valid"), Some(&"utf8-value".to_string()));
        assert_eq!(result.get("also-valid"), Some(&"another-value".to_string()));
        // The invalid header should be skipped
        assert!(!result.contains_key("invalid"));
    }

    #[test_log::test]
    fn test_client_new() {
        let reqwest_client = reqwest::Client::new();
        let _client = Client::new(reqwest_client);
        // If we get here without panic, the test passes
    }

    #[test_log::test]
    fn test_client_builder_build() {
        let builder = ClientBuilder::default();
        let result =
            GenericClientBuilder::<crate::ReqwestRequestBuilder, crate::ReqwestClient>::build(
                builder,
            );
        assert!(result.is_ok());
    }

    /// Accepts connections on a local port and never responds.
    fn silent_server() -> (std::net::SocketAddr, std::thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            std::thread::sleep(Duration::from_secs(2));
        });
        (addr, handle)
    }

    #[test_log::test(switchy_async::test(real_time))]
    async fn test_client_timeout_elapses() {
        let (addr, _server) = silent_server();
        let client = crate::ReqwestClient::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();
        let error = client
            .get(&format!("http://{addr}/"))
            .send()
            .await
            .err()
            .expect("request to a silent server should time out");
        assert!(error.is_timeout(), "{error:?}");
    }

    #[test_log::test(switchy_async::test(real_time))]
    async fn test_request_timeout_overrides_client() {
        let (addr, _server) = silent_server();
        let client = crate::ReqwestClient::new();
        let error = client
            .get(&format!("http://{addr}/"))
            .timeout(Duration::from_millis(100))
            .send()
            .await
            .err()
            .expect("request to a silent server should time out");
        assert!(error.is_timeout(), "{error:?}");
    }

    #[test_log::test]
    fn test_decode_error_is_not_timeout() {
        assert!(!Error::Decode.is_timeout());
    }
}
