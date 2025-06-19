# MoosicBox Admin HTMX

Basic HTMX API endpoints for administration functionality in MoosicBox.

## Overview

The MoosicBox Admin HTMX package provides:

- **API Endpoints**: Basic REST endpoints for administrative operations
- **Service Integration**: Interfaces with Qobuz and Tidal streaming services
- **Scan Management**: Library scan control and status endpoints
- **Info Retrieval**: System information and status endpoints

## Current Implementation

### Available API Modules
- **Scan API**: Control music library scanning operations
- **Info API**: Retrieve system and server information
- **Qobuz API**: Qobuz streaming service integration endpoints
- **Tidal API**: Tidal streaming service integration endpoints
- **Utilities**: Common API utilities and helpers

### Features
- **Library Scanning**: Trigger and monitor library scans
- **Service Status**: Check status of integrated streaming services
- **System Info**: Basic system information retrieval
- **Error Handling**: Structured error responses
- **HTMX Integration**: Designed to work with HTMX frontend requests

## Installation

### From Source

```bash
# Clone and build
git clone https://github.com/MoosicBox/MoosicBox.git
cd MoosicBox
cargo build --package moosicbox_admin_htmx
```

## Usage

### API Endpoints

The package provides REST API endpoints that can be integrated into a web server:

```rust
use moosicbox_admin_htmx::api;

// Example integration (actual web server setup depends on your framework)
// GET /admin/scan/status - Get scan status
// POST /admin/scan/start - Start library scan
// GET /admin/info - Get system info
// GET /admin/qobuz/status - Check Qobuz service status
// GET /admin/tidal/status - Check Tidal service status
```

### Scan Operations

```bash
# Check scan status
curl http://localhost:8000/admin/scan/status

# Start a library scan
curl -X POST http://localhost:8000/admin/scan/start

# Get scan progress
curl http://localhost:8000/admin/scan/progress
```

### Service Status

```bash
# Check Qobuz service status
curl http://localhost:8000/admin/qobuz/status

# Check Tidal service status
curl http://localhost:8000/admin/tidal/status

# Get system information
curl http://localhost:8000/admin/info
```

## Development

### Building

```bash
# Build the library
cargo build --package moosicbox_admin_htmx

# Build with API feature
cargo build --package moosicbox_admin_htmx --features api
```

### API Development

The package provides modular API endpoints:

```rust
use moosicbox_admin_htmx::api::{scan, info, qobuz, tidal};

// Endpoints are organized by functionality:
// - scan: Library scanning operations
// - info: System information
// - qobuz: Qobuz service integration
// - tidal: Tidal service integration
// - util: Common utilities
```

## Implementation Notes

- Currently provides API endpoints only (no web interface)
- Designed for HTMX frontend integration
- Modular structure allows selective feature usage
- Error handling follows standard HTTP response patterns
- Built with async/await support for non-blocking operations

## Future Development

The package is structured to support future expansion into:

- Full web administration interface
- User management functionality
- Configuration management
- Real-time status monitoring
- Dashboard implementation

## Contributing

To contribute to this package:

1. Focus on the API modules in `src/api/`
2. Follow the existing pattern for new endpoints
3. Ensure proper error handling and response formatting
4. Add tests for new functionality
5. Update documentation for new endpoints
