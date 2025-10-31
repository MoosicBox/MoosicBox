# Basic Routing Example

Demonstrates how to set up hostname-based routing with the MoosicBox Load Balancer.

## Summary

This example shows how to create a `Router` that routes incoming requests to different backend server clusters based on the request's hostname. It demonstrates the core routing functionality without running a full server, making it easy to understand the basic concepts.

## What This Example Demonstrates

- Creating `LoadBalancer` instances for different backend server clusters
- Building a hostname-to-cluster routing map using `BTreeMap`
- Configuring the `Router` with multiple hostname mappings
- Using a wildcard (`*`) fallback for unmatched hostnames
- Understanding the relationship between `Router`, `LoadBalancer`, and upstream servers

## Prerequisites

- Basic understanding of load balancing concepts
- Familiarity with hostname-based routing (Host header in HTTP)
- Understanding of Rust's `Arc` and `BTreeMap` types

## Running the Example

```bash
cargo run --manifest-path packages/load_balancer/examples/basic_routing/Cargo.toml
```

Or from the repository root:

```bash
cd packages/load_balancer/examples/basic_routing
cargo run
```

## Expected Output

```
=== MoosicBox Load Balancer - Basic Routing Example ===

Step 1: Creating API cluster with backend servers
  API backends: ["192.168.1.10:8080", "192.168.1.11:8080"]

Step 2: Creating web cluster with backend servers
  Web backends: ["192.168.1.20:80", "192.168.1.21:80"]

Step 3: Creating default fallback cluster
  Default backends: ["192.168.1.100:8080"]

Step 4: Building hostname-to-cluster routing map
  api.example.com -> API cluster
  www.example.com -> Web cluster
  * (wildcard) -> Default cluster

Step 5: Creating Router with routing configuration
  ✓ Router created successfully

=== Router Configuration Summary ===
The router is now configured to route requests as follows:
  • Requests to 'api.example.com' → 192.168.1.10:8080, 192.168.1.11:8080
  • Requests to 'www.example.com' → 192.168.1.20:80, 192.168.1.21:80
  • Requests to any other hostname → 192.168.1.100:8080 (fallback)
  • ACME challenge requests (/.well-known/acme-challenge/*) → 'solver' cluster (if configured)

=== Key Concepts ===
1. LoadBalancer: Manages a set of upstream servers with round-robin selection
2. Router: Maps hostnames to LoadBalancer instances for request routing
3. Wildcard (*): Catches all unmatched hostnames as a fallback
4. BTreeMap: Ensures consistent ordering of hostname matching

✓ Example completed successfully!
```

## Code Walkthrough

### Step 1: Creating Load Balancers for Backend Clusters

```rust
let api_upstreams = vec!["192.168.1.10:8080", "192.168.1.11:8080"];
let api_lb = LoadBalancer::try_from_iter(&api_upstreams)?;
let api_lb = Arc::new(api_lb);
```

- **Purpose**: Create a `LoadBalancer` that distributes requests across multiple backend servers
- **Round-robin**: By default, uses `RoundRobin` selection strategy
- **Arc**: Wrapped in `Arc` for shared ownership across the router

### Step 2: Building the Routing Map

```rust
let mut routing_map = BTreeMap::new();
routing_map.insert("api.example.com".to_string(), api_lb);
routing_map.insert("www.example.com".to_string(), web_lb);
routing_map.insert("*".to_string(), default_lb);
```

- **BTreeMap**: Maps hostnames to their corresponding `LoadBalancer` instances
- **Hostname matching**: Exact hostname matches take precedence over wildcard
- **Wildcard fallback**: The `"*"` entry catches all unmatched hostnames

### Step 3: Creating the Router

```rust
let router = Router::new(routing_map);
```

- **Router construction**: Takes the routing map as its only parameter
- **ProxyHttp trait**: The `Router` implements `ProxyHttp` for request handling
- **Ready to use**: This router can now be used in a Pingora server

## Key Concepts

### 1. Hostname-Based Routing

The load balancer routes requests based on the `Host` header in HTTP requests:

- `api.example.com` → API backend cluster
- `www.example.com` → Web backend cluster
- Any other hostname → Default fallback cluster

### 2. Load Balancer Selection Strategy

Each `LoadBalancer` uses **round-robin selection**:

- Distributes requests evenly across all upstream servers
- If an upstream is unhealthy (when health checks are configured), it's skipped
- Provides basic but effective load distribution

### 3. Wildcard Fallback

The `"*"` hostname acts as a catch-all:

- Handles requests to unknown/unmatched hostnames
- Prevents routing failures for unexpected hosts
- Useful for default routing or maintenance pages

### 4. ACME Challenge Routing

The router has special handling for Let's Encrypt ACME challenges:

- Paths matching `/.well-known/acme-challenge/*` route to a `"solver"` cluster
- Allows certificate validation while running the load balancer
- If no `"solver"` cluster exists, falls back to hostname-based routing

## Testing the Example

This example demonstrates the routing setup but doesn't run a server. To test with a real server:

1. **Use the provided binary**: The `moosicbox_lb` binary uses this same routing logic
2. **Configure via environment variables**:
    ```bash
    export CLUSTERS="api.example.com:192.168.1.10:8080,192.168.1.11:8080;www.example.com:192.168.1.20:80;*:192.168.1.100:8080"
    moosicbox_lb
    ```
3. **Send test requests** with different `Host` headers to verify routing

## Troubleshooting

### "LoadBalancer creation failed"

- **Cause**: Invalid upstream address format
- **Solution**: Ensure addresses are in `"ip:port"` or `"hostname:port"` format
- **Example**: `"192.168.1.10:8080"` or `"api-server:8080"`

### Understanding Upstream Selection

- **Without health checks**: All upstreams are always selected in round-robin order
- **With health checks**: Only healthy upstreams are selected (see the main `moosicbox_lb` binary for health check configuration)

## Related Examples

- See the `moosicbox_load_balancer` package README for a full server example
- See `moosicbox_load_balancer::Router` documentation for the `ProxyHttp` trait implementation
- See `packages/load_balancer/src/server.rs` for production server configuration with health checks and TLS
