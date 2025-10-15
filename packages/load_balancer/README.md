# MoosicBox Load Balancer

A high-performance HTTP/HTTPS load balancer built with Pingora for the MoosicBox ecosystem.

## Overview

The MoosicBox Load Balancer provides:

- **High Performance**: Built on Cloudflare's Pingora framework for exceptional performance
- **HTTP/HTTPS Support**: Handles both encrypted and unencrypted traffic
- **Host-Based Routing**: Routes requests based on the Host header to appropriate backend clusters
- **TCP Health Checking**: Automatic detection and handling of unhealthy backends via TCP health checks
- **SSL/TLS Termination**: Handles SSL encryption/decryption at the edge with HTTP/2 support
- **ACME Challenge Support**: Special routing for Let's Encrypt certificate validation

## Installation

### From Source

```bash
cargo install --path packages/load_balancer
```

## Usage

### Basic Usage

Start the load balancer:

```bash
moosicbox_lb
```

Or using cargo:

```bash
cargo run --bin moosicbox_lb
```

### Configuration

The load balancer is configured through environment variables.

#### Environment Variables

| Variable       | Description                              | Default                    |
| -------------- | ---------------------------------------- | -------------------------- |
| `PORT`         | Port for HTTP traffic                    | `6188`                     |
| `SSL_PORT`     | Port for HTTPS traffic                   | `6189`                     |
| `CLUSTERS`     | Cluster configuration (see format below) | _Required_                 |
| `SSL_CRT_PATH` | Path to SSL certificate file             | `/etc/pingora/ssl/tls.crt` |
| `SSL_KEY_PATH` | Path to SSL private key file             | `/etc/pingora/ssl/tls.key` |

#### Cluster Configuration

The `CLUSTERS` environment variable defines backend clusters using this format:

```
host1,host2:backend1,backend2;host3:backend3,backend4
```

Examples:

```bash
# Single cluster with wildcard routing
export CLUSTERS="*:127.0.0.1:8001,127.0.0.1:8002"

# Multiple hosts routing to different backends
export CLUSTERS="api.example.com:10.0.1.1:8080,10.0.1.2:8080;web.example.com:10.0.2.1:80"

# Including a solver for ACME challenges
export CLUSTERS="example.com:10.0.1.1:8080;solver:127.0.0.1:8080"
```

### SSL/TLS Configuration

#### Using Let's Encrypt Certificates

```bash
export SSL_CERT_PATH="/etc/letsencrypt/live/yourdomain.com/fullchain.pem"
export SSL_KEY_PATH="/etc/letsencrypt/live/yourdomain.com/privkey.pem"
```

#### Using Self-Signed Certificates

```bash
# Generate self-signed certificate (for development)
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes

export SSL_CERT_PATH="./cert.pem"
export SSL_KEY_PATH="./key.pem"
```

## Features

- **Load Balancing Algorithm**: Round-robin distribution across backend servers
- **Health Checks**: TCP-based health monitoring of backend servers (checks every 10 seconds)
- **SSL/TLS Support**: Full SSL termination with HTTP/2 support
- **Host-Based Routing**: Routes requests to different backend clusters based on the Host header
- **Wildcard Routing**: Supports `*` as a fallback cluster for unmatched hosts
- **ACME Challenge Routing**: Automatic routing of `/.well-known/acme-challenge/*` requests to a dedicated solver cluster

## Development

### Debug Mode

Run with detailed logging:

```bash
RUST_LOG="moosicbox_load_balancer=debug" moosicbox_lb
```

### Testing Load Balancing

You can test the load balancer by starting multiple backend servers:

```bash
# Terminal 1: Start first backend
cargo run --bin moosicbox_server -- 8001

# Terminal 2: Start second backend
cargo run --bin moosicbox_server -- 8002

# Terminal 3: Start load balancer
export CLUSTERS="*:127.0.0.1:8001,127.0.0.1:8002"
cargo run --bin moosicbox_lb
```

## Production Deployment

### Docker Deployment

```dockerfile
FROM rust:bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin moosicbox_lb

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/moosicbox_lb /usr/local/bin/
EXPOSE 6188 6189
CMD ["moosicbox_lb"]
```

### Kubernetes Deployment

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
    name: moosicbox-lb
spec:
    replicas: 2
    selector:
        matchLabels:
            app: moosicbox-lb
    template:
        metadata:
            labels:
                app: moosicbox-lb
        spec:
            containers:
                - name: moosicbox-lb
                  image: moosicbox/load-balancer:latest
                  ports:
                      - containerPort: 6188
                        name: http
                      - containerPort: 6189
                        name: https
                  env:
                      - name: CLUSTERS
                        value: '*:moosicbox-server-1:8001,moosicbox-server-2:8001'
```

## Troubleshooting

### Common Issues

1. **Backend servers unreachable**: Verify backend server addresses and network connectivity
2. **SSL certificate errors**: Check certificate file paths and permissions
3. **High latency**: Monitor backend server health and resource usage
4. **Connection failures**: Check connection limits and timeout settings

### Logging

The load balancer logs to `moosicbox_lb.log` and stdout. Enable debug logging for detailed information:

```bash
RUST_LOG="moosicbox_load_balancer=debug" moosicbox_lb
```

Logs include:

- Cluster selection and upstream routing decisions
- Health check status
- SSL/TLS configuration status
- Request routing details

## Performance Tuning

### System Limits

Increase system file descriptor limits for high-traffic scenarios:

```bash
# /etc/security/limits.conf
* soft nofile 65536
* hard nofile 65536
```

### Load Balancer Configuration

The load balancer uses Pingora's built-in connection pooling and management features. Key configuration points:

- **Health Check Frequency**: Currently set to 10 seconds in the code (see `packages/load_balancer/src/server.rs:62`)
- **Load Balancing**: Uses round-robin selection across healthy backends
- **Backend Selection**: Automatically excludes unhealthy backends from rotation

**Planned**: Additional configuration options for connection limits, timeout values, and health check intervals may be added in future versions.

## See Also

- [MoosicBox Server](../server/README.md) - The main music server backend
- [MoosicBox Tunnel Server](../tunnel_server/README.md) - Remote access proxy
- [Pingora Documentation](https://github.com/cloudflare/pingora) - Underlying load balancing framework
