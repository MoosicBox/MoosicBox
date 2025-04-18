use std::{collections::BTreeMap, marker::PhantomData};

use async_trait::async_trait;
use bytes::Bytes;

use crate::{
    Error, GenericClient, GenericClientBuilder, GenericRequestBuilder, GenericResponse, Method,
    StatusCode,
};

pub struct Client(reqwest::Client);

impl Client {
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

pub struct ClientBuilder;

impl crate::ReqwestClientBuilder {
    #[must_use]
    pub const fn new() -> Self {
        Self(ClientBuilder, PhantomData, PhantomData)
    }
}

impl GenericClientBuilder<crate::ReqwestRequestBuilder, crate::ReqwestClient> for ClientBuilder {
    fn build(self) -> Result<crate::ReqwestClient, Error> {
        Ok(crate::ClientWrapper(
            Client(reqwest::Client::new()),
            PhantomData,
        ))
    }
}

pub struct RequestBuilder(Option<reqwest::RequestBuilder>);

#[async_trait]
impl GenericRequestBuilder<crate::ReqwestResponse> for RequestBuilder {
    fn header(&mut self, name: &str, value: &str) {
        let builder = self.0.take().unwrap();
        self.0 = Some(builder.header(name, value));
    }

    fn body(&mut self, body: Bytes) {
        let builder = self.0.take().unwrap();
        self.0 = Some(builder.body(body));
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

pub struct Response {
    headers: Option<BTreeMap<String, String>>,
    inner: Option<reqwest::Response>,
}

#[async_trait]
impl GenericResponse for Response {
    #[must_use]
    fn status(&self) -> StatusCode {
        self.inner.as_ref().unwrap().status().into()
    }

    #[must_use]
    fn headers(&mut self) -> &BTreeMap<String, String> {
        if self.headers.is_none() {
            self.headers = Some(headers_to_btree(self.inner.as_ref().unwrap().headers()));
        }

        self.headers.as_ref().unwrap()
    }

    #[must_use]
    async fn text(&mut self) -> Result<String, Error> {
        let response = self.inner.take().unwrap();
        Ok(response.text().await?)
    }

    #[must_use]
    async fn bytes(&mut self) -> Result<Bytes, Error> {
        let response = self.inner.take().unwrap();
        Ok(response.bytes().await?)
    }

    #[must_use]
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
