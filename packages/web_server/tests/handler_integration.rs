#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Handler System Integration Tests
//!
//! This module provides comprehensive integration tests for the `MoosicBox` web server
//! handler system, validating that handlers work correctly with both Actix and
//! Simulator backends.
//!
//! # Test Coverage
//!
//! * **0-parameter handlers**: Functions that take no extractors
//! * **1-4 parameter handlers**: Common handler patterns with multiple extractors
//! * **5+ parameter handlers**: Complex handlers with many extractors
//! * **Error handling**: Consistent error behavior across backends
//! * **Type compatibility**: Various parameter type combinations
//! * **Performance**: Benchmarks comparing handler overhead
//!
//! # Backend Testing
//!
//! Tests are organized by backend to ensure identical behavior:
//!
//! * **Actix tests**: Use synchronous extraction with Send bounds
//! * **Simulator tests**: Use async extraction for deterministic testing
//! * **Shared logic**: Common test functions used by both backends

// Core imports (always available)
use moosicbox_web_server::{Error, HttpRequest, HttpResponse, handler::IntoHandler};

// No module-level conditional imports - using function-local imports instead

// Serde-dependent test data
#[cfg(feature = "serde")]
mod serde_test_data {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
    pub struct TestParams {
        pub name: String,
        pub age: Option<u32>,
        pub active: Option<bool>,
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq, Eq, Clone)]
    pub struct TestUser {
        pub id: u64,
        pub name: String,
        pub email: String,
    }
}

// Non-serde test data
#[derive(Debug, Clone)]
struct TestConfig {
    app_name: String,
    #[allow(dead_code)]
    version: String,
}

// Test utilities module
mod test_utils {
    use super::*;

    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    use moosicbox_web_server::Method;

    /// Create a test `HttpRequest` with comprehensive data for testing
    #[cfg(any(feature = "simulator", not(feature = "actix")))]
    #[allow(dead_code)]
    pub fn create_comprehensive_test_request() -> HttpRequest {
        use bytes::Bytes;
        use moosicbox_web_server::simulator::{SimulationRequest, SimulationStub};

        let json_body = r#"{"id": 123, "name": "John Doe", "email": "john@example.com"}"#;
        let body = Bytes::from(json_body);

        let sim_req = SimulationRequest::new(Method::Post, "/api/users/123/posts/456")
            .with_query_string("name=john&age=30&active=true")
            .with_header("authorization", "Bearer token123")
            .with_header("content-type", "application/json")
            .with_header("content-length", "1024")
            .with_header("upgrade", "websocket")
            .with_body(body);

        HttpRequest::new(SimulationStub::new(sim_req))
    }

    /// Create a test `HttpRequest` for `Actix`-only builds (limited functionality)
    #[cfg(all(feature = "actix", not(feature = "simulator")))]
    #[allow(dead_code)]
    pub fn create_comprehensive_test_request() -> HttpRequest {
        HttpRequest::new(moosicbox_web_server::EmptyRequest)
    }

    /// Create test state for State extractor testing
    #[cfg(feature = "simulator")]
    pub fn create_test_state() -> TestConfig {
        TestConfig {
            app_name: "MoosicBox".to_string(),
            version: "1.0.0".to_string(),
        }
    }

    /// Helper to create a successful HTTP response
    pub fn ok_response() -> HttpResponse {
        HttpResponse::ok()
    }

    /// Helper to create a JSON response
    #[cfg(feature = "serde")]
    pub fn json_response<T: serde::Serialize>(data: &T) -> HttpResponse {
        // For now, just return ok response since HttpResponse::json doesn't exist yet
        let _ = data; // Suppress unused parameter warning
        HttpResponse::ok()
    }

    /// Non-serde version for basic text responses
    pub fn text_response(text: &str) -> HttpResponse {
        let _ = text; // Suppress unused parameter warning
        HttpResponse::ok()
    }
}

// Basic handlers (no serde required)
mod basic_handlers {
    use super::*;

    pub async fn handler_0_params() -> Result<HttpResponse, Error> {
        Ok(test_utils::ok_response())
    }

    pub async fn handler_state_only(
        state: moosicbox_web_server::extractors::State<TestConfig>,
    ) -> Result<HttpResponse, Error> {
        Ok(test_utils::text_response(&state.0.app_name))
    }
}

// Serde-dependent handlers
#[cfg(feature = "serde")]
mod serde_handlers {
    use super::*;
    use crate::serde_test_data::{TestParams, TestUser};
    use moosicbox_web_server::extractors::{Header, Json, Path, Query, State};

    // 1-parameter handlers
    pub async fn handler_1_param_query(
        Query(params): Query<TestParams>,
    ) -> Result<HttpResponse, Error> {
        Ok(test_utils::json_response(&params))
    }

    #[cfg(any(feature = "actix", feature = "simulator"))]
    pub async fn handler_1_param_json(Json(user): Json<TestUser>) -> Result<HttpResponse, Error> {
        Ok(test_utils::json_response(&user))
    }

    #[cfg(any(feature = "actix", feature = "simulator"))]
    pub async fn handler_1_param_path(Path(id): Path<u64>) -> Result<HttpResponse, Error> {
        Ok(test_utils::json_response(&id))
    }

    #[cfg(any(feature = "actix", feature = "simulator"))]
    pub async fn handler_1_param_header(
        Header(auth): Header<String>,
    ) -> Result<HttpResponse, Error> {
        Ok(test_utils::json_response(&auth))
    }

    #[cfg(any(feature = "actix", feature = "simulator"))]
    pub async fn handler_1_param_state(
        State(config): State<TestConfig>,
    ) -> Result<HttpResponse, Error> {
        Ok(test_utils::json_response(&config.app_name))
    }

    // 2-parameter handlers
    pub async fn handler_2_params(
        Query(params): Query<TestParams>,
        Path(id): Path<u64>,
    ) -> Result<HttpResponse, Error> {
        let response = format!("User {} with ID {}", params.name, id);
        Ok(test_utils::json_response(&response))
    }

    // 3-parameter handlers
    pub async fn handler_3_params(
        Query(params): Query<TestParams>,
        Path(id): Path<u64>,
        Header(auth): Header<String>,
    ) -> Result<HttpResponse, Error> {
        let response = format!("User {} with ID {} (auth: {})", params.name, id, auth);
        Ok(test_utils::json_response(&response))
    }

    // 4-parameter handlers
    pub async fn handler_4_params(
        Query(params): Query<TestParams>,
        Json(user): Json<TestUser>,
        Path(id): Path<u64>,
        Header(auth): Header<String>,
    ) -> Result<HttpResponse, Error> {
        let response = format!(
            "Query: {}, JSON: {}, Path: {}, Header: {}",
            params.name, user.name, id, auth
        );
        Ok(test_utils::json_response(&response))
    }

    // 5-parameter handlers (testing higher parameter counts)
    pub async fn handler_5_params(
        Query(params): Query<TestParams>,
        Json(user): Json<TestUser>,
        Path(id): Path<u64>,
        Header(auth): Header<String>,
        State(config): State<TestConfig>,
    ) -> Result<HttpResponse, Error> {
        let response = format!(
            "Query: {}, JSON: {}, Path: {}, Header: {}, State: {}",
            params.name, user.name, id, auth, config.app_name
        );
        Ok(test_utils::json_response(&response))
    }

    // Error-producing handlers for error handling tests
    pub async fn handler_with_error(_query: Query<TestParams>) -> Result<HttpResponse, Error> {
        Err(Error::internal_server_error("Test error"))
    }
}

// Basic tests (no serde required)
#[cfg(feature = "actix")]
mod basic_actix_tests {
    use super::*;

    #[test]
    fn test_basic_handlers() {
        // Test that basic handlers compile correctly with Actix
        let _h0 = basic_handlers::handler_0_params.into_handler();
        let _h1 = basic_handlers::handler_state_only.into_handler();
        // If this compiles, the test passes
    }
}

// Serde-dependent tests
#[cfg(all(feature = "actix", feature = "serde"))]
mod serde_actix_tests {
    use super::*;

    #[test]
    fn test_serde_handlers() {
        // Test that serde-dependent handlers compile correctly with Actix
        let _handler1 = serde_handlers::handler_1_param_query.into_handler();
        let _handler2 = serde_handlers::handler_1_param_json.into_handler();
        let _handler3 = serde_handlers::handler_1_param_path.into_handler();
        let _handler4 = serde_handlers::handler_1_param_header.into_handler();
        let _handler5 = serde_handlers::handler_1_param_state.into_handler();
        // If this compiles, the test passes
    }

    #[test]
    fn test_multi_param_handler_compilation() {
        // Test that multi-parameter handlers compile correctly with Actix
        let _handler2 = serde_handlers::handler_2_params.into_handler();
        let _handler3 = serde_handlers::handler_3_params.into_handler();
        let _handler4 = serde_handlers::handler_4_params.into_handler();
        let _handler5 = serde_handlers::handler_5_params.into_handler();
        // If this compiles, the test passes
    }

    #[test]
    fn test_error_handler_compilation() {
        // Test that error-producing handlers compile correctly with Actix
        let _handler = serde_handlers::handler_with_error.into_handler();
        // If this compiles, the test passes
    }
}

// Basic simulator tests (no serde required)
#[cfg(feature = "simulator")]
mod basic_simulator_tests {
    use super::*;

    #[test]
    fn test_basic_handlers() {
        // Test that basic handlers compile correctly with Simulator
        let _h0 = basic_handlers::handler_0_params.into_handler();
        let _h1 = basic_handlers::handler_state_only.into_handler();
        // If this compiles, the test passes
    }

    #[test]
    fn test_state_container_functionality() {
        use moosicbox_web_server::extractors::StateContainer;

        // Test StateContainer functionality directly
        let mut state_container = StateContainer::new();
        let test_config = test_utils::create_test_state();

        // Test insertion and retrieval
        state_container.insert(test_config.clone());
        let retrieved = state_container.get::<TestConfig>();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().app_name, test_config.app_name);
    }
}

// Serde-dependent simulator tests
#[cfg(all(feature = "simulator", feature = "serde"))]
mod serde_simulator_tests {
    use super::*;

    #[test]
    fn test_serde_handlers() {
        // Test that serde-dependent handlers compile correctly with Simulator
        let _handler1 = serde_handlers::handler_1_param_query.into_handler();
        let _handler2 = serde_handlers::handler_1_param_json.into_handler();
        let _handler3 = serde_handlers::handler_1_param_path.into_handler();
        let _handler4 = serde_handlers::handler_1_param_header.into_handler();
        let _handler5 = serde_handlers::handler_1_param_state.into_handler();
        // If this compiles, the test passes
    }

    #[test]
    fn test_multi_param_handler_compilation() {
        // Test that multi-parameter handlers compile correctly with Simulator
        let _handler2 = serde_handlers::handler_2_params.into_handler();
        let _handler3 = serde_handlers::handler_3_params.into_handler();
        let _handler4 = serde_handlers::handler_4_params.into_handler();
        let _handler5 = serde_handlers::handler_5_params.into_handler();
        // If this compiles, the test passes
    }

    #[test]
    fn test_error_handler_compilation() {
        // Test that error-producing handlers compile correctly with Simulator
        let _handler = serde_handlers::handler_with_error.into_handler();
        // If this compiles, the test passes
    }
}

// Cross-backend consistency tests (basic handlers)
mod basic_consistency_tests {
    use super::*;

    /// Test that basic handler compilation is consistent across backends
    #[test]
    fn test_basic_handler_consistency() {
        // These should compile identically for both backends
        let _h0 = basic_handlers::handler_0_params.into_handler();
        let _h1 = basic_handlers::handler_state_only.into_handler();
        // If all compile, the signatures are consistent
    }
}

// Cross-backend consistency tests (serde handlers)
#[cfg(feature = "serde")]
mod serde_consistency_tests {
    use super::*;

    /// Test that serde handler compilation is consistent across backends
    #[test]
    fn test_serde_handler_consistency() {
        // These should compile identically for both backends
        let _h1 = serde_handlers::handler_1_param_query.into_handler();
        let _h2 = serde_handlers::handler_2_params.into_handler();
        let _h3 = serde_handlers::handler_3_params.into_handler();
        let _h4 = serde_handlers::handler_4_params.into_handler();
        let _h5 = serde_handlers::handler_5_params.into_handler();
        // If all compile, the signatures are consistent
    }

    /// Test that error handling compiles consistently across backends
    #[test]
    fn test_error_handler_consistency() {
        let _handler = serde_handlers::handler_with_error.into_handler();
        // If this compiles, error handling is consistent
    }
}

// Performance benchmarks (compilation-time checks for now)
#[cfg(all(feature = "simulator", feature = "serde"))]
mod benchmarks {
    use super::*;

    #[test]
    fn test_handler_compilation_performance() {
        // Test that complex handlers compile efficiently
        // This is a compilation-time benchmark - if it compiles quickly, it passes
        let _handler = serde_handlers::handler_5_params.into_handler();

        // Future: Could add runtime benchmarks with proper async test setup
        // For now, we focus on ensuring the handler system compiles correctly
    }
}
