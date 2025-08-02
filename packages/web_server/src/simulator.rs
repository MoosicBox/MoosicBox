use std::{collections::BTreeMap, pin::Pin};

use bytes::Bytes;
use moosicbox_web_server_core::WebServer;
use switchy_http_models::Method;

use crate::WebServerBuilder;

/// Simulation-specific implementation of HTTP request data
#[derive(Debug, Clone)]
pub struct SimulationRequest {
    pub method: Method,
    pub path: String,
    pub query_string: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Bytes>,
}

impl SimulationRequest {
    #[must_use]
    pub fn new(method: Method, path: impl Into<String>) -> Self {
        Self {
            method,
            path: path.into(),
            query_string: String::new(),
            headers: BTreeMap::new(),
            body: None,
        }
    }

    #[must_use]
    pub fn with_query_string(mut self, query: impl Into<String>) -> Self {
        self.query_string = query.into();
        self
    }

    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    #[must_use]
    pub fn with_body(mut self, body: impl Into<Bytes>) -> Self {
        self.body = Some(body.into());
        self
    }
}

/// Enhanced Stub that can hold simulation data
#[derive(Debug, Clone)]
pub struct SimulationStub {
    pub request: SimulationRequest,
}

impl SimulationStub {
    #[must_use]
    pub const fn new(request: SimulationRequest) -> Self {
        Self { request }
    }

    #[must_use]
    pub fn header(&self, name: &str) -> Option<&str> {
        self.request.headers.get(name).map(String::as_str)
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.request.path
    }

    #[must_use]
    pub fn query_string(&self) -> &str {
        &self.request.query_string
    }

    #[must_use]
    pub const fn method(&self) -> &Method {
        &self.request.method
    }

    #[must_use]
    pub const fn body(&self) -> Option<&Bytes> {
        self.request.body.as_ref()
    }
}

struct SimulatorWebServer {}

impl WebServer for SimulatorWebServer {
    fn start(&self) -> Pin<Box<dyn Future<Output = ()>>> {
        Box::pin(async {})
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = ()>>> {
        Box::pin(async {})
    }
}

impl WebServerBuilder {
    #[must_use]
    pub fn build_simulator(self) -> Box<dyn WebServer> {
        Box::new(SimulatorWebServer {})
    }
}
