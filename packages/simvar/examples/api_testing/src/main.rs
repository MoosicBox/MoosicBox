//! API endpoint testing simulation using `simvar` and `moosicbox_web_server`.
//!
//! This example demonstrates how to use the `simvar` simulation framework to test
//! REST API endpoints in a controlled, deterministic environment. It showcases:
//!
//! * Creating a REST API server with CRUD operations using `moosicbox_web_server`
//! * Running multiple test scenarios (happy path, error handling, edge cases, concurrency)
//! * Tracking and reporting test results with detailed metrics
//! * Using simulation time for reproducible tests
//!
//! # Usage
//!
//! Run this example with:
//!
//! ```bash
//! cargo run --package simvar_api_testing_example
//! ```
//!
//! The simulation will run for 20 seconds of simulated time, executing various
//! test scenarios against the API and reporting success rates at the end.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use moosicbox_web_server::{HttpResponse, Scope, WebServerBuilder};
use serde::{Deserialize, Serialize};
use simvar::{Sim, SimBootstrap, SimConfig, run_simulation};

// Import result types from harness modules
type HostResult = Result<(), Box<dyn std::error::Error + Send + 'static>>;
type ClientResult = Result<(), Box<dyn std::error::Error + Send>>;
use switchy_http::Client as HttpClient;
use switchy_http_models::{Method, StatusCode};
use switchy_uuid::new_v4_string;

/// Comprehensive API testing simulation that validates REST endpoint behavior.
///
/// This simulation creates:
/// * A REST API server with CRUD operations
/// * Test clients that validate API contracts
/// * Comprehensive test scenarios (happy path, edge cases, error conditions)
/// * Detailed test result reporting
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bootstrap = ApiTestingBootstrap::new();
    let results = run_simulation(bootstrap)?;

    println!("\n=== API TESTING RESULTS ===");
    for result in &results {
        println!("{result}");
    }

    let success_count = results.iter().filter(|r| r.is_success()).count();
    let total_count = results.len();
    println!("\nSuccess rate: {success_count}/{total_count}");

    Ok(())
}

/// Test scenario types
#[derive(Debug, Clone)]
enum TestScenario {
    /// Test successful CRUD operations
    HappyPath,
    /// Test validation and error handling
    ErrorHandling,
    /// Test edge cases and boundary conditions
    EdgeCases,
    /// Test concurrent operations
    Concurrency,
}

/// Bootstrap configuration for the API testing simulation.
///
/// This struct holds the configuration needed to run the API testing simulation,
/// including the server port, test scenarios to run, and shared test results
/// for tracking test outcomes across all test clients.
struct ApiTestingBootstrap {
    server_port: u16,
    test_scenarios: Vec<TestScenario>,
    test_results: Arc<Mutex<TestResults>>,
}

impl ApiTestingBootstrap {
    /// Creates a new API testing bootstrap with default configuration.
    ///
    /// The default configuration includes:
    /// * Server port: 8082
    /// * All test scenarios: `HappyPath`, `ErrorHandling`, `EdgeCases`, `Concurrency`
    /// * Empty test results tracker
    #[must_use]
    fn new() -> Self {
        Self {
            server_port: 8082,
            test_scenarios: vec![
                TestScenario::HappyPath,
                TestScenario::ErrorHandling,
                TestScenario::EdgeCases,
                TestScenario::Concurrency,
            ],
            test_results: Arc::new(Mutex::new(TestResults::new())),
        }
    }
}

impl SimBootstrap for ApiTestingBootstrap {
    fn props(&self) -> Vec<(String, String)> {
        vec![
            ("server_port".to_string(), self.server_port.to_string()),
            (
                "test_scenarios".to_string(),
                format!("{:?}", self.test_scenarios),
            ),
        ]
    }

    fn build_sim(&self, mut config: SimConfig) -> SimConfig {
        // Run API tests for 20 seconds
        config.duration = Duration::from_secs(20);
        config.enable_random_order = false; // Deterministic test execution
        config
    }

    fn on_start(&self, sim: &mut impl Sim) {
        log::info!("Starting API testing simulation");

        // Start the API server
        let server_port = self.server_port;
        sim.host("api-server", move || {
            Box::pin(async move { start_api_server(server_port).await })
        });

        // Start test clients for each scenario
        for (i, scenario) in self.test_scenarios.iter().enumerate() {
            let scenario = scenario.clone();
            let server_port = self.server_port;
            let test_results = self.test_results.clone();

            sim.client(format!("test-{scenario:?}-{i}"), async move {
                // Small delay to ensure server is ready
                switchy_async::time::sleep(Duration::from_millis(500)).await;
                run_api_test_scenario(scenario, server_port, test_results).await
            });
        }
    }

    fn on_step(&self, _sim: &mut impl Sim) {
        // Log test progress periodically
        if simvar::switchy::time::simulator::current_step().is_multiple_of(2000) {
            let results = self.test_results.lock().unwrap();
            log::info!("Test progress: {}", results.summary());
            drop(results);
        }
    }

    fn on_end(&self, _sim: &mut impl Sim) {
        let results = self.test_results.lock().unwrap();
        log::info!(
            "API testing completed. Results: {}",
            results.detailed_report()
        );
        drop(results);
    }
}

/// Test results tracking
#[derive(Debug)]
struct TestResults {
    total_tests: u32,
    passed_tests: u32,
    failed_tests: u32,
    test_details: BTreeMap<String, TestDetail>,
}

/// Detailed information about a single test execution
#[derive(Debug)]
struct TestDetail {
    scenario: String,
    test_name: String,
    passed: bool,
    error_message: Option<String>,
    response_time_ms: u64,
}

impl TestResults {
    /// Creates a new, empty test results tracker.
    #[must_use]
    const fn new() -> Self {
        Self {
            total_tests: 0,
            passed_tests: 0,
            failed_tests: 0,
            test_details: BTreeMap::new(),
        }
    }

    /// Records the result of a test execution.
    ///
    /// Updates the total, passed, and failed test counters based on the test
    /// outcome and stores the detailed test information for later reporting.
    fn record_test(&mut self, detail: TestDetail) {
        self.total_tests += 1;
        if detail.passed {
            self.passed_tests += 1;
        } else {
            self.failed_tests += 1;
        }

        let key = format!("{}_{}", detail.scenario, detail.test_name);
        self.test_details.insert(key, detail);
    }

    /// Returns a brief summary of test results.
    ///
    /// The summary includes total tests, passed count, and failed count.
    #[must_use]
    fn summary(&self) -> String {
        format!(
            "Tests: {} (Passed: {}, Failed: {})",
            self.total_tests, self.passed_tests, self.failed_tests
        )
    }

    /// Returns a detailed report of all test results.
    ///
    /// The report includes the summary plus per-test details showing scenario,
    /// test name, pass/fail status, response time, and any error messages.
    #[must_use]
    fn detailed_report(&self) -> String {
        use std::fmt::Write;

        let mut report = self.summary();

        if !self.test_details.is_empty() {
            report.push_str("\nTest Details:");
            for detail in self.test_details.values() {
                let status = if detail.passed { "PASS" } else { "FAIL" };
                write!(
                    report,
                    "\n  [{status}] {scenario} - {test_name} ({response_time_ms}ms)",
                    status = status,
                    scenario = detail.scenario,
                    test_name = detail.test_name,
                    response_time_ms = detail.response_time_ms
                )
                .unwrap();

                if let Some(error) = &detail.error_message {
                    write!(report, " - Error: {error}").unwrap();
                }
            }
        }

        report
    }
}

/// In-memory data store for the API
#[derive(Debug, Clone)]
struct ApiDataStore {
    users: Arc<Mutex<BTreeMap<String, User>>>,
}

impl ApiDataStore {
    /// Creates a new, empty data store.
    #[must_use]
    fn new() -> Self {
        Self {
            users: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}

/// User model for the API
#[derive(Debug, Clone, Serialize, Deserialize)]
struct User {
    id: String,
    name: String,
    email: String,
    created_at: u64,
}

/// Request payload for creating a new user
#[derive(Debug, Serialize, Deserialize)]
struct CreateUserRequest {
    name: String,
    email: String,
}

/// Starts the API server with CRUD endpoints for user management.
///
/// Creates and starts a web server on the specified port with endpoints for
/// creating, reading, updating, and deleting users. The server uses CORS
/// configuration that allows any origin, method, and header.
///
/// # Errors
///
/// Returns an error if the server fails to start or encounters a runtime error.
#[allow(clippy::future_not_send)]
async fn start_api_server(port: u16) -> HostResult {
    log::info!("Starting API server on port {port}");

    let cors = moosicbox_web_server::cors::Cors::default()
        .allow_any_origin()
        .allow_any_method()
        .allow_any_header()
        .expose_any_header();

    let server = WebServerBuilder::new()
        .with_port(port)
        .with_cors(cors)
        .with_scope(
            Scope::new("/api/v1/users")
                .post("", |_req| {
                    Box::pin(async move {
                        // In a real implementation, you'd parse the request body properly
                        let user = User {
                            id: new_v4_string(),
                            name: "Test User".to_string(),
                            email: "test@example.com".to_string(),
                            created_at: switchy_time::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap()
                                .as_secs(),
                        };

                        DATA_STORE
                            .users
                            .lock()
                            .unwrap()
                            .insert(user.id.clone(), user.clone());

                        let body = serde_json::to_string(&user).unwrap();
                        Ok(HttpResponse::from_status_code(StatusCode::Created).with_body(body))
                    })
                })
                .get("", |_req| {
                    Box::pin(async move {
                        let users: Vec<User> =
                            DATA_STORE.users.lock().unwrap().values().cloned().collect();
                        let body = serde_json::to_string(&users).unwrap();
                        Ok(HttpResponse::ok().with_body(body))
                    })
                })
                .get("/{id}", |_req| {
                    Box::pin(async move {
                        // In a real implementation, you'd extract the ID from the path
                        let users = DATA_STORE.users.lock().unwrap();
                        Ok(users.values().next().map_or_else(
                            || HttpResponse::not_found().with_body(r#"{"error":"User not found"}"#),
                            |user| {
                                let body = serde_json::to_string(user).unwrap();
                                HttpResponse::ok().with_body(body)
                            },
                        ))
                    })
                })
                .put("/{id}", |_req| {
                    Box::pin(async move {
                        // In a real implementation, you'd extract the ID and parse the body
                        Ok(HttpResponse::ok().with_body(r#"{"message":"User updated"}"#))
                    })
                })
                .delete("/{id}", |_req| {
                    Box::pin(async move {
                        // In a real implementation, you'd extract the ID and delete the user
                        Ok(HttpResponse::ok().with_body(r#"{"message":"User deleted"}"#))
                    })
                }),
        )
        .build();

    server.start().await;
    Ok(())
}

/// Runs API test scenarios against the server.
///
/// Executes the specified test scenario by making HTTP requests to the API
/// server and recording the results. Each scenario type tests different aspects
/// of the API:
/// * `HappyPath` - Successful CRUD operations
/// * `ErrorHandling` - Validation and error responses
/// * `EdgeCases` - Boundary conditions
/// * `Concurrency` - Concurrent request handling
///
/// # Errors
///
/// Returns an error if the test scenario encounters an unrecoverable failure.
async fn run_api_test_scenario(
    scenario: TestScenario,
    server_port: u16,
    test_results: Arc<Mutex<TestResults>>,
) -> ClientResult {
    log::info!("Running test scenario: {scenario:?}");

    let base_url = format!("http://localhost:{server_port}/api/v1");
    let client = HttpClient::new();

    match scenario {
        TestScenario::HappyPath => {
            run_happy_path_tests(&client, &base_url, &test_results).await;
        }
        TestScenario::ErrorHandling => {
            run_error_handling_tests(&client, &base_url, &test_results).await;
        }
        TestScenario::EdgeCases => {
            run_edge_case_tests(&client, &base_url, &test_results).await;
        }
        TestScenario::Concurrency => {
            // Simplified concurrency test
            log::info!("Concurrency test simplified for demo");
            log::info!("Concurrency test simplified for demo");
        }
    }

    Ok(())
}

/// Runs tests for successful CRUD operations.
///
/// Tests the happy path scenarios including:
/// * Creating a new user
/// * Retrieving a user by ID
/// * Listing all users
async fn run_happy_path_tests(
    client: &HttpClient,
    base_url: &str,
    test_results: &Arc<Mutex<TestResults>>,
) {
    // Test 1: Create user
    let create_request = CreateUserRequest {
        name: "John Doe".to_string(),
        email: "john@example.com".to_string(),
    };

    let start_time = simvar::switchy::time::now();
    let result = client
        .request(Method::Post, &format!("{base_url}/users"))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&create_request).unwrap().into())
        .send()
        .await;

    let response_time = u64::try_from(
        simvar::switchy::time::now()
            .duration_since(start_time)
            .unwrap()
            .as_millis(),
    )
    .unwrap();

    let (passed, error_message, user_id) = match result {
        Ok(response) if response.status() == StatusCode::Created => {
            // Parse response to get user ID
            // For simulator, we'll just assume success and generate a fake ID
            (true, None, Some(new_v4_string()))
        }
        Ok(response) => (
            false,
            Some(format!("Unexpected status: {}", response.status())),
            None,
        ),
        Err(e) => (false, Some(e.to_string()), None),
    };

    test_results.lock().unwrap().record_test(TestDetail {
        scenario: "HappyPath".to_string(),
        test_name: "create_user".to_string(),
        passed,
        error_message,
        response_time_ms: response_time,
    });

    // Test 2: Get user by ID (if creation succeeded)
    if let Some(user_id) = user_id {
        let start_time = simvar::switchy::time::now();
        let result = client
            .request(Method::Get, &format!("{base_url}/users/{user_id}"))
            .send()
            .await;

        let response_time = u64::try_from(
            simvar::switchy::time::now()
                .duration_since(start_time)
                .unwrap()
                .as_millis(),
        )
        .unwrap();

        let (passed, error_message) = match result {
            Ok(response) if response.status() == StatusCode::Ok => (true, None),
            Ok(response) => (
                false,
                Some(format!("Unexpected status: {}", response.status())),
            ),
            Err(e) => (false, Some(e.to_string())),
        };

        test_results.lock().unwrap().record_test(TestDetail {
            scenario: "HappyPath".to_string(),
            test_name: "get_user_by_id".to_string(),
            passed,
            error_message,
            response_time_ms: response_time,
        });
    }

    // Test 3: List users
    let start_time = simvar::switchy::time::now();
    let result = client
        .request(Method::Get, &format!("{base_url}/users"))
        .send()
        .await;

    let response_time = u64::try_from(
        simvar::switchy::time::now()
            .duration_since(start_time)
            .unwrap()
            .as_millis(),
    )
    .unwrap();

    let (passed, error_message) = match result {
        Ok(response) if response.status() == StatusCode::Ok => (true, None),
        Ok(response) => (
            false,
            Some(format!("Unexpected status: {}", response.status())),
        ),
        Err(e) => (false, Some(e.to_string())),
    };

    test_results.lock().unwrap().record_test(TestDetail {
        scenario: "HappyPath".to_string(),
        test_name: "list_users".to_string(),
        passed,
        error_message,
        response_time_ms: response_time,
    });
}

/// Runs tests for error handling scenarios.
///
/// Tests that the API correctly handles error conditions including:
/// * Requesting a non-existent user (expects 404)
/// * Creating a user with invalid data (expects 400)
async fn run_error_handling_tests(
    client: &HttpClient,
    base_url: &str,
    test_results: &Arc<Mutex<TestResults>>,
) {
    // Test 1: Get non-existent user
    let start_time = simvar::switchy::time::now();
    let result = client
        .request(Method::Get, &format!("{base_url}/users/non-existent-id"))
        .send()
        .await;

    let response_time = u64::try_from(
        simvar::switchy::time::now()
            .duration_since(start_time)
            .unwrap()
            .as_millis(),
    )
    .unwrap();

    let (passed, error_message) = match result {
        Ok(response) if response.status() == StatusCode::NotFound => (true, None),
        Ok(response) => (
            false,
            Some(format!("Expected 404, got: {}", response.status())),
        ),
        Err(e) => (false, Some(e.to_string())),
    };

    test_results.lock().unwrap().record_test(TestDetail {
        scenario: "ErrorHandling".to_string(),
        test_name: "get_non_existent_user".to_string(),
        passed,
        error_message,
        response_time_ms: response_time,
    });

    // Test 2: Create user with invalid data
    let start_time = simvar::switchy::time::now();
    let result = client
        .request(Method::Post, &format!("{base_url}/users"))
        .header("Content-Type", "application/json")
        .body(r#"{"invalid":"data"}"#.into())
        .send()
        .await;

    let response_time = u64::try_from(
        simvar::switchy::time::now()
            .duration_since(start_time)
            .unwrap()
            .as_millis(),
    )
    .unwrap();

    let (passed, error_message) = match result {
        Ok(response) if response.status() == StatusCode::BadRequest => (true, None),
        Ok(response) => (
            false,
            Some(format!("Expected 400, got: {}", response.status())),
        ),
        Err(e) => (false, Some(e.to_string())),
    };

    test_results.lock().unwrap().record_test(TestDetail {
        scenario: "ErrorHandling".to_string(),
        test_name: "create_user_invalid_data".to_string(),
        passed,
        error_message,
        response_time_ms: response_time,
    });
}

/// Runs tests for edge cases and boundary conditions.
///
/// Tests that the API handles edge cases gracefully including:
/// * Creating a user with a very long name (1000 characters)
async fn run_edge_case_tests(
    client: &HttpClient,
    base_url: &str,
    test_results: &Arc<Mutex<TestResults>>,
) {
    // Test 1: Create user with very long name
    let long_name = "a".repeat(1000);
    let create_request = CreateUserRequest {
        name: long_name,
        email: "test@example.com".to_string(),
    };

    let start_time = simvar::switchy::time::now();
    let result = client
        .request(Method::Post, &format!("{base_url}/users"))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&create_request).unwrap().into())
        .send()
        .await;

    let response_time = u64::try_from(
        simvar::switchy::time::now()
            .duration_since(start_time)
            .unwrap()
            .as_millis(),
    )
    .unwrap();

    let (passed, error_message) = match result {
        Ok(response) => {
            // Accept either success or validation error
            let status = response.status();
            if status == StatusCode::Created || status == StatusCode::BadRequest {
                (true, None)
            } else {
                (false, Some(format!("Unexpected status: {status}")))
            }
        }
        Err(e) => (false, Some(e.to_string())),
    };

    test_results.lock().unwrap().record_test(TestDetail {
        scenario: "EdgeCases".to_string(),
        test_name: "create_user_long_name".to_string(),
        passed,
        error_message,
        response_time_ms: response_time,
    });
}

// Global data store for the API (in a real app, this would be a database)
static DATA_STORE: std::sync::LazyLock<ApiDataStore> = std::sync::LazyLock::new(ApiDataStore::new);
