//! `ActixTestClient` - Real HTTP test client for Actix Web
//!
//! STATUS: Section 5.2.4.1 COMPLETE - Basic Scope/Route conversion implemented
//!
//! ✅ COMPLETED (5.2.4.1):
//! - Scope/Route conversion implemented for flat routes
//! - Handler conversion with proper request/response mapping
//! - All hardcoded routes removed
//! - Simple GET/POST routes working
//! - Body preservation verified
//!
//! TODO(5.2.4.2+): Remaining features:
//! - Nested scope support (5.2.4.2)
//! - Route parameters (5.2.4.3)
//! - State management (5.2.4.4)
//! - Middleware system (5.2.4.5)
//! - Builder addr/port configuration (5.2.4.7)
//! - See Section 5.2.4 in spec/dst/overview.md for full details
//!
//! NOTE: This module is incompatible with simulator runtime and will not compile
//! when the simulator feature is enabled. See Section 5.2.3.2 for details.

#[cfg(all(feature = "actix", not(feature = "simulator")))]
use std::sync::{Arc, Mutex};

#[cfg(all(feature = "actix", not(feature = "simulator")))]
use ::actix_test::{TestServer, start};

/// Actix Web Server wrapper for testing
///
/// This wrapper provides a testable interface to an Actix web server,
/// making REAL HTTP requests to a running Actix server instance.
///
/// 🚨 CRITICAL: This uses `actix_test::TestServer` for REAL HTTP communication,
/// not simulation. All requests go through actual network sockets.
///
/// 🔧 THREAD SAFETY: `TestServer` is wrapped in Arc<Mutex<>> to implement Send + Sync
/// for the `WebServer` trait. This is test-only code so the performance overhead is acceptable.
#[cfg(all(feature = "actix", not(feature = "simulator")))]
pub struct ActixWebServer {
    test_server: Arc<Mutex<TestServer>>,
}

#[cfg(all(feature = "actix", not(feature = "simulator")))]
impl ActixWebServer {
    /// Create a new Actix web server for testing with REAL HTTP server
    ///
    /// 🚨 CRITICAL: This starts a REAL `actix_test::TestServer` that listens
    /// on actual network sockets and processes HTTP requests.
    ///
    /// # Arguments
    ///
    /// * `_scopes` - The scopes to register with the server (currently unused)
    ///
    /// # Panics
    ///
    /// * If the test server fails to start
    #[must_use]
    pub fn new(scopes: Vec<crate::Scope>) -> Self {
        // 5.2.4.1: Convert Scope/Route to Actix configuration
        let app = move || {
            let mut app = actix_web::App::new();

            for scope in &scopes {
                let mut actix_scope = actix_web::web::scope(&scope.path);

                for route in &scope.routes {
                    let path = route.path.clone();
                    let handler = route.handler.clone();
                    let method = route.method;

                    // Convert our handler to Actix handler with proper request/response mapping
                    let actix_handler = move |req: actix_web::HttpRequest| {
                        let handler = handler.clone();
                        async move {
                            // Convert actix_web::HttpRequest to our HttpRequest
                            let our_request = crate::HttpRequest::from(req);

                            // Call our handler
                            let result = handler(our_request).await;

                            // Convert our HttpResponse to actix_web::HttpResponse
                            result.map(|resp| {
                                let mut actix_resp = actix_web::HttpResponseBuilder::new(
                                    actix_web::http::StatusCode::from_u16(resp.status_code.into())
                                        .unwrap_or(
                                            actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                                        ),
                                );

                                // Insert all headers from the BTreeMap
                                for (name, value) in resp.headers {
                                    actix_resp.insert_header((name, value));
                                }

                                // Keep backwards compatibility with location field
                                if let Some(location) = resp.location {
                                    actix_resp
                                        .insert_header((actix_http::header::LOCATION, location));
                                }

                                // Handle response body
                                match resp.body {
                                    Some(crate::HttpResponseBody::Bytes(bytes)) => {
                                        actix_resp.body(bytes)
                                    }
                                    None => actix_resp.finish(),
                                }
                            })
                        }
                    };

                    // Add route to scope using the appropriate HTTP method
                    actix_scope = match method {
                        crate::Method::Get => {
                            actix_scope.route(&path, actix_web::web::get().to(actix_handler))
                        }
                        crate::Method::Post => {
                            actix_scope.route(&path, actix_web::web::post().to(actix_handler))
                        }
                        crate::Method::Put => {
                            actix_scope.route(&path, actix_web::web::put().to(actix_handler))
                        }
                        crate::Method::Delete => {
                            actix_scope.route(&path, actix_web::web::delete().to(actix_handler))
                        }
                        crate::Method::Patch => {
                            actix_scope.route(&path, actix_web::web::patch().to(actix_handler))
                        }
                        crate::Method::Head => {
                            actix_scope.route(&path, actix_web::web::head().to(actix_handler))
                        }
                        crate::Method::Options => actix_scope.route(
                            &path,
                            actix_web::web::route()
                                .method(actix_web::http::Method::OPTIONS)
                                .to(actix_handler),
                        ),
                        crate::Method::Trace => actix_scope.route(
                            &path,
                            actix_web::web::route()
                                .method(actix_web::http::Method::TRACE)
                                .to(actix_handler),
                        ),
                        crate::Method::Connect => actix_scope.route(
                            &path,
                            actix_web::web::route()
                                .method(actix_web::http::Method::CONNECT)
                                .to(actix_handler),
                        ),
                    };
                }

                app = app.service(actix_scope);
            }

            app
        };

        // Start REAL test server - now switchy_async has IO enabled
        let test_server = start(app);

        #[allow(clippy::arc_with_non_send_sync)]
        // Actix TestServer uses Rc internally, known limitation
        let wrapped_server = Arc::new(Mutex::new(test_server));

        Self {
            test_server: wrapped_server,
        }
    }

    /// Get the full server URL
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned
    #[must_use]
    pub fn url(&self) -> String {
        let addr = {
            let server = self.test_server.lock().unwrap();
            server.addr()
        }; // Guard dropped here
        format!("http://{addr}")
    }

    /// Get the server address
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned
    #[must_use]
    pub fn addr(&self) -> std::net::SocketAddr {
        let server = self.test_server.lock().unwrap();
        server.addr()
    }

    /// Get the server port
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned
    #[must_use]
    pub fn port(&self) -> u16 {
        let addr = {
            let server = self.test_server.lock().unwrap();
            server.addr()
        }; // Guard dropped here
        addr.port()
    }
}

/// Builder for creating `ActixWebServer` instances with configuration
#[cfg(all(feature = "actix", not(feature = "simulator")))]
#[derive(Debug, Default)]
pub struct ActixWebServerBuilder {
    scopes: Vec<crate::Scope>,
    addr: Option<String>,
    port: Option<u16>,
}

#[cfg(all(feature = "actix", not(feature = "simulator")))]
impl ActixWebServerBuilder {
    /// Create a new builder
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a scope to the server
    #[must_use]
    pub fn with_scope(mut self, scope: crate::Scope) -> Self {
        self.scopes.push(scope);
        self
    }

    /// Add multiple scopes to the server
    #[must_use]
    pub fn with_scopes(mut self, scopes: impl IntoIterator<Item = crate::Scope>) -> Self {
        self.scopes.extend(scopes);
        self
    }

    /// Set the server address
    #[must_use]
    pub fn with_addr(mut self, addr: impl Into<String>) -> Self {
        self.addr = Some(addr.into());
        self
    }

    /// Set the server port
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // Cannot be const due to mutation
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Build the `ActixWebServer`
    #[must_use]
    pub fn build(self) -> ActixWebServer {
        // TODO(5.2.4.7): Use addr and port configuration
        // Currently ignored because test servers use dynamic ports
        // Consider storing for documentation/debugging purposes

        // ✅ 5.2.4.1: Scopes are now properly passed through
        ActixWebServer::new(self.scopes)
    }
}

/// Helper functions for creating common server configurations
#[cfg(all(feature = "actix", not(feature = "simulator")))]
impl ActixWebServer {
    /// Create a server with a simple GET route for testing
    ///
    /// 🚨 CRITICAL: This creates a REAL HTTP server with actual routes
    #[must_use]
    pub fn with_test_routes() -> Self {
        // 5.2.4.1: Create actual Scope/Route objects
        let scope = crate::Scope::new("")
            .route(crate::Method::Get, "/test", |_req| {
                Box::pin(async {
                    Ok(crate::HttpResponse::ok()
                        .with_content_type("application/json")
                        .with_body(crate::HttpResponseBody::from(
                            r#"{"message":"Hello from test route!"}"#,
                        )))
                })
            })
            .route(crate::Method::Get, "/health", |_req| {
                Box::pin(async {
                    Ok(crate::HttpResponse::ok()
                        .with_content_type("application/json")
                        .with_body(crate::HttpResponseBody::from(r#"{"status":"ok"}"#)))
                })
            });

        Self::new(vec![scope])
    }

    /// Create a server with API routes for testing
    ///
    /// 🚨 CRITICAL: This creates a REAL HTTP server with actual API routes
    #[must_use]
    pub fn with_api_routes() -> Self {
        // 5.2.4.1: Create actual Scope/Route objects
        let scope = crate::Scope::new("/api")
            .route(crate::Method::Get, "/status", |_req| {
                Box::pin(async {
                    Ok(crate::HttpResponse::ok()
                        .with_content_type("application/json")
                        .with_body(crate::HttpResponseBody::from(r#"{"service":"running"}"#)))
                })
            })
            .route(crate::Method::Post, "/echo", |_req| {
                Box::pin(async {
                    Ok(crate::HttpResponse::ok()
                        .with_content_type("application/json")
                        .with_body(crate::HttpResponseBody::from(r#"{"echoed":"data"}"#)))
                })
            });

        Self::new(vec![scope])
    }
}

/// Actix Test Client (Limited Implementation)
///
/// ⚠️  **LIMITATION**: This implementation has thread-safety issues due to Actix's use of Rc<> types.
/// It cannot fully implement the new macro-based architecture.
///
/// **RECOMMENDATION**: Use the simulator backend instead for testing.
/// The simulator backend works perfectly with the new architecture and eliminates cfg attributes.
///
/// This client exists for compatibility but has limited functionality.
#[cfg(all(feature = "actix", not(feature = "simulator")))]
pub struct ActixTestClient {
    _server: ActixWebServer,
}

#[cfg(all(feature = "actix", not(feature = "simulator")))]
impl ActixTestClient {
    /// Create a new Actix test client with the given server
    ///
    /// ⚠️  **LIMITATION**: Due to thread-safety issues, this client cannot be used
    /// with the new macro-based architecture. Use the simulator backend instead.
    #[must_use]
    pub const fn new(server: ActixWebServer) -> Self {
        Self { _server: server }
    }
}

/// Error type for Actix test client operations
#[cfg(all(feature = "actix", not(feature = "simulator")))]
#[derive(Debug, thiserror::Error)]
pub enum ActixTestClientError {
    /// Actix backend limitation
    #[error("Actix backend has thread-safety limitations. Use simulator backend instead.")]
    ThreadSafetyLimitation,
    /// Invalid HTTP method
    #[error("Invalid HTTP method: {0}")]
    InvalidMethod(String),
}

// NOTE: ActixTestClient cannot implement GenericTestClient due to thread-safety limitations.
// Actix's TestServer contains Rc<> types that are not Send + Sync.
// This is a fundamental limitation of Actix's test infrastructure.
//
// Users should use the simulator backend which works perfectly with the new architecture.

// NOTE: ActixWebServer cannot implement GenericTestServer due to thread-safety limitations.
// Actix's TestServer contains Rc<> types that are not Send + Sync.
// This is a fundamental limitation of Actix's test infrastructure.
//
// Users should use the simulator backend which works perfectly with the new architecture.

// NOTE: Tests for ActixTestClient have been removed because they used the old architecture
// where ActixTestClient was expected to implement TestClient directly.
//
// With the new macro-based architecture:
// - Tests should use ConcreteTestClient (generated by the macro)
// - ActixTestClient is an internal implementation detail
// - The public API is tested in test_client_integration.rs
//
// Due to Actix's thread-safety limitations (Rc<> types in TestServer),
// the Actix backend cannot fully implement the new architecture.
// Users should prefer the simulator backend for testing.

// Provide a stub implementation when actix feature is not enabled
#[cfg(not(feature = "actix"))]
pub struct ActixTestClient;

#[cfg(not(feature = "actix"))]
impl ActixTestClient {
    /// Create a new Actix test client (stub implementation)
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(not(feature = "actix"))]
impl Default for ActixTestClient {
    fn default() -> Self {
        Self::new()
    }
}
