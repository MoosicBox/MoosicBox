use std::{collections::BTreeMap, marker::PhantomData};

use async_trait::async_trait;
use bytes::Bytes;

use crate::{
    Error, GenericClient, GenericClientBuilder, GenericRequestBuilder, GenericResponse, Method,
    StatusCode,
};

#[derive(Default)]
pub struct Client;

impl Client {
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

pub struct ClientBuilder;

impl crate::SimulatorClientBuilder {
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
