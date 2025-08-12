//! Generic traits for test client abstraction
//!
//! This module defines the core traits that enable the macro-based test client
//! architecture. Unlike the original `TestClient` trait, these traits avoid
//! Self references in return types, making them compatible with trait objects
//! and the wrapper pattern used throughout the switchy packages.

use std::collections::BTreeMap;

use super::TestResponse;

/// Generic trait for test client implementations
///
/// This trait defines the core functionality that all test client backends
/// must implement. It avoids Self references in return types to enable
/// the wrapper pattern used in switchy packages.
pub trait GenericTestClient: Send + Sync {
    /// Error type for test client operations
    type Error: std::error::Error + Send + Sync + 'static;

    /// Execute a request with the given method, path, headers, and body
    ///
    /// This is the core method that all test client implementations must provide.
    /// The wrapper types will build higher-level methods (get, post, etc.) on top of this.
    ///
    /// # Arguments
    ///
    /// * `method` - HTTP method as string (GET, POST, PUT, DELETE, etc.)
    /// * `path` - Request path (e.g., "/api/users")
    /// * `headers` - Request headers as key-value pairs
    /// * `body` - Optional request body as bytes
    ///
    /// # Errors
    ///
    /// * Returns error if the request cannot be executed
    /// * Returns error if the response cannot be parsed
    /// * Returns error if the HTTP method is not supported
    fn execute_request(
        &self,
        method: &str,
        path: &str,
        headers: &BTreeMap<String, String>,
        body: Option<&[u8]>,
    ) -> Result<TestResponse, Self::Error>;

    /// Get the base URL for the test server
    ///
    /// This method returns the full base URL (including protocol and port)
    /// that can be used to construct absolute URLs for requests.
    ///
    /// # Returns
    ///
    /// * Base URL string (e.g., <http://127.0.0.1:8080>)
    fn base_url(&self) -> String;
}

/// Generic trait for test server implementations
///
/// This trait defines the core functionality that all test server backends
/// must implement. It provides server lifecycle management and configuration.
pub trait GenericTestServer: Send + Sync {
    /// Error type for test server operations
    type Error: std::error::Error + Send + Sync + 'static;

    /// Get the server URL
    ///
    /// Returns the full URL where the server is listening.
    ///
    /// # Returns
    ///
    /// * Server URL string (e.g., <http://127.0.0.1:8080>)
    fn url(&self) -> String;
    /// Get the server port
    ///
    /// Returns the port number where the server is listening.
    ///
    /// # Returns
    ///
    /// * Port number
    fn port(&self) -> u16;

    /// Start the server
    ///
    /// Starts the test server if it's not already running.
    /// Some implementations may start automatically and this becomes a no-op.
    ///
    /// # Errors
    ///
    /// * Returns error if the server fails to start
    /// * Returns error if the server is already running and cannot be restarted
    fn start(&mut self) -> Result<(), Self::Error>;

    /// Stop the server
    ///
    /// Stops the test server if it's running.
    /// Some implementations may stop automatically on drop and this becomes a no-op.
    ///
    /// # Errors
    ///
    /// * Returns error if the server fails to stop gracefully
    fn stop(&mut self) -> Result<(), Self::Error>;
}
