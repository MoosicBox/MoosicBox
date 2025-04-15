#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, marker::PhantomData, num::NonZeroU16};

use async_trait::async_trait;
use bytes::Bytes;
use moosicbox_http_models::Method;
use strum::{AsRefStr, EnumString};
use thiserror::Error;

pub use moosicbox_http_models as models;

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

#[async_trait]
pub trait GenericRequestBuilder<R>: Send + Sync {
    fn header(&mut self, name: &str, value: &str);
    #[allow(unused)]
    fn body(&mut self, body: Bytes);
    #[cfg(feature = "json")]
    fn form(&mut self, form: &serde_json::Value);
    async fn send(&mut self) -> Result<R, Error>;
}

pub trait GenericClientBuilder<RB, C: GenericClient<RB>>: Send + Sync {
    /// # Errors
    ///
    /// * If the `Client` fails to build
    fn build(self) -> Result<C, Error>;
}

pub trait GenericClient<RB>: Send + Sync {
    fn get(&self, url: &str) -> RB {
        self.request(Method::Get, url)
    }

    fn post(&self, url: &str) -> RB {
        self.request(Method::Post, url)
    }

    fn put(&self, url: &str) -> RB {
        self.request(Method::Put, url)
    }

    fn patch(&self, url: &str) -> RB {
        self.request(Method::Patch, url)
    }

    fn delete(&self, url: &str) -> RB {
        self.request(Method::Delete, url)
    }

    fn head(&self, url: &str) -> RB {
        self.request(Method::Head, url)
    }

    fn options(&self, url: &str) -> RB {
        self.request(Method::Options, url)
    }

    fn request(&self, method: Method, url: &str) -> RB;
}

#[async_trait]
pub trait GenericResponse: Send + Sync {
    fn status(&self) -> StatusCode;
    fn headers(&mut self) -> &BTreeMap<String, String>;
    async fn text(&mut self) -> Result<String, Error>;
    async fn bytes(&mut self) -> Result<Bytes, Error>;
    #[cfg(feature = "stream")]
    fn bytes_stream(
        &mut self,
    ) -> std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, Error>> + Send>>;
}

pub struct RequestBuilderWrapper<R, B: GenericRequestBuilder<R>>(
    pub(crate) B,
    pub(crate) PhantomData<R>,
);
pub struct ClientWrapper<RB, T: GenericClient<RB>>(pub(crate) T, pub(crate) PhantomData<RB>);
pub struct ClientBuilderWrapper<RB, C: GenericClient<RB>, T: GenericClientBuilder<RB, C>>(
    pub(crate) T,
    PhantomData<RB>,
    PhantomData<C>,
);
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
                #[must_use]
                pub fn header(mut self, name: &str, value: &str) -> Self {
                    self.0.header(name, value);
                    self
                }

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
                #[must_use]
                fn status(&self) -> StatusCode {
                    self.0.status()
                }

                #[must_use]
                fn headers(&mut self) -> &BTreeMap<String, String> {
                    self.0.headers()
                }

                #[must_use]
                async fn text(&mut self) -> Result<String, Error> {
                    self.0.text().await
                }

                #[must_use]
                async fn bytes(&mut self) -> Result<Bytes, Error> {
                    self.0.bytes().await
                }

                #[must_use]
                #[cfg(feature = "stream")]
                fn bytes_stream(
                    &mut self,
                ) -> std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, Error>> + Send>>
                {
                    self.0.bytes_stream()
                }
            }

            impl ModuleResponse {
                #[must_use]
                pub fn status(&self) -> StatusCode {
                    <Self as GenericResponse>::status(self)
                }

                #[must_use]
                pub fn headers(&mut self) -> &BTreeMap<String, String> {
                    <Self as GenericResponse>::headers(self)
                }

                /// # Errors
                ///
                /// * If the text response fails
                pub async fn text(mut self) -> Result<String, Error> {
                    <Self as GenericResponse>::text(&mut self).await
                }

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
                /// # Errors
                ///
                /// * If the `Client` fails to build
                pub fn build(self) -> Result<ModuleClient, Error> {
                    <Self as GenericClientBuilder<ModuleRequestBuilder, ModuleClient>>::build(self)
                }
            }

            impl ModuleResponse {
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
                /// # Panics
                ///
                /// * If the empty `ClientBuilder` somehow fails to build
                #[must_use]
                pub fn new() -> Self {
                    Self::builder().0.build().unwrap()
                }

                #[must_use]
                pub const fn builder() -> ModuleClientBuilder {
                    ModuleClientBuilder::new()
                }

                #[must_use]
                pub fn get(&self, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::get(self, url)
                }

                #[must_use]
                pub fn post(&self, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::post(self, url)
                }

                #[must_use]
                pub fn put(&self, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::put(self, url)
                }

                #[must_use]
                pub fn patch(&self, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::patch(self, url)
                }

                #[must_use]
                pub fn delete(&self, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::delete(self, url)
                }

                #[must_use]
                pub fn head(&self, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::head(self, url)
                }

                #[must_use]
                pub fn options(&self, url: &str) -> ModuleRequestBuilder {
                    <Self as GenericClient<ModuleRequestBuilder>>::options(self, url)
                }

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
