# MoosicBox Load Balancer

A high-performance HTTP/HTTPS load balancer built with Pingora for the MoosicBox ecosystem.

## Overview

The MoosicBox Load Balancer provides:

- **High Performance**: Built on Cloudflare's Pingora framework for exceptional performance
- **HTTP/HTTPS Support**: Handle both encrypted and unencrypted traffic
- **Intelligent Routing**: Route requests to appropriate backend servers
- **Health Checking**: Automatic detection and handling of unhealthy backends
- **SSL/TLS Termination**: Handle SSL encryption/decryption at the edge
- **Connection Pooling**: Efficient connection management to backend servers

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

The load balancer can be configured through environment variables and configuration files.

#### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `BIND_ADDRESS` | Address to bind the load balancer | `0.0.0.0:80` |
| `BACKEND_SERVERS` | Comma-separated list of backend servers | - |
| `SSL_CERT_PATH` | Path to SSL certificate file | - |
| `SSL_KEY_PATH` | Path to SSL private key file | - |

#### Backend Server Configuration

Configure backend servers via environment variables:
```bash
export BACKEND_SERVERS="http://127.0.0.1:8001,http://127.0.0.1:8002,http://127.0.0.1:8003"
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

- **Load Balancing Algorithms**: Round-robin, least connections, and weighted distribution
- **Health Checks**: Active and passive health monitoring of backend servers
- **SSL/TLS Support**: Full SSL termination with certificate management
- **Request Routing**: Path-based and host-based routing capabilities
- **Connection Limits**: Configure maximum connections per backend
- **Graceful Shutdown**: Handle server restarts without dropping connections

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
export BACKEND_SERVERS="http://127.0.0.1:8001,http://127.0.0.1:8002"
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
EXPOSE 80 443
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
        - containerPort: 80
        - containerPort: 443
        env:
        - name: BACKEND_SERVERS
          value: "http://moosicbox-server-1:8001,http://moosicbox-server-2:8001"
```

## Troubleshooting

### Common Issues

1. **Backend servers unreachable**: Verify backend server addresses and network connectivity
2. **SSL certificate errors**: Check certificate file paths and permissions
3. **High latency**: Monitor backend server health and resource usage
4. **Connection failures**: Check connection limits and timeout settings

### Monitoring

Monitor load balancer performance:
```bash
# Check backend server status
curl http://localhost/health

# View load balancer metrics
curl http://localhost/metrics
```

## Performance Tuning

### System Limits

Increase system file descriptor limits for high-traffic scenarios:
```bash
# /etc/security/limits.conf
* soft nofile 65536
* hard nofile 65536
```

### Load Balancer Configuration

Optimize for your use case:
- Increase connection pool sizes for high-throughput scenarios
- Adjust health check intervals based on backend stability
- Configure appropriate timeout values for your network conditions

## See Also

- [MoosicBox Server](../server/README.md) - The main music server backend
- [MoosicBox Tunnel Server](../tunnel_server/README.md) - Remote access proxy
- [Pingora Documentation](https://github.com/cloudflare/pingora) - Underlying load balancing framework
