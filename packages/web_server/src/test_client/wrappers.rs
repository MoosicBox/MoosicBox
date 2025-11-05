//! Wrapper types for test client abstraction
//!
//! This module provides wrapper types that implement the original `TestClient` trait
//! while delegating to the generic traits. This follows the same pattern as
//! `RngWrapper` in the `switchy_random` package.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::traits::{GenericTestClient, GenericTestServer};
use super::{HttpMethod, TestRequestBuilder, TestResponse};

/// Wrapper for test client implementations
///
/// This wrapper provides thread-safe access to any `GenericTestClient` implementation
/// and implements the original `TestClient` trait. It follows the same pattern as
/// `RngWrapper` in `switchy_random`.
pub struct TestClientWrapper<C: GenericTestClient>(
    /// Thread-safe reference to the underlying client implementation
    Arc<Mutex<C>>,
);

impl<C: GenericTestClient> Clone for TestClientWrapper<C> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<C: GenericTestClient> TestClientWrapper<C> {
    /// Create a new test client wrapper
    #[must_use]
    pub fn new(client: C) -> Self {
        Self(Arc::new(Mutex::new(client)))
    }

    /// Get the base URL for the test server
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn base_url(&self) -> String {
        self.0.lock().unwrap().base_url()
    }

    /// Build a full URL from a path
    #[must_use]
    pub fn url(&self, path: &str) -> String {
        let base = self.base_url();
        if path.starts_with('/') {
            format!("{base}{path}")
        } else {
            format!("{base}/{path}")
        }
    }
}

/// Error type for test client wrapper operations
#[derive(Debug, thiserror::Error)]
pub enum TestClientWrapperError {
    /// Underlying client error
    #[error("Test client error: {0}")]
    Client(Box<dyn std::error::Error + Send + Sync>),
    /// Lock acquisition error
    #[error("Failed to acquire client lock")]
    Lock,
}

impl<C: GenericTestClient> super::TestClient for TestClientWrapper<C> {
    type Error = TestClientWrapperError;

    fn get(&self, path: &str) -> TestRequestBuilder<'_, Self> {
        TestRequestBuilder::new(self, HttpMethod::Get, path.to_string())
    }

    fn post(&self, path: &str) -> TestRequestBuilder<'_, Self> {
        TestRequestBuilder::new(self, HttpMethod::Post, path.to_string())
    }

    fn put(&self, path: &str) -> TestRequestBuilder<'_, Self> {
        TestRequestBuilder::new(self, HttpMethod::Put, path.to_string())
    }

    fn delete(&self, path: &str) -> TestRequestBuilder<'_, Self> {
        TestRequestBuilder::new(self, HttpMethod::Delete, path.to_string())
    }

    fn execute_request(
        &self,
        method: &str,
        path: &str,
        headers: &BTreeMap<String, String>,
        body: Option<&[u8]>,
    ) -> Result<TestResponse, Self::Error> {
        let client = self.0.lock().map_err(|_| TestClientWrapperError::Lock)?;
        client
            .execute_request(method, path, headers, body)
            .map_err(|e| TestClientWrapperError::Client(Box::new(e)))
    }
}

/// Wrapper for test server implementations
///
/// This wrapper provides thread-safe access to any `GenericTestServer` implementation.
/// It follows the same pattern as `TestClientWrapper`.
pub struct TestServerWrapper<S: GenericTestServer>(
    /// Thread-safe reference to the underlying server implementation
    Arc<Mutex<S>>,
);

impl<S: GenericTestServer> Clone for TestServerWrapper<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<S: GenericTestServer> TestServerWrapper<S> {
    /// Create a new test server wrapper
    #[must_use]
    pub fn new(server: S) -> Self {
        Self(Arc::new(Mutex::new(server)))
    }

    /// Get the server URL
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn url(&self) -> String {
        self.0.lock().unwrap().url()
    }

    /// Get the server port
    ///
    /// # Panics
    ///
    /// Panics if the internal mutex is poisoned.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.0.lock().unwrap().port()
    }

    /// Start the server
    ///
    /// # Errors
    ///
    /// * Returns error if the server fails to start
    /// * Returns error if lock acquisition fails
    pub fn start(&self) -> Result<(), TestServerWrapperError> {
        let mut server = self.0.lock().map_err(|_| TestServerWrapperError::Lock)?;
        server
            .start()
            .map_err(|e| TestServerWrapperError::Server(Box::new(e)))
    }

    /// Stop the server
    ///
    /// # Errors
    ///
    /// * Returns error if the server fails to stop
    /// * Returns error if lock acquisition fails
    pub fn stop(&self) -> Result<(), TestServerWrapperError> {
        let mut server = self.0.lock().map_err(|_| TestServerWrapperError::Lock)?;
        server
            .stop()
            .map_err(|e| TestServerWrapperError::Server(Box::new(e)))
    }
}

/// Error type for test server wrapper operations
#[derive(Debug, thiserror::Error)]
pub enum TestServerWrapperError {
    /// Underlying server error
    #[error("Test server error: {0}")]
    Server(Box<dyn std::error::Error + Send + Sync>),
    /// Lock acquisition error
    #[error("Failed to acquire server lock")]
    Lock,
}
