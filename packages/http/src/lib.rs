#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, num::NonZeroU16};

use async_trait::async_trait;
use bytes::Bytes;
use strum::{AsRefStr, EnumString};
use thiserror::Error;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "reqwest")]
pub mod reqwest;

#[cfg(feature = "simulator")]
pub mod simulator;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Decode")]
    Decode,

    #[cfg(feature = "json")]
    #[error(transparent)]
    Deserialize(#[from] serde_json::Error),

    #[cfg(feature = "reqwest")]
    #[error(transparent)]
    Reqwest(#[from] ::reqwest::Error),
}

#[derive(Debug, Clone, Copy, EnumString, AsRefStr)]
#[strum(serialize_all = "kebab-case")]
pub enum Header {
    Authorization,
    UserAgent,
    Range,
    ContentLength,
}

#[derive(Debug, Clone, Copy, EnumString, AsRefStr)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "SCREAMING_SNAKE_CASE"))]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Connect,
    Trace,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StatusCode(NonZeroU16);

impl StatusCode {
    #[must_use]
    pub const fn as_u16(&self) -> u16 {
        self.0.get()
    }
}

impl StatusCode {
    pub const OK: Self = Self(NonZeroU16::new(200).unwrap());
    pub const PARTIAL_CONTENT: Self = Self(NonZeroU16::new(206).unwrap());
    pub const UNAUTHORIZED: Self = Self(NonZeroU16::new(401).unwrap());
    pub const NOT_FOUND: Self = Self(NonZeroU16::new(404).unwrap());
}

impl StatusCode {
    /// Check if status is within 100-199.
    #[inline]
    #[must_use]
    pub fn is_informational(&self) -> bool {
        (100..200).contains(&self.0.get())
    }

    /// Check if status is within 200-299.
    #[inline]
    #[must_use]
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.0.get())
    }

    /// Check if status is within 300-399.
    #[inline]
    #[must_use]
    pub fn is_redirection(&self) -> bool {
        (300..400).contains(&self.0.get())
    }

    /// Check if status is within 400-499.
    #[inline]
    #[must_use]
    pub fn is_client_error(&self) -> bool {
        (400..500).contains(&self.0.get())
    }

    /// Check if status is within 500-599.
    #[inline]
    #[must_use]
    pub fn is_server_error(&self) -> bool {
        (500..600).contains(&self.0.get())
    }
}

impl From<StatusCode> for u16 {
    fn from(value: StatusCode) -> Self {
        value.0.get()
    }
}

impl std::fmt::Display for StatusCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.get() {
            401 => f.write_str("401 Unauthorized"),
            code => f.write_str(&code.to_string()),
        }
    }
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

pub trait IClient: Send + Sync {
    fn get(&self, url: &str) -> RequestBuilder {
        self.request(Method::Get, url)
    }

    fn post(&self, url: &str) -> RequestBuilder {
        self.request(Method::Post, url)
    }

    fn put(&self, url: &str) -> RequestBuilder {
        self.request(Method::Put, url)
    }

    fn patch(&self, url: &str) -> RequestBuilder {
        self.request(Method::Patch, url)
    }

    fn delete(&self, url: &str) -> RequestBuilder {
        self.request(Method::Delete, url)
    }

    fn head(&self, url: &str) -> RequestBuilder {
        self.request(Method::Head, url)
    }

    fn options(&self, url: &str) -> RequestBuilder {
        self.request(Method::Options, url)
    }

    fn request(&self, method: Method, url: &str) -> RequestBuilder;
}

pub struct RequestBuilder {
    builder: Box<dyn IRequestBuilder>,
}

#[async_trait]
trait IRequestBuilder: Send + Sync {
    fn header(&mut self, name: &str, value: &str);
    #[allow(unused)]
    fn body(&mut self, body: Bytes);
    #[cfg(feature = "json")]
    fn form(&mut self, form: &serde_json::Value);
    async fn send(&mut self) -> Result<Response, Error>;
}

impl RequestBuilder {
    #[must_use]
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.builder.header(name, value);
        self
    }

    /// # Errors
    ///
    /// * If there was an error while sending request, redirect loop was
    ///   detected or redirect limit was exhausted.
    pub async fn send(mut self) -> Result<Response, Error> {
        self.builder.send().await
    }
}

#[async_trait]
impl IRequestBuilder for RequestBuilder {
    fn header(&mut self, name: &str, value: &str) {
        self.builder.header(name, value);
    }

    fn body(&mut self, body: Bytes) {
        self.builder.body(body);
    }

    #[cfg(feature = "json")]
    fn form(&mut self, form: &serde_json::Value) {
        self.builder.form(form);
    }

    async fn send(&mut self) -> Result<Response, Error> {
        self.builder.send().await
    }
}

#[cfg(feature = "json")]
impl RequestBuilder {
    /// # Panics
    ///
    /// * If the `serde_json` serialization to bytes fails
    #[must_use]
    pub fn json<T: serde::Serialize + ?Sized>(mut self, body: &T) -> Self {
        let mut bytes: Vec<u8> = Vec::new();
        serde_json::to_writer(&mut bytes, body).unwrap();
        <Self as IRequestBuilder>::body(&mut self, bytes.into());
        self
    }

    /// # Panics
    ///
    /// * If the `serde_json` serialization to bytes fails
    #[must_use]
    pub fn form<T: serde::Serialize + ?Sized>(mut self, form: &T) -> Self {
        let value = serde_json::to_value(form).unwrap();
        <Self as IRequestBuilder>::form(&mut self, &value);
        self
    }
}

pub struct Response {
    inner: Box<dyn IResponse>,
}

#[async_trait]
pub trait IResponse: Send + Sync {
    fn status(&self) -> StatusCode;
    fn headers(&mut self) -> &BTreeMap<String, String>;
    async fn text(&mut self) -> Result<String, Error>;
    async fn bytes(&mut self) -> Result<Bytes, Error>;
    #[cfg(feature = "stream")]
    fn bytes_stream(
        &mut self,
    ) -> std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, Error>> + Send>>;
}

#[async_trait]
impl IResponse for Response {
    #[must_use]
    fn status(&self) -> StatusCode {
        self.inner.status()
    }

    #[must_use]
    fn headers(&mut self) -> &BTreeMap<String, String> {
        self.inner.headers()
    }

    #[must_use]
    async fn text(&mut self) -> Result<String, Error> {
        self.inner.text().await
    }

    #[must_use]
    async fn bytes(&mut self) -> Result<Bytes, Error> {
        self.inner.bytes().await
    }

    #[cfg(feature = "stream")]
    #[must_use]
    fn bytes_stream(
        &mut self,
    ) -> std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, Error>> + Send>> {
        self.inner.bytes_stream()
    }
}

impl Response {
    #[must_use]
    pub fn status(&self) -> StatusCode {
        <Self as IResponse>::status(self)
    }

    #[must_use]
    pub fn headers(&mut self) -> &BTreeMap<String, String> {
        <Self as IResponse>::headers(self)
    }

    /// # Errors
    ///
    /// * If the text response fails
    pub async fn text(mut self) -> Result<String, Error> {
        <Self as IResponse>::text(&mut self).await
    }

    /// # Errors
    ///
    /// * If the bytes response fails
    pub async fn bytes(mut self) -> Result<Bytes, Error> {
        <Self as IResponse>::bytes(&mut self).await
    }
}

#[cfg(feature = "stream")]
impl Response {
    /// # Errors
    ///
    /// * If the `bytes_stream` response fails
    pub fn bytes_stream(mut self) -> impl futures_core::Stream<Item = Result<Bytes, Error>> {
        <Self as IResponse>::bytes_stream(&mut self)
    }
}

#[cfg(feature = "json")]
impl Response {
    /// # Errors
    ///
    /// * If the json response fails
    pub async fn json<T: serde::de::DeserializeOwned>(mut self) -> Result<T, Error> {
        let bytes = <Self as IResponse>::bytes(&mut self).await?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

pub struct Client {
    client: Box<dyn IClient>,
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

impl Client {
    /// # Panics
    ///
    /// * If the empty `ClientBuilder` somehow fails to build
    #[must_use]
    pub fn new() -> Self {
        Self::builder().build().unwrap()
    }

    #[must_use]
    pub const fn builder() -> ClientBuilder {
        ClientBuilder {}
    }
}

impl IClient for Client {
    fn request(&self, method: Method, url: &str) -> RequestBuilder {
        self.client.request(method, url)
    }
}

pub struct ClientBuilder {}

impl ClientBuilder {
    /// # Errors
    ///
    /// * If a TLS backend cannot be initialized, or the resolver cannot load the system configuration.
    ///
    /// # Panics
    ///
    /// * If no HTTP backend features are enabled
    pub fn build(self) -> Result<Client, Error> {
        if cfg!(feature = "simulator") {
            #[cfg(feature = "simulator")]
            {
                Ok(Client {
                    client: Box::new(simulator::SimulatorClient),
                })
            }
            #[cfg(not(feature = "simulator"))]
            unreachable!()
        } else if cfg!(feature = "reqwest") {
            #[cfg(feature = "reqwest")]
            {
                self.build_reqwest()
            }
            #[cfg(not(feature = "reqwest"))]
            unreachable!()
        } else {
            panic!("No HTTP backend feature enabled");
        }
    }

    /// # Errors
    ///
    /// * If a TLS backend cannot be initialized, or the resolver cannot load the system configuration.
    #[cfg(feature = "reqwest")]
    #[allow(unreachable_code)]
    pub fn build_reqwest(self) -> Result<Client, Error> {
        #[cfg(feature = "simulator")]
        return Ok(Client {
            client: Box::new(simulator::SimulatorClient),
        });

        let builder = ::reqwest::Client::builder();
        let client = builder.build()?;

        Ok(Client {
            client: Box::new(reqwest::ReqwestClient::new(client)),
        })
    }
}
