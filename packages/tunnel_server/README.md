# MoosicBox Tunnel Server

Basic WebSocket-based tunneling server for MoosicBox remote access.

## Overview

The MoosicBox Tunnel Server provides:

- **WebSocket Tunneling**: Basic tunneling through WebSocket connections
- **Client Registration**: Simple client authentication and registration
- **Database Integration**: PostgreSQL backend for client management
- **HTTP API**: REST endpoints for tunnel management
- **Health Monitoring**: Basic health check endpoint

## Features

### Core Functionality

- **WebSocket Server**: Handles WebSocket connections for tunneling
- **Authentication**: Basic client registration and token-based auth
- **Database Storage**: Client data stored in PostgreSQL
- **HTTP Endpoints**: REST API for tunnel operations
- **CORS Support**: Cross-origin resource sharing enabled

### Available Endpoints

- **Health Check**: `/health` endpoint for service monitoring
- **WebSocket**: `/ws` endpoint for tunnel connections
- **Client Registration**: Authentication and client management
- **Track/Album/Artist**: Media proxy endpoints
- **Tunnel Management**: Basic tunnel lifecycle operations

## Installation

### From Source

```bash
# Install dependencies
sudo apt update
sudo apt install build-essential libssl-dev pkg-config libpq-dev

# Clone and build
git clone https://github.com/MoosicBox/MoosicBox.git
cd MoosicBox
TUNNEL_ACCESS_TOKEN=your-secure-token cargo build --release --bin moosicbox_tunnel_server

# Install binary
sudo cp target/release/moosicbox_tunnel_server /usr/local/bin/
```

### Database Setup

```bash
# Install and setup PostgreSQL
sudo apt install postgresql postgresql-contrib
sudo systemctl start postgresql
sudo systemctl enable postgresql

# Create database and user
sudo -u postgres createdb moosicbox_tunnel
sudo -u postgres createuser moosicbox
```

## Usage

### Running the Tunnel Server

```bash
# Start tunnel server on default port 8000
moosicbox_tunnel_server

# Start on custom port
moosicbox_tunnel_server 8443

# With environment variables
export PORT=8443
export BIND_ADDR=0.0.0.0
export DATABASE_URL=postgresql://user:pass@localhost/moosicbox_tunnel
moosicbox_tunnel_server
```

### Environment Variables

#### Build-Time Variables

- `TUNNEL_ACCESS_TOKEN`: Required access token for general authorization (must be set during compilation)

#### Runtime Variables

- `PORT`: Server port (default: 8000)
- `BIND_ADDR`: Bind address (default: 0.0.0.0)
- `DATABASE_URL`: PostgreSQL connection string
- `MAX_THREADS`: Maximum blocking threads (default: 64)
- `ACTIX_WORKERS`: Number of Actix workers

## API Reference

### Health Check

```bash
curl http://localhost:8000/health
```

### WebSocket Connection

```javascript
const ws = new WebSocket('ws://localhost:8000/ws');
ws.onopen = () => console.log('Connected');
ws.onmessage = (event) => console.log('Message:', event.data);
```

### Client Authentication

```bash
# Register client
curl -X POST http://localhost:8000/auth/register-client \
  -H "Content-Type: application/json" \
  -d '{"client_id": "my-client"}'

# Get authentication token
curl -X POST http://localhost:8000/auth/signature-token \
  -H "Content-Type: application/json" \
  -d '{"client_id": "my-client", "signature": "..."}'
```

## Configuration

The server can be configured via environment variables:

```bash
# Basic configuration
export PORT=8443
export BIND_ADDR=0.0.0.0
export DATABASE_URL=postgresql://localhost/moosicbox_tunnel

# Performance tuning
export MAX_THREADS=64
export ACTIX_WORKERS=4

# Logging
export RUST_LOG=info
export TOKIO_CONSOLE=1  # For tokio console debugging
```

## Development

### Building

```bash
# Build with all features
TUNNEL_ACCESS_TOKEN=your-secure-token cargo build --release --bin moosicbox_tunnel_server

# Build with specific features
TUNNEL_ACCESS_TOKEN=your-secure-token cargo build --release --bin moosicbox_tunnel_server --features postgres-raw
```

### Running in Development

```bash
# Run with debug logging
TUNNEL_ACCESS_TOKEN=your-secure-token RUST_LOG=debug cargo run --bin moosicbox_tunnel_server

# Run with tokio console
TUNNEL_ACCESS_TOKEN=your-secure-token TOKIO_CONSOLE=1 cargo run --bin moosicbox_tunnel_server
```

## Implementation Notes

- The server is built with Actix Web framework
- Uses PostgreSQL for data persistence
- WebSocket connections are managed through a dedicated service
- CORS is configured for web client access
- Supports graceful shutdown with proper cleanup
- Includes basic telemetry and metrics collection
