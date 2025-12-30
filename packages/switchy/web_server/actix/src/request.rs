//! Actix-specific HTTP request implementation.
//!
//! This module provides the [`ActixRequest`] type, which extracts data from an
//! `actix_web::HttpRequest` and stores it in a `Send + Sync` compatible format.
//! This is necessary because Actix's native request type uses `Rc` internally
//! and cannot be shared across threads.

use std::{any::TypeId, collections::BTreeMap};

use bytes::Bytes;
use switchy_http_models::Method;
use switchy_web_server::{
    PathParams,
    request::{ErasedState, HttpRequestTrait},
};

/// Actix-specific HTTP request implementation.
///
/// This struct extracts and stores data from an `actix_web::HttpRequest` in a
/// `Send + Sync` compatible way. The original Actix request cannot be stored
/// directly because it uses `Rc` internally and is not thread-safe.
#[derive(Clone, Debug)]
pub struct ActixRequest {
    path: String,
    query_string: String,
    method: Method,
    headers: BTreeMap<String, String>,
    cookies: BTreeMap<String, String>,
    remote_addr: Option<String>,
    path_params: PathParams,
    body: Option<Bytes>,
}

impl ActixRequest {
    /// Creates a new `ActixRequest` by extracting data from an Actix web request.
    #[must_use]
    pub fn new(inner: &actix_web::HttpRequest) -> Self {
        use actix_web::http::Method as ActixMethod;

        let method = match *inner.method() {
            ActixMethod::GET => Method::Get,
            ActixMethod::POST => Method::Post,
            ActixMethod::PUT => Method::Put,
            ActixMethod::PATCH => Method::Patch,
            ActixMethod::DELETE => Method::Delete,
            ActixMethod::HEAD => Method::Head,
            ActixMethod::OPTIONS => Method::Options,
            ActixMethod::CONNECT => Method::Connect,
            _ => Method::Trace,
        };

        let headers = inner
            .headers()
            .iter()
            .filter_map(|(k, v)| {
                v.to_str()
                    .ok()
                    .map(|val| (k.as_str().to_string(), val.to_string()))
            })
            .collect();

        let cookies = inner
            .cookies()
            .map(|jar| {
                jar.iter()
                    .map(|c| (c.name().to_string(), c.value().to_string()))
                    .collect()
            })
            .unwrap_or_default();

        let remote_addr = inner
            .connection_info()
            .peer_addr()
            .map(std::string::ToString::to_string);

        Self {
            path: inner.path().to_string(),
            query_string: inner.query_string().to_string(),
            method,
            headers,
            cookies,
            remote_addr,
            path_params: PathParams::new(),
            body: None,
        }
    }

    /// Creates a new `ActixRequest` with path parameters.
    #[must_use]
    pub fn with_path_params(mut self, params: PathParams) -> Self {
        self.path_params = params;
        self
    }

    /// Creates a new `ActixRequest` with a body.
    #[must_use]
    pub fn with_body(mut self, body: Bytes) -> Self {
        self.body = Some(body);
        self
    }
}

impl HttpRequestTrait for ActixRequest {
    fn path(&self) -> &str {
        &self.path
    }

    fn query_string(&self) -> &str {
        &self.query_string
    }

    fn method(&self) -> Method {
        self.method
    }

    fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }

    fn headers(&self) -> BTreeMap<String, String> {
        self.headers.clone()
    }

    fn body(&self) -> Option<&Bytes> {
        self.body.as_ref()
    }

    fn cookie(&self, name: &str) -> Option<String> {
        self.cookies.get(name).cloned()
    }

    fn cookies(&self) -> BTreeMap<String, String> {
        self.cookies.clone()
    }

    fn remote_addr(&self) -> Option<String> {
        self.remote_addr.clone()
    }

    fn path_params(&self) -> &PathParams {
        &self.path_params
    }

    fn app_state_any(&self, _type_id: TypeId) -> Option<ErasedState> {
        // State access needs to be handled differently for the sub-crate
        // since we don't have access to the original actix request's app_data
        // This is a limitation of the current architecture
        None
    }
}

impl From<&actix_web::HttpRequest> for ActixRequest {
    fn from(inner: &actix_web::HttpRequest) -> Self {
        Self::new(inner)
    }
}

#[cfg(test)]
mod tests {
    // Tests would require a running actix test server
    // which is better suited for integration tests
}
