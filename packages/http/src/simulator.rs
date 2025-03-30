use std::{collections::BTreeMap, num::NonZeroU16};

use async_trait::async_trait;
use bytes::Bytes;

use crate::{
    Error, IClient, IRequestBuilder, IResponse, Method, RequestBuilder, Response, StatusCode,
};

#[derive(Default)]
pub struct SimulatorClient;

impl SimulatorClient {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl IClient for SimulatorClient {
    fn request(&self, _method: Method, _url: &str) -> RequestBuilder {
        RequestBuilder {
            builder: Box::new(SimulatorRequestBuilder),
        }
    }
}

pub struct SimulatorRequestBuilder;

#[async_trait]
impl IRequestBuilder for SimulatorRequestBuilder {
    fn header(&mut self, _name: &str, _value: &str) {}

    fn body(&mut self, _body: Bytes) {}

    #[cfg(feature = "json")]
    fn form(&mut self, _form: &serde_json::Value) {}

    async fn send(&mut self) -> Result<Response, Error> {
        Ok(Response {
            inner: Box::new(SimulatorResponse::default()),
        })
    }
}

#[derive(Default)]
pub struct SimulatorResponse {
    headers: BTreeMap<String, String>,
}

#[async_trait]
impl IResponse for SimulatorResponse {
    #[must_use]
    fn status(&self) -> StatusCode {
        StatusCode(NonZeroU16::new(200).unwrap())
    }

    #[must_use]
    fn headers(&mut self) -> &BTreeMap<String, String> {
        &self.headers
    }

    #[must_use]
    async fn text(&mut self) -> Result<String, Error> {
        Ok(String::new())
    }

    #[must_use]
    async fn bytes(&mut self) -> Result<Bytes, Error> {
        Ok(Bytes::new())
    }

    #[cfg(feature = "stream")]
    #[must_use]
    fn bytes_stream(
        &mut self,
    ) -> std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, Error>> + Send>> {
        Box::pin(futures_util::stream::empty())
    }
}
