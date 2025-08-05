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

/// Bootstrap configuration for the basic web server simulation
struct BasicWebServerBootstrap {
    server_port: u16,
    client_count: usize,
    request_interval: Duration,
}

impl BasicWebServerBootstrap {
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

    fn build_sim(&self, mut config: SimConfig) -> SimConfig {
        // Run simulation for 10 seconds
        config.duration = Duration::from_secs(10);
        config.enable_random_order = true;
        config
    }

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

    fn on_step(&self, _sim: &mut impl Sim) {
        // Optional: Add per-step logic here
    }

    fn on_end(&self, _sim: &mut impl Sim) {
        log::info!("Basic web server simulation completed");
    }
}

/// Start the web server with example endpoints
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
                    Box::pin(async move { Ok(HttpResponse::ok().with_body(r#"{"status":"healthy"}"#)) })
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

/// Run a client that makes periodic requests to the server
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

/// Request/Response types for the echo endpoint
#[derive(Debug, Serialize, Deserialize)]
struct EchoRequest {
    message: String,
    timestamp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct EchoResponse {
    echo: String,
    received_at: u64,
    server_time: u64,
}

#[derive(Debug, Serialize, Deserialize)]
struct StatusResponse {
    status: String,
    uptime_seconds: u64,
    requests_served: u64,
}


