# Basic Usage Example

This example demonstrates how to create and configure a basic HTTP/HTTPS load balancer using the MoosicBox load balancer package, which is built on Cloudflare's Pingora framework.

## What This Example Demonstrates

- **Load Balancer Creation**: Setting up a `LoadBalancer` with multiple upstream servers
- **Round-Robin Selection**: Distributing requests evenly across backends
- **Health Checking**: Configuring TCP health checks to monitor upstream availability
- **Hostname-Based Routing**: Using the `Router` to route requests based on the Host header
- **Wildcard Fallback**: Handling unmatched hostnames with a fallback route
- **HTTPS/TLS Support**: Optional TLS configuration for secure connections
- **Background Services**: Running health checks as background tasks

## Prerequisites

- Rust toolchain (1.70+ recommended)
- Basic understanding of load balancing concepts
- Familiarity with HTTP servers and reverse proxies
- Optional: OpenSSL for generating test certificates

## Running the Example

```bash
# From the repository root
cargo run --manifest-path packages/load_balancer/examples/basic_usage/Cargo.toml

# Or from the example directory
cd packages/load_balancer/examples/basic_usage
cargo run
```

**Note**: This example configures the load balancer to listen on ports 6188 (HTTP) and 6189 (HTTPS). The upstream servers are configured to be `127.0.0.1:8001` and `127.0.0.1:8002`.

## Expected Output

When you run the example, you should see output similar to:

```
Starting MoosicBox Load Balancer Example
=========================================

Step 1: Creating Pingora server instance...
Step 2: Defining upstream servers...
  Upstreams: ["127.0.0.1:8001", "127.0.0.1:8002"]

Step 3: Creating load balancer with round-robin selection...
Step 4: Configuring TCP health checks...
  Health check frequency: 10 seconds

Step 5: Setting up hostname-based routing...
  Route: api.example.com -> upstreams
  Route: * (fallback) -> upstreams

Step 6: Creating background health check service...
Step 7: Creating HTTP proxy service...

Step 8: Configuring listening addresses...
  HTTP: 0.0.0.0:6188
  No TLS certificates found (cert.pem/key.pem), HTTPS disabled

Step 9: Adding services to server...

Step 10: Starting load balancer...

┌─────────────────────────────────────────────┐
│  Load Balancer is now running!              │
├─────────────────────────────────────────────┤
│  HTTP:  http://localhost:6188               │
│  HTTPS: https://localhost:6189              │
├─────────────────────────────────────────────┤
│  Upstream servers:                          │
│    - 127.0.0.1:8001                         │
│    - 127.0.0.1:8002                         │
└─────────────────────────────────────────────┘

Press Ctrl+C to stop the server
```

## Code Walkthrough

### 1. Server Initialization

The example starts by creating a Pingora server instance:

```rust
let mut pingora_server = Server::new(None).expect("Failed to create server");
pingora_server.bootstrap();
```

### 2. Creating the Load Balancer

Define upstream servers and create a load balancer with round-robin selection:

```rust
let upstreams = vec!["127.0.0.1:8001", "127.0.0.1:8002"];
let mut lb = LoadBalancer::try_from_iter(upstreams.iter().map(|s| s.as_ref()))
    .expect("Failed to create load balancer");
```

### 3. Health Check Configuration

Configure TCP health checks to run every 10 seconds:

```rust
let health_check = TcpHealthCheck::new();
lb.set_health_check(health_check);
lb.health_check_frequency = Some(std::time::Duration::from_secs(10));
```

### 4. Router Setup

Create a router that maps hostnames to load balancers:

```rust
let mut router_map = BTreeMap::new();
router_map.insert("api.example.com".to_string(), lb_arc.clone());
router_map.insert("*".to_string(), lb_arc.clone()); // Wildcard fallback

let router = Router::new(router_map);
```

The router will:

- Route requests to `api.example.com` to the load balancer
- Route all other hostnames to the load balancer (via the `*` wildcard)

### 5. Creating Services

Create the HTTP proxy service and health check background service:

```rust
let health_check_service = background_service("health check", (*lb_arc).clone());
let mut proxy_service = http_proxy_service(&pingora_server.configuration, router);
```

### 6. Configuring Listeners

Add TCP and optionally TLS listeners:

```rust
proxy_service.add_tcp("0.0.0.0:6188");

// Optional TLS
if certificates_exist {
    let mut tls_settings = TlsSettings::intermediate(cert_path, key_path)?;
    tls_settings.enable_h2();
    proxy_service.add_tls_with_settings("0.0.0.0:6189", None, tls_settings);
}
```

### 7. Running the Server

Add services to the server and start:

```rust
pingora_server.add_service(health_check_service);
pingora_server.add_service(proxy_service);
pingora_server.run_forever();
```

## Key Concepts

### Load Balancing

The `LoadBalancer` from Pingora provides:

- **Backend Selection**: Chooses which upstream server to send each request to
- **Health Awareness**: Automatically excludes unhealthy backends from rotation
- **Selection Algorithms**: Round-robin (shown here), consistent hashing, and more

### Router

The `Router` struct implements the `ProxyHttp` trait and handles:

- **Hostname Matching**: Routes based on the HTTP Host header
- **Upstream Selection**: Delegates to the appropriate load balancer
- **Fallback Routing**: Uses wildcard (`*`) for unmatched hosts
- **ACME Challenge Routing**: Special handling for Let's Encrypt validation

### Health Checks

TCP health checks:

- **Probe Type**: Simple TCP connection attempt
- **Frequency**: Configurable interval (10 seconds in this example)
- **Automatic Exclusion**: Unhealthy backends are removed from rotation
- **Background Execution**: Runs as a separate background service

### TLS/HTTPS Support

The example demonstrates optional HTTPS configuration:

- **Certificate Loading**: Loads PEM-format certificate and key files
- **HTTP/2 Support**: Enables HTTP/2 via `enable_h2()`
- **Graceful Fallback**: HTTP-only operation if certificates unavailable

## Testing the Example

### 1. Start Backend Servers

First, start some backend servers on ports 8001 and 8002. You can use any HTTP server. For example, using Python's built-in server:

```bash
# Terminal 1
python3 -m http.server 8001

# Terminal 2
python3 -m http.server 8002
```

Or using the MoosicBox server if available:

```bash
# Terminal 1
PORT=8001 cargo run --bin moosicbox_server

# Terminal 2
PORT=8002 cargo run --bin moosicbox_server
```

### 2. Start the Load Balancer

```bash
# Terminal 3
cargo run --manifest-path packages/load_balancer/examples/basic_usage/Cargo.toml
```

### 3. Test Load Balancing

Send requests and observe round-robin distribution:

```bash
# Send multiple requests
for i in {1..6}; do
  curl -H "Host: api.example.com" http://localhost:6188/
  echo ""
done
```

Watch the backend server logs to see requests being distributed across both servers.

### 4. Test Hostname Routing

Test specific hostname routing:

```bash
# Request to api.example.com
curl -H "Host: api.example.com" http://localhost:6188/

# Request to other hostname (uses wildcard fallback)
curl -H "Host: www.example.com" http://localhost:6188/
```

### 5. Test Health Checking

Stop one of the backend servers and observe that the load balancer automatically stops sending traffic to it:

1. Stop the server on port 8001 (Ctrl+C in its terminal)
2. Wait 10+ seconds for health check to detect failure
3. Send more requests - they should all go to port 8002
4. Restart the server on port 8001
5. Wait 10+ seconds for health check to detect recovery
6. Requests should now be distributed across both servers again

### 6. Enable HTTPS (Optional)

Generate self-signed certificates for testing:

```bash
cd packages/load_balancer/examples/basic_usage
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes \
  -subj "/CN=localhost"

# Restart the load balancer
cargo run
```

Then test HTTPS:

```bash
curl -k https://localhost:6189/
```

## Troubleshooting

### Port Already in Use

**Problem**: Error message "address already in use"

**Solution**: Check if another process is using ports 6188 or 6189:

```bash
lsof -ti:6188 | xargs kill
lsof -ti:6189 | xargs kill
```

### Connection Refused

**Problem**: Load balancer starts but requests fail with "connection refused"

**Solution**: Ensure backend servers are running on ports 8001 and 8002 before starting the load balancer.

### All Backends Unhealthy

**Problem**: Load balancer reports all backends as unhealthy

**Solution**:

- Verify backend servers are reachable: `telnet 127.0.0.1 8001`
- Check firewall settings
- Ensure backend servers are listening on the correct ports

### TLS Certificate Errors

**Problem**: HTTPS fails with certificate errors

**Solution**: For testing, use the `-k` flag with curl to skip certificate validation, or add the self-signed certificate to your trust store.

## Related Examples

- **moosicbox_server examples**: Backend server implementations to use with this load balancer
- **web_server examples**: Understanding HTTP routing patterns
- **tunnel examples**: Related proxy and forwarding patterns

## Production Considerations

This example is designed for learning and development. For production use:

1. **Use Valid TLS Certificates**: Obtain certificates from Let's Encrypt or a CA
2. **Configure Proper Upstream Addresses**: Use actual backend server addresses
3. **Tune Health Check Settings**: Adjust frequency based on your requirements
4. **Monitor Performance**: Use Pingora's metrics and logging capabilities
5. **Handle Graceful Shutdown**: Implement signal handling for clean shutdowns
6. **Scale Appropriately**: Consider multiple load balancer instances behind a DNS round-robin or anycast

See the [main load_balancer package documentation](../../README.md) for production deployment guidance.
