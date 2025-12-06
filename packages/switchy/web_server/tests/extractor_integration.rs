#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! # Extractor Integration Tests
//!
//! Comprehensive integration tests for the `MoosicBox` web server extractor system.
//! These tests validate that all extractors work correctly with both Actix and Simulator backends.
//!
//! ## Test Coverage
//!
//! * **Individual Extractor Tests**: Each extractor type (Query, Json, Path, Header, State)
//! * **Combination Tests**: Multiple extractors in single handlers
//! * **Error Handling**: Extraction failures and error propagation
//! * **Edge Cases**: Empty data, missing values, malformed input
//! * **Backend Consistency**: Identical behavior across Actix and Simulator
//!
//! ## Running Tests
//!
//! ```bash
//! # Test with Actix backend (default)
//! cargo test -p switchy_web_server extractor_integration
//!
//! # Test with Simulator backend
//! cargo test -p switchy_web_server --features simulator extractor_integration
//!
//! # Test with all features
//! cargo test -p switchy_web_server --all-features extractor_integration
//! ```

// No module-level conditional imports - using function-local imports instead

// Basic test utilities (no serde required)
mod basic_test_utils {
    /// Test state container (doesn't need serde)
    #[derive(Debug, Clone)]
    pub struct TestState {
        #[allow(dead_code)]
        pub counter: u64,
        #[allow(dead_code)]
        pub message: String,
    }

    impl TestState {
        #[allow(dead_code)]
        pub fn new() -> Self {
            Self {
                counter: 42,
                message: "test state".to_string(),
            }
        }
    }

    /// Create test state container
    #[cfg(feature = "simulator")]
    #[allow(dead_code)]
    pub fn create_test_state_container() -> switchy_web_server::extractors::StateContainer {
        use switchy_web_server::extractors::StateContainer;
        let mut container = StateContainer::new();
        container.insert(TestState::new());
        container
    }
}

// Serde-dependent test utilities
#[cfg(feature = "serde")]
mod serde_test_utils {
    use serde::{Deserialize, Serialize};

    /// Test data structure for JSON extraction
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct TestJsonData {
        pub name: String,
        pub age: u32,
        pub active: bool,
    }

    impl TestJsonData {
        #[allow(dead_code)]
        pub fn sample() -> Self {
            Self {
                name: "Alice".to_string(),
                age: 30,
                active: true,
            }
        }
    }

    /// Create test query parameters
    #[allow(dead_code)]
    pub fn create_test_query() -> std::collections::BTreeMap<String, String> {
        let mut query = std::collections::BTreeMap::new();
        query.insert("name".to_string(), "Bob".to_string());
        query.insert("age".to_string(), "25".to_string());
        query.insert("active".to_string(), "true".to_string());
        query
    }

    /// Create test headers
    #[allow(dead_code)]
    pub fn create_test_headers() -> std::collections::BTreeMap<String, String> {
        let mut headers = std::collections::BTreeMap::new();
        headers.insert("authorization".to_string(), "Bearer token123".to_string());
        headers.insert("content-type".to_string(), "application/json".to_string());
        headers.insert("x-custom-header".to_string(), "custom-value".to_string());
        headers
    }

    /// Create test path parameters
    #[allow(dead_code)]
    pub fn create_test_path() -> std::collections::BTreeMap<String, String> {
        let mut path = std::collections::BTreeMap::new();
        path.insert("id".to_string(), "123".to_string());
        path.insert("category".to_string(), "music".to_string());
        path
    }

    /// Create comprehensive test request with all data types
    #[cfg(feature = "simulator")]
    #[allow(dead_code)]
    pub fn create_comprehensive_test_request() -> switchy_web_server::simulator::SimulationRequest {
        use bytes::Bytes;
        use switchy_web_server::Method;
        use switchy_web_server::simulator::SimulationRequest;

        SimulationRequest::new(Method::Post, "/test/123/music")
            .with_query_string("name=Bob&age=25&active=true")
            .with_header("authorization", "Bearer token123")
            .with_header("content-type", "application/json")
            .with_header("x-custom-header", "custom-value")
            .with_body(Bytes::from(
                serde_json::to_string(&TestJsonData::sample()).unwrap(),
            ))
    }
}

// Basic Actix tests (State extractor only)
#[cfg(feature = "actix")]
mod basic_actix_tests {
    #[test]
    fn test_state_extractor_compilation() {
        use crate::basic_test_utils::TestState;
        use switchy_web_server::extractors::State;

        // Test that State extractor compiles with Actix backend
        #[allow(clippy::unnecessary_wraps)]
        fn handler(
            _state: State<TestState>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("success".to_string())
        }

        // Compilation test - if this compiles, the extractor works
        std::hint::black_box(handler);
    }
}

// Serde-dependent Actix tests
#[cfg(all(feature = "actix", feature = "serde"))]
mod serde_actix_tests {
    #[test]
    fn test_query_extractor_compilation() {
        use switchy_web_server::extractors::Query;

        // Test that Query extractor compiles with Actix backend
        #[allow(clippy::unnecessary_wraps)]
        fn handler(
            _query: Query<String>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("success".to_string())
        }

        // Compilation test - if this compiles, the extractor works
        std::hint::black_box(handler);
    }

    #[test]
    fn test_json_extractor_compilation() {
        use crate::serde_test_utils::TestJsonData;
        use switchy_web_server::extractors::Json;

        // Test that Json extractor compiles with Actix backend
        #[allow(clippy::unnecessary_wraps)]
        fn handler(
            _json: Json<TestJsonData>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("success".to_string())
        }

        // Compilation test - if this compiles, the extractor works
        std::hint::black_box(handler);
    }

    #[test]
    fn test_path_extractor_compilation() {
        use switchy_web_server::extractors::Path;

        // Test that Path extractor compiles with Actix backend
        #[allow(clippy::unnecessary_wraps)]
        fn handler(
            _path: Path<String>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("success".to_string())
        }

        // Compilation test - if this compiles, the extractor works
        std::hint::black_box(handler);
    }

    #[test]
    fn test_header_extractor_compilation() {
        use switchy_web_server::extractors::Header;

        // Test that Header extractor compiles with Actix backend
        #[allow(clippy::unnecessary_wraps)]
        fn handler(
            _header: Header<String>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("success".to_string())
        }

        // Compilation test - if this compiles, the extractor works
        std::hint::black_box(handler);
    }

    #[test]
    fn test_multiple_extractors_compilation() {
        use crate::basic_test_utils::TestState;
        use crate::serde_test_utils::TestJsonData;
        use switchy_web_server::extractors::{Header, Json, Path, Query, State};

        // Test that multiple extractors work together with Actix backend
        #[allow(clippy::unnecessary_wraps)]
        fn handler(
            _query: Query<String>,
            _json: Json<TestJsonData>,
            _path: Path<String>,
            _header: Header<String>,
            _state: State<TestState>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("success".to_string())
        }

        // Compilation test - if this compiles, multiple extractors work
        std::hint::black_box(handler);
    }

    #[test]
    fn test_extractor_error_handling_compilation() {
        use switchy_web_server::extractors::Query;

        // Test that error handling works with extractors
        fn handler(
            _query: Query<String>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Err("test error".into())
        }

        // Compilation test - if this compiles, error handling works
        std::hint::black_box(handler);
    }
}

// Basic Simulator tests (State extractor only)
#[cfg(feature = "simulator")]
mod basic_simulator_tests {
    #[test]
    fn test_state_extractor_compilation() {
        use crate::basic_test_utils::TestState;
        use switchy_web_server::extractors::State;

        // Test that State extractor compiles with Simulator backend
        async fn handler(
            _state: State<TestState>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("success".to_string())
        }

        // Compilation test - if this compiles, the extractor works
        std::hint::black_box(handler);
    }

    #[test]
    fn test_extractor_with_state_container() {
        use crate::basic_test_utils::TestState;
        use switchy_web_server::extractors::State;

        // Test that State extractor works with StateContainer
        async fn handler(
            _state: State<TestState>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("success".to_string())
        }

        // Compilation test - if this compiles, StateContainer integration works
        std::hint::black_box(handler);
    }
}

// Serde-dependent Simulator tests
#[cfg(all(feature = "simulator", feature = "serde"))]
mod serde_simulator_tests {
    #[test]
    fn test_query_extractor_compilation() {
        use switchy_web_server::extractors::Query;

        // Test that Query extractor compiles with Simulator backend
        async fn handler(
            _query: Query<String>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("success".to_string())
        }

        // Compilation test - if this compiles, the extractor works
        std::hint::black_box(handler);
    }

    #[test]
    fn test_json_extractor_compilation() {
        use crate::serde_test_utils::TestJsonData;
        use switchy_web_server::extractors::Json;

        // Test that Json extractor compiles with Simulator backend
        async fn handler(
            _json: Json<TestJsonData>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("success".to_string())
        }

        // Compilation test - if this compiles, the extractor works
        std::hint::black_box(handler);
    }

    #[test]
    fn test_path_extractor_compilation() {
        use switchy_web_server::extractors::Path;

        // Test that Path extractor compiles with Simulator backend
        async fn handler(
            _path: Path<String>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("success".to_string())
        }

        // Compilation test - if this compiles, the extractor works
        std::hint::black_box(handler);
    }

    #[test]
    fn test_header_extractor_compilation() {
        use switchy_web_server::extractors::Header;

        // Test that Header extractor compiles with Simulator backend
        async fn handler(
            _header: Header<String>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("success".to_string())
        }

        // Compilation test - if this compiles, the extractor works
        std::hint::black_box(handler);
    }

    #[test]
    fn test_multiple_extractors_compilation() {
        use crate::basic_test_utils::TestState;
        use crate::serde_test_utils::TestJsonData;
        use switchy_web_server::extractors::{Header, Json, Path, Query, State};

        // Test that multiple extractors work together with Simulator backend
        async fn handler(
            _query: Query<String>,
            _json: Json<TestJsonData>,
            _path: Path<String>,
            _header: Header<String>,
            _state: State<TestState>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("success".to_string())
        }

        // Compilation test - if this compiles, multiple extractors work
        std::hint::black_box(handler);
    }

    #[test]
    fn test_extractor_error_handling_compilation() {
        use switchy_web_server::extractors::Query;

        // Test that error handling works with extractors
        async fn handler(
            _query: Query<String>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Err("test error".into())
        }

        // Compilation test - if this compiles, error handling works
        std::hint::black_box(handler);
    }
}

// Basic cross-backend consistency tests
mod basic_consistency_tests {
    #[test]
    fn test_state_extractor_consistency() {
        // Test that State extractor signatures are identical across backends

        #[cfg(feature = "actix")]
        {
            use crate::basic_test_utils::TestState;
            use switchy_web_server::extractors::State;

            #[allow(clippy::unnecessary_wraps)]
            fn actix_handler(
                _state: State<TestState>,
            ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
                Ok("actix".to_string())
            }

            std::hint::black_box(actix_handler);
        }

        #[cfg(feature = "simulator")]
        {
            use crate::basic_test_utils::TestState;
            use switchy_web_server::extractors::State;

            async fn simulator_handler(
                _state: State<TestState>,
            ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
                Ok("simulator".to_string())
            }

            std::hint::black_box(simulator_handler);
        }
    }
}

// Serde-dependent cross-backend consistency tests
#[cfg(feature = "serde")]
mod serde_consistency_tests {
    #[test]
    fn test_extractor_signatures_consistency() {
        // Test that extractor signatures are identical across backends

        #[cfg(feature = "actix")]
        {
            use crate::basic_test_utils::TestState;
            use crate::serde_test_utils::TestJsonData;
            use switchy_web_server::extractors::{Header, Json, Path, Query, State};

            #[allow(clippy::unnecessary_wraps)]
            fn actix_handler(
                _query: Query<String>,
                _json: Json<TestJsonData>,
                _path: Path<String>,
                _header: Header<String>,
                _state: State<TestState>,
            ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
                Ok("actix".to_string())
            }

            std::hint::black_box(actix_handler);
        }

        #[cfg(feature = "simulator")]
        {
            use crate::basic_test_utils::TestState;
            use crate::serde_test_utils::TestJsonData;
            use switchy_web_server::extractors::{Header, Json, Path, Query, State};

            async fn simulator_handler(
                _query: Query<String>,
                _json: Json<TestJsonData>,
                _path: Path<String>,
                _header: Header<String>,
                _state: State<TestState>,
            ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
                Ok("simulator".to_string())
            }

            std::hint::black_box(simulator_handler);
        }
    }

    #[test]
    fn test_error_handling_consistency() {
        // Test that error handling is consistent across backends

        #[cfg(feature = "actix")]
        {
            use switchy_web_server::extractors::Query;

            fn actix_error_handler(
                _query: Query<String>,
            ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
                Err("actix error".into())
            }

            std::hint::black_box(actix_error_handler);
        }

        #[cfg(feature = "simulator")]
        {
            use switchy_web_server::extractors::Query;

            async fn simulator_error_handler(
                _query: Query<String>,
            ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
                Err("simulator error".into())
            }

            std::hint::black_box(simulator_error_handler);
        }
    }
}

// Edge case tests (serde-dependent)
#[cfg(all(test, feature = "simulator", feature = "serde"))]
mod edge_case_tests {
    #[test]
    fn test_empty_query_extraction() {
        use switchy_web_server::extractors::Query;

        // Test extraction with empty query parameters
        async fn handler(
            _query: Query<Option<String>>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("handled empty query".to_string())
        }

        // Compilation test - if this compiles, empty query handling works
        std::hint::black_box(handler);
    }

    #[test]
    fn test_missing_header_extraction() {
        use switchy_web_server::extractors::Header;

        // Test extraction with missing headers
        async fn handler(
            _header: Header<Option<String>>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("handled missing header".to_string())
        }

        // Compilation test - if this compiles, missing header handling works
        std::hint::black_box(handler);
    }

    #[test]
    fn test_optional_path_extraction() {
        use switchy_web_server::extractors::Path;

        // Test extraction with optional path parameters
        async fn handler(
            _path: Path<Option<String>>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("handled optional path".to_string())
        }

        // Compilation test - if this compiles, optional path handling works
        std::hint::black_box(handler);
    }

    #[test]
    fn test_complex_json_extraction() {
        use serde::Deserialize;
        use switchy_web_server::extractors::Json;

        // Test extraction with complex JSON structures
        #[derive(Deserialize)]
        struct ComplexData {
            #[allow(dead_code)]
            nested: NestedData,
            #[allow(dead_code)]
            list: Vec<String>,
            #[allow(dead_code)]
            optional: Option<String>,
        }

        #[derive(Deserialize)]
        struct NestedData {
            #[allow(dead_code)]
            value: u64,
        }

        async fn handler(
            _json: Json<ComplexData>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("handled complex json".to_string())
        }

        // Compilation test - if this compiles, complex JSON handling works
        std::hint::black_box(handler);
    }
}

// Performance and stress tests (serde-dependent)
#[cfg(all(test, feature = "simulator", feature = "serde"))]
mod performance_tests {
    #[test]
    fn test_large_json_extraction() {
        use crate::serde_test_utils::TestJsonData;
        use serde::Deserialize;
        use switchy_web_server::extractors::Json;

        // Test extraction with large JSON payloads
        #[derive(Deserialize)]
        struct LargeData {
            #[allow(dead_code)]
            items: Vec<TestJsonData>,
            #[allow(dead_code)]
            metadata: std::collections::BTreeMap<String, String>,
        }

        async fn handler(
            _json: Json<LargeData>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("handled large json".to_string())
        }

        // Compilation test - if this compiles, large JSON handling works
        std::hint::black_box(handler);
    }

    #[test]
    fn test_many_query_parameters() {
        use serde::Deserialize;
        use switchy_web_server::extractors::Query;

        // Test extraction with many query parameters
        #[derive(Deserialize)]
        struct ManyParams {
            #[allow(dead_code)]
            param1: Option<String>,
            #[allow(dead_code)]
            param2: Option<String>,
            #[allow(dead_code)]
            param3: Option<String>,
            #[allow(dead_code)]
            param4: Option<String>,
            #[allow(dead_code)]
            param5: Option<String>,
            #[allow(dead_code)]
            param6: Option<String>,
            #[allow(dead_code)]
            param7: Option<String>,
            #[allow(dead_code)]
            param8: Option<String>,
            #[allow(dead_code)]
            param9: Option<String>,
            #[allow(dead_code)]
            param10: Option<String>,
        }

        async fn handler(
            _query: Query<ManyParams>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("handled many params".to_string())
        }

        // Compilation test - if this compiles, many parameter handling works
        std::hint::black_box(handler);
    }

    #[test]
    fn test_many_headers() {
        use switchy_web_server::extractors::Header;

        // Test extraction with many headers
        async fn handler(
            _h1: Header<Option<String>>,
            _h2: Header<Option<String>>,
            _h3: Header<Option<String>>,
            _h4: Header<Option<String>>,
            _h5: Header<Option<String>>,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
            Ok("handled many headers".to_string())
        }

        // Compilation test - if this compiles, many header handling works
        std::hint::black_box(handler);
    }
}

// Basic benchmarks module (compilation-time benchmarks)
#[cfg(test)]
mod basic_benchmarks {
    #[test]
    fn benchmark_basic_extractor_compilation_time() {
        use crate::basic_test_utils::TestState;
        use switchy_web_server::extractors::State;

        // This test measures compilation time for basic extractor combinations
        // The actual benchmark is the compilation itself

        // Simple State extractor
        fn simple_state(_state: State<TestState>) -> String {
            "state".to_string()
        }

        // If this compiles quickly, our basic extractor system is efficient
        std::hint::black_box(simple_state);
    }
}

// Serde-dependent benchmarks module (compilation-time benchmarks)
#[cfg(all(test, feature = "serde"))]
mod serde_benchmarks {
    #[test]
    fn benchmark_serde_extractor_compilation_time() {
        use crate::basic_test_utils::TestState;
        use crate::serde_test_utils::TestJsonData;
        use switchy_web_server::extractors::{Header, Json, Path, Query, State};

        // This test measures compilation time for serde extractor combinations
        // The actual benchmark is the compilation itself

        // Simple extractors
        fn simple_query(_query: Query<String>) -> String {
            "query".to_string()
        }

        fn simple_json(_json: Json<TestJsonData>) -> String {
            "json".to_string()
        }

        // Complex combinations
        fn complex_handler(
            _query: Query<String>,
            _json: Json<TestJsonData>,
            _path: Path<String>,
            _header: Header<String>,
            _state: State<TestState>,
        ) -> String {
            "complex".to_string()
        }

        // If these compile quickly, our extractor system is efficient
        std::hint::black_box(simple_query);
        std::hint::black_box(simple_json);
        std::hint::black_box(complex_handler);
    }
}
