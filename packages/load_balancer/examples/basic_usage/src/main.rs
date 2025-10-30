#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions, clippy::similar_names)]

//! Basic usage example for `moosicbox_load_balancer`
//!
//! This example demonstrates how to set up a simple load balancer with
//! hostname-based routing and multiple upstream servers.

use std::collections::BTreeMap;

use moosicbox_load_balancer::Router;
use pingora::{listeners::tls::TlsSettings, prelude::*};
use pingora_core::services::{background::GenBackgroundService, listening::Service};
use pingora_load_balancing::{LoadBalancer, health_check::TcpHealthCheck, selection::RoundRobin};
use pingora_proxy::{HttpProxy, http_proxy_service};

fn main() {
    // Initialize logging so we can see what's happening
    env_logger::init();

    println!("Starting MoosicBox Load Balancer Example");
    println!("=========================================\n");

    // Step 1: Create the Pingora server instance
    println!("Step 1: Creating Pingora server instance...");
    let mut pingora_server = Server::new(None).expect("Failed to create server");
    pingora_server.bootstrap();

    // Step 2: Define upstream server addresses
    // In a real scenario, these would be your actual backend servers
    println!("Step 2: Defining upstream servers...");
    let upstreams = vec!["127.0.0.1:8001", "127.0.0.1:8002"];
    println!("  Upstreams: {upstreams:?}");

    // Step 3: Create a load balancer with health checking
    println!("\nStep 3: Creating load balancer with round-robin selection...");
    let mut lb =
        LoadBalancer::try_from_iter(upstreams.as_slice()).expect("Failed to create load balancer");

    // Step 4: Configure health checks
    println!("Step 4: Configuring TCP health checks...");
    let health_check = TcpHealthCheck::new();
    lb.set_health_check(health_check);
    lb.health_check_frequency = Some(std::time::Duration::from_secs(10));
    println!("  Health check frequency: 10 seconds");

    // Step 5: Create background service for health checking
    println!("\nStep 5: Creating background health check service...");
    let health_check_service: GenBackgroundService<LoadBalancer<RoundRobin>> =
        background_service("health check", lb);

    // Step 7: Get the Arc from the background service for routing
    let lb_arc = health_check_service.task();

    // Step 8: Create the router mapping
    println!("Step 6: Setting up hostname-based routing...");
    let mut router_map = BTreeMap::new();

    // Map specific hostname to load balancer
    router_map.insert("api.example.com".to_string(), lb_arc.clone());
    println!("  Route: api.example.com -> upstreams");

    // Add wildcard fallback for all other hostnames
    router_map.insert("*".to_string(), lb_arc);
    println!("  Route: * (fallback) -> upstreams");

    // Create the router
    let router = Router::new(router_map);

    // Step 7: Create the HTTP proxy service
    println!("\nStep 7: Creating HTTP proxy service...");
    let mut proxy_service = http_proxy_service(&pingora_server.configuration, router);

    // Step 8: Configure listening addresses
    let http_port = 6188;
    let https_port = 6189;
    let http_addr = format!("0.0.0.0:{http_port}");

    println!("\nStep 8: Configuring listening addresses...");
    proxy_service.add_tcp(&http_addr);
    println!("  HTTP: {http_addr}");

    // Optional: Add HTTPS support if certificates are available
    // In this example, we'll show the configuration but skip if certs don't exist
    setup_tls(&mut proxy_service, https_port);

    // Step 9: Add services to the server
    println!("\nStep 9: Adding services to server...");
    pingora_server.add_service(health_check_service);
    pingora_server.add_service(proxy_service);

    // Step 10: Start the server
    println!("\nStep 10: Starting load balancer...");
    println!("\n┌─────────────────────────────────────────────┐");
    println!("│  Load Balancer is now running!              │");
    println!("├─────────────────────────────────────────────┤");
    println!("│  HTTP:  http://localhost:{http_port:<18}│");
    println!("│  HTTPS: https://localhost:{https_port:<17}│");
    println!("├─────────────────────────────────────────────┤");
    println!("│  Upstream servers:                          │");
    for upstream in &upstreams {
        println!("│    - {upstream:<36} │");
    }
    println!("└─────────────────────────────────────────────┘");
    println!("\nPress Ctrl+C to stop the server\n");

    // Run forever
    pingora_server.run_forever();
}

/// Helper function to set up TLS/HTTPS support
///
/// This checks for certificate files and configures HTTPS if they exist.
fn setup_tls(proxy_service: &mut Service<HttpProxy<Router>>, tls_port: u16) {
    // For this example, we use self-signed certs in the example directory if they exist
    let cert_path = "cert.pem";
    let key_path = "key.pem";

    if std::path::Path::new(cert_path).exists() && std::path::Path::new(key_path).exists() {
        println!("  Found TLS certificates, enabling HTTPS...");
        let mut tls_settings = TlsSettings::intermediate(cert_path, key_path)
            .expect("Failed to load TLS certificates");
        tls_settings.enable_h2();

        let tls_addr = format!("0.0.0.0:{tls_port}");
        proxy_service.add_tls_with_settings(&tls_addr, None, tls_settings);
        println!("  HTTPS: {tls_addr}");
    } else {
        println!("  No TLS certificates found (cert.pem/key.pem), HTTPS disabled");
        println!("  To enable HTTPS, generate certificates with:");
        println!(
            "    openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes"
        );
    }
}
