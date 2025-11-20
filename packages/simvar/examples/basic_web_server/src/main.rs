//! Basic web server simulation example using `simvar` and `moosicbox_web_server`.
//!
//! This example demonstrates how to use the `simvar` simulation framework to test
//! a web server application with multiple concurrent clients. The simulation includes:
//!
//! * A web server host that serves HTTP requests on multiple endpoints
//! * Multiple client actors that make periodic requests to the server
//! * Automatic metrics collection and reporting
//!
//! # Example Endpoints
//!
//! The web server provides three endpoints:
//!
//! * `GET /api/v1/health` - Health check endpoint
//! * `GET /api/v1/status` - Server status with uptime information
//! * `POST /api/v1/echo` - Echo endpoint that returns the request with server timestamp
//!
//! # Running the Example
//!
//! ```bash
//! cargo run --package simvar_basic_web_server_example
//! ```
//!
//! The simulation runs for 10 seconds with 3 concurrent clients making requests
//! at 500ms intervals.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::time::Duration;

use moosicbox_web_server::{HttpResponse, Scope, WebServerBuilder};
use serde::{Deserialize, Serialize};
use simvar::{
    Sim, SimBootstrap, SimConfig, client::ClientResult, host::HostResult, run_simulation,
};
use switchy_http::Client as HttpClient;
use switchy_http_models::Method;

// // Import result types from harness modules
// type HostResult = Result<(), Box<dyn std::error::Error + 'static>>;
// type ClientResult = Result<(), Box<dyn std::error::Error>>;

/// Example demonstrating a basic web server simulation using simvar and `moosicbox_web_server`.
///
/// This simulation creates:
/// * A web server host that serves HTTP requests
/// * Multiple client actors that make requests to the server
/// * Metrics collection and reporting
///
/// # Errors
///
/// Returns an error if:
/// * The simulation fails to initialize or run
/// * An unrecoverable error occurs during the simulation
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bootstrap = BasicWebServerBootstrap::new();
    let results = run_simulation(bootstrap)?;

    println!("\n=== SIMULATION RESULTS ===");
    for result in &results {
        println!("{result}");
    }

    let success_count = results.iter().filter(|r| r.is_success()).count();
    let total_count = results.len();
    println!("\nSuccess rate: {success_count}/{total_count}");

    Ok(())
}

/// Bootstrap configuration for the basic web server simulation.
///
/// This struct configures the simulation parameters including the server port,
/// number of concurrent clients, and how frequently clients make requests.
struct BasicWebServerBootstrap {
    /// The TCP port on which the web server will listen
    server_port: u16,
    /// The number of concurrent client actors to simulate
    client_count: usize,
    /// The interval between consecutive requests from each client
    request_interval: Duration,
}

impl BasicWebServerBootstrap {
    /// Creates a new `BasicWebServerBootstrap` with default configuration values.
    ///
    /// Default values:
    /// * `server_port`: 8080
    /// * `client_count`: 3
    /// * `request_interval`: 500ms
    #[must_use]
    const fn new() -> Self {
        Self {
            server_port: 8080,
            client_count: 3,
            request_interval: Duration::from_millis(500),
        }
    }
}

impl SimBootstrap for BasicWebServerBootstrap {
    /// Returns the simulation properties as key-value pairs for reporting.
    ///
    /// Includes server port, client count, and request interval in milliseconds.
    fn props(&self) -> Vec<(String, String)> {
        vec![
            ("server_port".to_string(), self.server_port.to_string()),
            ("client_count".to_string(), self.client_count.to_string()),
            (
                "request_interval_ms".to_string(),
                self.request_interval.as_millis().to_string(),
            ),
        ]
    }

    /// Configures the simulation settings.
    ///
    /// Sets the simulation duration to 10 seconds and enables random ordering
    /// of concurrent events.
    fn build_sim(&self, mut config: SimConfig) -> SimConfig {
        // Run simulation for 10 seconds
        config.duration = Duration::from_secs(10);
        config.enable_random_order = true;
        config
    }

    /// Initializes and starts the simulation actors.
    ///
    /// Creates one web server host and multiple client actors based on the
    /// configured `client_count`.
    fn on_start(&self, sim: &mut impl Sim) {
        log::info!("Starting basic web server simulation");

        // Start the web server host
        let server_port = self.server_port;
        sim.host("web-server", move || {
            Box::pin(async move { start_web_server(server_port).await })
        });

        // Start multiple client actors
        for i in 0..self.client_count {
            let client_id = i + 1;
            let server_port = self.server_port;
            let request_interval = self.request_interval;

            sim.client(format!("client-{client_id}"), async move {
                run_client(client_id, server_port, request_interval).await
            });
        }
    }

    /// Called at each simulation step.
    ///
    /// Currently unused - reserved for future per-step logic.
    fn on_step(&self, _sim: &mut impl Sim) {
        // Optional: Add per-step logic here
    }

    /// Called when the simulation ends.
    ///
    /// Logs the completion of the simulation.
    fn on_end(&self, _sim: &mut impl Sim) {
        log::info!("Basic web server simulation completed");
    }
}

/// Start the web server with example endpoints.
///
/// Creates and starts a web server listening on the specified port with three endpoints:
/// * `GET /api/v1/health` - Returns health status
/// * `GET /api/v1/status` - Returns server status with uptime
/// * `POST /api/v1/echo` - Echoes the request with timestamp
///
/// # Errors
///
/// Currently always returns `Ok(())`. Future implementations may return errors if:
/// * The web server fails to bind to the specified port
/// * The server encounters a fatal runtime error
///
/// # Panics
///
/// Panics if the current time cannot be obtained from the system clock when calling `.unwrap()` on time operations.
#[allow(clippy::future_not_send)]
async fn start_web_server(port: u16) -> HostResult {
    log::info!("Starting web server on port {port}");

    let cors = moosicbox_web_server::cors::Cors::default()
        .allow_any_origin()
        .allow_any_method()
        .allow_any_header()
        .expose_any_header();

    let server = WebServerBuilder::new()
        .with_port(port)
        .with_cors(cors)
        .with_scope(
            Scope::new("/api/v1")
                .get("/health", |_req| {
                    Box::pin(
                        async move { Ok(HttpResponse::ok().with_body(r#"{"status":"healthy"}"#)) },
                    )
                })
                .get("/status", |_req| {
                    Box::pin(async move {
                        let uptime = switchy_time::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();

                        let status = StatusResponse {
                            status: "running".to_string(),
                            uptime_seconds: uptime,
                            requests_served: 42, // In a real app, you'd track this
                        };

                        let body = serde_json::to_string(&status).unwrap();
                        Ok(HttpResponse::ok().with_body(body))
                    })
                })
                .post("/echo", |_req| {
                    Box::pin(async move {
                        // In a real implementation, you'd parse the request body
                        let server_time = switchy_time::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();

                        let response = EchoResponse {
                            echo: "Message received".to_string(),
                            received_at: server_time,
                            server_time,
                        };

                        let body = serde_json::to_string(&response).unwrap();
                        Ok(HttpResponse::ok().with_body(body))
                    })
                }),
        )
        .build();

    // In a real simulation, you might want to handle graceful shutdown
    server.start().await;

    Ok(())
}

/// Run a client that makes periodic requests to the server.
///
/// The client makes three types of requests in rotation:
/// * Health check (`GET /api/v1/health`)
/// * Status request (`GET /api/v1/status`)
/// * Echo request (`POST /api/v1/echo`)
///
/// The client continues making requests until the simulation is cancelled.
///
/// # Errors
///
/// Currently always returns `Ok(())`. HTTP request failures are logged as warnings
/// but do not cause the client to fail. Future implementations may return errors if:
/// * The client encounters an unrecoverable error
/// * Required resources become unavailable
///
/// # Panics
///
/// Panics if:
/// * The current time cannot be obtained from the system clock when calling `.unwrap()`
/// * JSON serialization of the echo request fails when calling `.unwrap()`
async fn run_client(
    client_id: usize,
    server_port: u16,
    request_interval: Duration,
) -> ClientResult {
    log::info!("Starting client {client_id}");

    let base_url = format!("http://localhost:{server_port}");
    let client = HttpClient::new();
    let mut request_count = 0;

    loop {
        if simvar::utils::is_simulator_cancelled() {
            break;
        }

        request_count += 1;

        // Make different types of requests
        match request_count % 3 {
            0 => {
                // Health check
                let url = format!("{base_url}/api/v1/health");
                match client.request(Method::Get, &url).send().await {
                    Ok(response) => {
                        log::debug!(
                            "Client {client_id}: Health check - Status: {}",
                            response.status()
                        );
                    }
                    Err(e) => {
                        log::warn!("Client {client_id}: Health check failed: {e}");
                    }
                }
            }
            1 => {
                // Status request
                let url = format!("{base_url}/api/v1/status");
                match client.request(Method::Get, &url).send().await {
                    Ok(response) => {
                        log::debug!(
                            "Client {client_id}: Status check - Status: {}",
                            response.status()
                        );
                    }
                    Err(e) => {
                        log::warn!("Client {client_id}: Status check failed: {e}");
                    }
                }
            }
            _ => {
                // Echo request
                let echo_data = EchoRequest {
                    message: format!("Hello from client {client_id}, request {request_count}"),
                    timestamp: switchy_time::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                };

                let url = format!("{base_url}/api/v1/echo");
                let body = serde_json::to_string(&echo_data).unwrap();

                match client
                    .request(Method::Post, &url)
                    .header("Content-Type", "application/json")
                    .body(body.into())
                    .send()
                    .await
                {
                    Ok(response) => {
                        log::debug!(
                            "Client {client_id}: Echo request - Status: {}",
                            response.status()
                        );
                    }
                    Err(e) => {
                        log::warn!("Client {client_id}: Echo request failed: {e}");
                    }
                }
            }
        }

        // Wait before next request
        switchy_async::time::sleep(request_interval).await;
    }

    log::info!("Client {client_id} completed {request_count} requests");
    Ok(())
}

/// Request payload for the echo endpoint.
///
/// Sent by clients to the `POST /api/v1/echo` endpoint.
#[derive(Debug, Serialize, Deserialize)]
struct EchoRequest {
    /// The message to be echoed back
    message: String,
    /// Unix timestamp (seconds since epoch) when the request was created
    timestamp: u64,
}

/// Response payload from the echo endpoint.
///
/// Returned by the server in response to `POST /api/v1/echo` requests.
#[derive(Debug, Serialize, Deserialize)]
struct EchoResponse {
    /// The echoed message content
    echo: String,
    /// Unix timestamp (seconds since epoch) when the server received the request
    received_at: u64,
    /// Unix timestamp (seconds since epoch) of the server's current time
    server_time: u64,
}

/// Response payload for the status endpoint.
///
/// Returned by the server in response to `GET /api/v1/status` requests.
#[derive(Debug, Serialize, Deserialize)]
struct StatusResponse {
    /// The current status of the server (e.g., "running")
    status: String,
    /// Number of seconds the server has been running
    uptime_seconds: u64,
    /// Total number of requests served since server start
    requests_served: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use simvar::SimConfig;
    use std::time::Duration;

    #[test]
    fn test_bootstrap_new_has_correct_defaults() {
        let bootstrap = BasicWebServerBootstrap::new();

        assert_eq!(
            bootstrap.server_port, 8080,
            "Default server port should be 8080"
        );
        assert_eq!(
            bootstrap.client_count, 3,
            "Default client count should be 3"
        );
        assert_eq!(
            bootstrap.request_interval,
            Duration::from_millis(500),
            "Default request interval should be 500ms"
        );
    }

    #[test]
    fn test_bootstrap_props_contains_all_config_values() {
        let bootstrap = BasicWebServerBootstrap::new();
        let props = bootstrap.props();

        // Verify we have exactly 3 properties
        assert_eq!(props.len(), 3, "Should have exactly 3 properties");

        // Convert to map for easier testing
        let props_map: std::collections::BTreeMap<_, _> = props.into_iter().collect();

        assert_eq!(
            props_map.get("server_port"),
            Some(&"8080".to_string()),
            "server_port property should be present and correct"
        );
        assert_eq!(
            props_map.get("client_count"),
            Some(&"3".to_string()),
            "client_count property should be present and correct"
        );
        assert_eq!(
            props_map.get("request_interval_ms"),
            Some(&"500".to_string()),
            "request_interval_ms property should be present and correct"
        );
    }

    #[test]
    fn test_bootstrap_build_sim_sets_duration() {
        let bootstrap = BasicWebServerBootstrap::new();
        let config = SimConfig::default();

        let updated_config = bootstrap.build_sim(config);

        assert_eq!(
            updated_config.duration,
            Duration::from_secs(10),
            "Simulation duration should be set to 10 seconds"
        );
    }

    #[test]
    fn test_bootstrap_build_sim_enables_random_order() {
        let bootstrap = BasicWebServerBootstrap::new();
        let config = SimConfig::default();

        let updated_config = bootstrap.build_sim(config);

        assert!(
            updated_config.enable_random_order,
            "Random order should be enabled"
        );
    }

    #[test]
    fn test_echo_request_serialization() {
        let request = EchoRequest {
            message: "test message".to_string(),
            timestamp: 1_234_567_890,
        };

        let json = serde_json::to_string(&request).expect("Should serialize EchoRequest");
        let deserialized: EchoRequest =
            serde_json::from_str(&json).expect("Should deserialize EchoRequest");

        assert_eq!(deserialized.message, "test message");
        assert_eq!(deserialized.timestamp, 1_234_567_890);
    }

    #[test]
    fn test_echo_request_with_empty_message() {
        let request = EchoRequest {
            message: String::new(),
            timestamp: 0,
        };

        let json = serde_json::to_string(&request)
            .expect("Should serialize EchoRequest with empty message");
        let deserialized: EchoRequest =
            serde_json::from_str(&json).expect("Should deserialize EchoRequest with empty message");

        assert_eq!(deserialized.message, "");
        assert_eq!(deserialized.timestamp, 0);
    }

    #[test]
    fn test_echo_response_serialization() {
        let response = EchoResponse {
            echo: "response message".to_string(),
            received_at: 1_234_567_890,
            server_time: 1_234_567_900,
        };

        let json = serde_json::to_string(&response).expect("Should serialize EchoResponse");
        let deserialized: EchoResponse =
            serde_json::from_str(&json).expect("Should deserialize EchoResponse");

        assert_eq!(deserialized.echo, "response message");
        assert_eq!(deserialized.received_at, 1_234_567_890);
        assert_eq!(deserialized.server_time, 1_234_567_900);
    }

    #[test]
    fn test_status_response_serialization() {
        let response = StatusResponse {
            status: "running".to_string(),
            uptime_seconds: 3600,
            requests_served: 100,
        };

        let json = serde_json::to_string(&response).expect("Should serialize StatusResponse");
        let deserialized: StatusResponse =
            serde_json::from_str(&json).expect("Should deserialize StatusResponse");

        assert_eq!(deserialized.status, "running");
        assert_eq!(deserialized.uptime_seconds, 3600);
        assert_eq!(deserialized.requests_served, 100);
    }

    #[test]
    fn test_status_response_with_large_values() {
        let response = StatusResponse {
            status: "running".to_string(),
            uptime_seconds: u64::MAX,
            requests_served: u64::MAX,
        };

        let json = serde_json::to_string(&response)
            .expect("Should serialize StatusResponse with large values");
        let deserialized: StatusResponse = serde_json::from_str(&json)
            .expect("Should deserialize StatusResponse with large values");

        assert_eq!(deserialized.status, "running");
        assert_eq!(deserialized.uptime_seconds, u64::MAX);
        assert_eq!(deserialized.requests_served, u64::MAX);
    }

    #[test]
    fn test_echo_request_json_format() {
        let request = EchoRequest {
            message: "test".to_string(),
            timestamp: 123,
        };

        let json = serde_json::to_string(&request).expect("Should serialize EchoRequest");

        // Verify JSON contains expected fields
        assert!(
            json.contains(r#""message""#),
            "JSON should contain message field"
        );
        assert!(
            json.contains(r#""timestamp""#),
            "JSON should contain timestamp field"
        );
        assert!(
            json.contains(r#""test""#),
            "JSON should contain message value"
        );
    }

    #[test]
    fn test_echo_response_json_format() {
        let response = EchoResponse {
            echo: "test".to_string(),
            received_at: 100,
            server_time: 200,
        };

        let json = serde_json::to_string(&response).expect("Should serialize EchoResponse");

        // Verify JSON contains expected fields
        assert!(json.contains(r#""echo""#), "JSON should contain echo field");
        assert!(
            json.contains(r#""received_at""#),
            "JSON should contain received_at field"
        );
        assert!(
            json.contains(r#""server_time""#),
            "JSON should contain server_time field"
        );
    }

    #[test]
    fn test_status_response_json_format() {
        let response = StatusResponse {
            status: "healthy".to_string(),
            uptime_seconds: 60,
            requests_served: 10,
        };

        let json = serde_json::to_string(&response).expect("Should serialize StatusResponse");

        // Verify JSON contains expected fields
        assert!(
            json.contains(r#""status""#),
            "JSON should contain status field"
        );
        assert!(
            json.contains(r#""uptime_seconds""#),
            "JSON should contain uptime_seconds field"
        );
        assert!(
            json.contains(r#""requests_served""#),
            "JSON should contain requests_served field"
        );
    }
}
