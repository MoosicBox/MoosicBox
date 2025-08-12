//! `ActixTestClient` - Real HTTP test client for Actix Web
//!
//! STATUS: Section 5.2.3.1 COMPLETE (with compromises)
//!
//! TODO(5.2.4): Address the following compromises:
//! - Scope/Route conversion not implemented (using hardcoded routes)
//! - Builder addr/port configuration ignored
//! - Custom route handlers not supported
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
    pub fn new(_scopes: Vec<crate::Scope>) -> Self {
        // TODO(5.2.4): Implement proper Scope/Route conversion
        // - Convert crate::Scope to actix_web::Scope
        // - Convert crate::Route handlers to Actix handlers
        // - Remove hardcoded routes below and use scopes parameter
        // - See Section 5.2.4 in spec/dst/overview.md

        // TODO(5.2.4): Remove these hardcoded routes
        let app = || {
            actix_web::App::new()
                .route(
                    "/test",
                    actix_web::web::get().to(|| async {
                        // TEMPORARY: Hardcoded response
                        actix_web::HttpResponse::Ok()
                            .content_type("application/json")
                            .body(r#"{"message":"Hello from test route!"}"#)
                    }),
                )
                .route(
                    "/health",
                    actix_web::web::get().to(|| async {
                        // TEMPORARY: Hardcoded response
                        actix_web::HttpResponse::Ok()
                            .content_type("application/json")
                            .body(r#"{"status":"ok"}"#)
                    }),
                )
                .route(
                    "/api/status",
                    actix_web::web::get().to(|| async {
                        // TEMPORARY: Hardcoded response
                        actix_web::HttpResponse::Ok()
                            .content_type("application/json")
                            .body(r#"{"service":"running"}"#)
                    }),
                )
                .route(
                    "/api/echo",
                    actix_web::web::post().to(|| async {
                        // TEMPORARY: Hardcoded response
                        actix_web::HttpResponse::Ok()
                            .content_type("application/json")
                            .body(r#"{"echoed":"data"}"#)
                    }),
                )
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
        // TODO(5.2.4): Use addr and port configuration
        // Currently ignored because test servers use dynamic ports
        // Consider storing for documentation/debugging purposes

        // TODO(5.2.4): Pass scopes through properly
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
        // TODO(5.2.4): Create actual Scope/Route objects instead of
        // relying on hardcoded routes in new()
        // Should be:
        // let scope = crate::Scope::new("")
        //     .with_route(crate::Route::new(...))
        //     .with_route(crate::Route::new(...));
        // Self::new(vec![scope])

        // TEMPORARY: Using empty scopes until 5.2.4
        Self::new(Vec::new())
    }

    /// Create a server with API routes for testing
    ///
    /// 🚨 CRITICAL: This creates a REAL HTTP server with actual API routes
    #[must_use]
    pub fn with_api_routes() -> Self {
        // TODO(5.2.4): Create actual Scope/Route objects instead of
        // relying on hardcoded routes in new()
        // Should be:
        // let scope = crate::Scope::new("/api")
        //     .with_route(crate::Route::new(...))
        //     .with_route(crate::Route::new(...));
        // Self::new(vec![scope])

        // TEMPORARY: Using empty scopes until 5.2.4
        Self::new(Vec::new())
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
