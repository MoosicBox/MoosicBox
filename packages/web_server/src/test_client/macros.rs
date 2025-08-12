//! Macro for generating concrete test client types
//!
//! This module provides the `impl_test_client!` macro that generates concrete
//! `TestClient` and `TestServer` types based on the selected backend. It follows
//! the same pattern as `impl_rng!` in the `switchy_random` package.

/// Generate concrete `TestClient` and `TestServer` types for a specific backend
///
/// This macro generates concrete types that hide the backend selection behind
/// a clean API. It follows the same pattern as `impl_rng!` in `switchy_random`.
///
/// # Arguments
///
/// * `$client_type` - The concrete client type to wrap
/// * `$server_type` - The concrete server type to wrap
///
/// # Generated Types
///
/// * `ConcreteTestClient` - Concrete test client type
/// * `ConcreteTestServer` - Concrete test server type
///
/// # Example
///
/// ```rust,ignore
/// // For Actix backend:
/// impl_test_client!(ActixTestClient, ActixWebServer);
///
/// // For Simulator backend:
/// impl_test_client!(SimulatorTestClient, SimulatorWebServer);
/// ```
#[allow(unused)]
macro_rules! impl_test_client {
    ($client_type:ty, $server_type:ty $(,)?) => {
        // Generate concrete TestClient type
        pub type ConcreteTestClient = crate::test_client::wrappers::TestClientWrapper<$client_type>;

        // Generate concrete TestServer type
        pub type ConcreteTestServer = crate::test_client::wrappers::TestServerWrapper<$server_type>;

        // Convenience constructors for TestClient
        impl ConcreteTestClient {
            /// Create a new test client with a default test server
            #[must_use]
            pub fn new_with_test_routes() -> Self {
                let server = <$server_type>::with_test_routes();
                let client = <$client_type>::new(server);
                crate::test_client::wrappers::TestClientWrapper::new(client)
            }

            /// Create a new test client with API routes
            #[must_use]
            pub fn new_with_api_routes() -> Self {
                let server = <$server_type>::with_api_routes();
                let client = <$client_type>::new(server);
                crate::test_client::wrappers::TestClientWrapper::new(client)
            }

            /// Create a test client from an inner client implementation
            #[must_use]
            pub fn from_inner(client: $client_type) -> Self {
                crate::test_client::wrappers::TestClientWrapper::new(client)
            }
        }

        // Convenience constructors for TestServer
        impl ConcreteTestServer {
            /// Create a new test server with empty scopes
            #[must_use]
            pub fn new_empty() -> Self {
                let server = <$server_type>::new(Vec::new());
                crate::test_client::wrappers::TestServerWrapper::new(server)
            }

            /// Create a new test server with the given scopes
            #[must_use]
            pub fn new_with_scopes(scopes: Vec<crate::Scope>) -> Self {
                let server = <$server_type>::new(scopes);
                crate::test_client::wrappers::TestServerWrapper::new(server)
            }

            /// Create a test server from an inner server implementation
            #[must_use]
            pub fn from_inner(server: $server_type) -> Self {
                crate::test_client::wrappers::TestServerWrapper::new(server)
            }

            /// Create a server with test routes
            #[must_use]
            pub fn new_with_test_routes() -> Self {
                let server = <$server_type>::with_test_routes();
                crate::test_client::wrappers::TestServerWrapper::new(server)
            }

            /// Create a server with API routes
            #[must_use]
            pub fn new_with_api_routes() -> Self {
                let server = <$server_type>::with_api_routes();
                crate::test_client::wrappers::TestServerWrapper::new(server)
            }
        }

        // Default implementations
        impl Default for ConcreteTestClient {
            fn default() -> Self {
                Self::new_with_test_routes()
            }
        }

        impl Default for ConcreteTestServer {
            fn default() -> Self {
                Self::new_with_test_routes()
            }
        }
    };
}

// Make the macro available to the rest of the module
pub(crate) use impl_test_client;
