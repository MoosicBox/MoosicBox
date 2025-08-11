use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    sync::{Arc, RwLock},
};

use bytes::Bytes;
use moosicbox_web_server_core::WebServer;
use switchy_http_models::Method;

use crate::{RouteHandler, WebServerBuilder};

/// Simulation-specific implementation of HTTP request data
#[derive(Debug, Clone)]
pub struct SimulationRequest {
    pub method: Method,
    pub path: String,
    pub query_string: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<Bytes>,
    pub cookies: BTreeMap<String, String>,
    pub remote_addr: Option<String>,
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
            cookies: BTreeMap::new(),
            remote_addr: None,
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

    #[must_use]
    pub fn with_cookies(mut self, cookies: impl IntoIterator<Item = (String, String)>) -> Self {
        self.cookies.extend(cookies);
        self
    }

    #[must_use]
    pub fn with_cookie(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.cookies.insert(name.into(), value.into());
        self
    }

    #[must_use]
    pub fn with_remote_addr(mut self, addr: impl Into<String>) -> Self {
        self.remote_addr = Some(addr.into());
        self
    }
}

/// Enhanced Stub that can hold simulation data
#[derive(Debug, Clone)]
pub struct SimulationStub {
    pub request: SimulationRequest,
    /// State container for the simulation
    pub state_container: Option<Arc<crate::extractors::state::StateContainer>>,
}

impl SimulationStub {
    #[must_use]
    pub const fn new(request: SimulationRequest) -> Self {
        Self {
            request,
            state_container: None,
        }
    }

    #[must_use]
    pub fn with_state_container(
        mut self,
        container: Arc<crate::extractors::state::StateContainer>,
    ) -> Self {
        self.state_container = Some(container);
        self
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

    #[must_use]
    pub fn cookie(&self, name: &str) -> Option<&str> {
        self.request.cookies.get(name).map(String::as_str)
    }

    #[must_use]
    pub const fn cookies(&self) -> &BTreeMap<String, String> {
        &self.request.cookies
    }

    #[must_use]
    pub fn remote_addr(&self) -> Option<&str> {
        self.request.remote_addr.as_deref()
    }

    /// Get state of type T from the state container
    #[must_use]
    pub fn state<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.state_container
            .as_ref()
            .and_then(|container| container.get::<T>())
    }
}

impl From<SimulationRequest> for SimulationStub {
    fn from(request: SimulationRequest) -> Self {
        Self::new(request)
    }
}

struct SimulatorWebServer {
    scopes: Vec<crate::Scope>,
    #[allow(unused)] // TODO: Remove in 5.1.3 when find_route() is implemented
    routes: BTreeMap<(Method, String), RouteHandler>,
    #[allow(unused)] // TODO: Remove in 5.1.6 when state management methods are implemented
    state: Arc<RwLock<BTreeMap<TypeId, Box<dyn Any + Send + Sync>>>>,
}

impl SimulatorWebServer {
    #[allow(unused)] // TODO: Remove in 5.1.7 when register_scope() calls this method
    pub fn register_route(&mut self, method: Method, path: &str, handler: RouteHandler) {
        self.routes.insert((method, path.to_string()), handler);
    }
}

impl WebServer for SimulatorWebServer {
    fn start(&self) -> Pin<Box<dyn Future<Output = ()>>> {
        let scopes = self.scopes.clone();
        Box::pin(async move {
            log::info!("Simulator web server started with {} scopes", scopes.len());
            for scope in &scopes {
                log::debug!("Scope '{}' has {} routes", scope.path, scope.routes.len());
                for route in &scope.routes {
                    log::debug!("  {:?} {}{}", route.method, scope.path, route.path);
                }
            }
        })
    }

    fn stop(&self) -> Pin<Box<dyn Future<Output = ()>>> {
        Box::pin(async {
            log::info!("Simulator web server stopped");
        })
    }
}

impl WebServerBuilder {
    #[must_use]
    pub fn build_simulator(self) -> Box<dyn WebServer> {
        Box::new(SimulatorWebServer {
            scopes: self.scopes,
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(BTreeMap::new())),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HttpRequest, HttpResponse};

    fn create_test_handler() -> RouteHandler {
        Box::new(|_req: HttpRequest| {
            Box::pin(async move { Ok(HttpResponse::ok().with_body("test response")) })
        })
    }

    #[test]
    fn test_route_registration_stores_handler_correctly() {
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(BTreeMap::new())),
        };

        let handler = create_test_handler();
        server.register_route(Method::Get, "/test", handler);

        assert!(
            server
                .routes
                .contains_key(&(Method::Get, "/test".to_string()))
        );
        assert_eq!(server.routes.len(), 1);
    }

    #[test]
    fn test_multiple_routes_can_be_registered_without_conflict() {
        let mut server = SimulatorWebServer {
            scopes: Vec::new(),
            routes: BTreeMap::new(),
            state: Arc::new(RwLock::new(BTreeMap::new())),
        };

        let handler1 = create_test_handler();
        let handler2 = create_test_handler();
        let handler3 = create_test_handler();

        server.register_route(Method::Get, "/users", handler1);
        server.register_route(Method::Post, "/users", handler2);
        server.register_route(Method::Get, "/posts", handler3);

        assert!(
            server
                .routes
                .contains_key(&(Method::Get, "/users".to_string()))
        );
        assert!(
            server
                .routes
                .contains_key(&(Method::Post, "/users".to_string()))
        );
        assert!(
            server
                .routes
                .contains_key(&(Method::Get, "/posts".to_string()))
        );
        assert_eq!(server.routes.len(), 3);
    }
}
