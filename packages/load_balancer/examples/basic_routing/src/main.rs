//! Basic hostname-based routing example using the `moosicbox_load_balancer` crate.
//!
//! This example demonstrates how to create a `Router` that routes requests to different
//! backend servers based on the request's hostname. It shows the core functionality of
//! the load balancer's routing system without running a full server.
//!
//! # Usage
//!
//! ```bash
//! cargo run --manifest-path packages/load_balancer/examples/basic_routing/Cargo.toml
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{collections::BTreeMap, sync::Arc};

use moosicbox_load_balancer::Router;
use pingora_load_balancing::LoadBalancer;

/// Errors that can occur in the basic routing example.
#[derive(Debug)]
pub enum Error {
    /// Failed to create load balancer from upstream addresses.
    LoadBalancerCreation(std::io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LoadBalancerCreation(e) => write!(f, "LoadBalancer creation failed: {e}"),
        }
    }
}

impl std::error::Error for Error {}

fn main() -> Result<(), Error> {
    println!("=== MoosicBox Load Balancer - Basic Routing Example ===\n");

    // Step 1: Define backend servers for the API cluster
    println!("Step 1: Creating API cluster with backend servers");
    let api_upstreams = vec!["192.168.1.10:8080", "192.168.1.11:8080"];
    println!("  API backends: {api_upstreams:?}");

    let api_lb =
        LoadBalancer::try_from_iter(&api_upstreams).map_err(Error::LoadBalancerCreation)?;
    let api_lb = Arc::new(api_lb);

    // Step 2: Define backend servers for the web cluster
    println!("\nStep 2: Creating web cluster with backend servers");
    let web_upstreams = vec!["192.168.1.20:80", "192.168.1.21:80"];
    println!("  Web backends: {web_upstreams:?}");

    let web_lb =
        LoadBalancer::try_from_iter(&web_upstreams).map_err(Error::LoadBalancerCreation)?;
    let web_lb = Arc::new(web_lb);

    // Step 3: Define backend servers for the default/fallback cluster
    println!("\nStep 3: Creating default fallback cluster");
    let default_upstreams = vec!["192.168.1.100:8080"];
    println!("  Default backends: {default_upstreams:?}");

    let default_lb =
        LoadBalancer::try_from_iter(&default_upstreams).map_err(Error::LoadBalancerCreation)?;
    let default_lb = Arc::new(default_lb);

    // Step 4: Create the routing map
    println!("\nStep 4: Building hostname-to-cluster routing map");
    let mut routing_map = BTreeMap::new();

    // Route api.example.com to the API cluster
    routing_map.insert("api.example.com".to_string(), api_lb);
    println!("  api.example.com -> API cluster");

    // Route www.example.com to the web cluster
    routing_map.insert("www.example.com".to_string(), web_lb);
    println!("  www.example.com -> Web cluster");

    // Use "*" as a wildcard fallback for any unmatched hostname
    routing_map.insert("*".to_string(), default_lb);
    println!("  * (wildcard) -> Default cluster");

    // Step 5: Create the Router
    println!("\nStep 5: Creating Router with routing configuration");
    let _router = Router::new(routing_map);
    println!("  ✓ Router created successfully");

    // Step 6: Demonstrate the router structure
    println!("\n=== Router Configuration Summary ===");
    println!("The router is now configured to route requests as follows:");
    println!("  • Requests to 'api.example.com' → 192.168.1.10:8080, 192.168.1.11:8080");
    println!("  • Requests to 'www.example.com' → 192.168.1.20:80, 192.168.1.21:80");
    println!("  • Requests to any other hostname → 192.168.1.100:8080 (fallback)");
    println!(
        "  • ACME challenge requests (/.well-known/acme-challenge/*) → 'solver' cluster (if configured)"
    );

    println!("\n=== Key Concepts ===");
    println!("1. LoadBalancer: Manages a set of upstream servers with round-robin selection");
    println!("2. Router: Maps hostnames to LoadBalancer instances for request routing");
    println!("3. Wildcard (*): Catches all unmatched hostnames as a fallback");
    println!("4. BTreeMap: Ensures consistent ordering of hostname matching");

    println!("\n=== Next Steps ===");
    println!("To use this router in a production server:");
    println!("1. Add TCP health checks to each LoadBalancer (see TcpHealthCheck)");
    println!("2. Configure the Pingora server with the Router as the ProxyHttp handler");
    println!("3. Set up TLS certificates for HTTPS support");
    println!("4. Configure environment variables (PORT, SSL_PORT, CLUSTERS)");
    println!("5. Run the server with proper logging and monitoring");

    println!("\n✓ Example completed successfully!");

    Ok(())
}
