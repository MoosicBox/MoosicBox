# MoosicBox Admin HTMX

HTMX-based web administration interface for MoosicBox.

## Overview

The MoosicBox Admin HTMX package provides:

- **Web Interface**: HTMX-powered admin dashboard with HTML rendering
- **Profile Management**: Create, select, and delete MoosicBox profiles
- **Service Integration**: Interfaces with Qobuz and Tidal streaming services
- **Scan Management**: Library scan path configuration and control
- **Server Info**: System information display

## Current Implementation

### Available Modules

- **Profiles**: Profile creation, selection, and deletion management
- **Scan**: Music library scan path management and scan execution (optional, enabled with `scan` feature)
- **Info**: Server identity and system information display (displayed as part of profile view)
- **Qobuz**: Qobuz service authentication and settings (optional, enabled with `qobuz` feature)
- **Tidal**: Tidal service authentication and settings (optional, enabled with `tidal` feature)
- **Utilities**: Common helper functions for HTMX interactions

### Features

- **Web Interface**: Full HTML-based admin interface using HTMX and Maud templating
- **Profile Management**: Create, select, and delete user profiles
- **Library Scanning**: Configure scan paths, add/remove scan sources, and trigger library scans
- **Service Authentication**: Login to Qobuz (username/password) and Tidal (device authorization)
- **Server Info**: Display server identity information
- **Real-time Updates**: HTMX-powered dynamic content updates without page reloads

## Installation

### From Source

```bash
# Clone and build
git clone https://github.com/MoosicBox/MoosicBox.git
cd MoosicBox
cargo build --package moosicbox_admin_htmx
```

## Usage

### Integration with Actix Web

The package provides HTMX endpoints that return HTML markup for integration into an Actix web server:

```rust
use actix_web::{web, App, HttpServer};
use moosicbox_admin_htmx::api;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(web::scope("/admin").configure(|cfg| {
                api::bind_services(cfg);
            }))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
```

### Available Endpoints

The package provides HTMX-compatible HTML endpoints:

**Main Interface:**

- `GET /admin` - Main admin dashboard

**Profile Management:**

- `GET /admin/profiles` - List all profiles
- `GET /admin/profiles/select` - Profile selection dropdown
- `POST /admin/profiles/select` - Select a profile
- `POST /admin/profiles/new` - Create new profile
- `DELETE /admin/profiles` - Delete a profile

**Scan Management (requires `scan` feature):**

- `GET /admin/scans` - View scan paths and controls
- `POST /admin/scan-paths` - Add a new scan path
- `DELETE /admin/scan-paths` - Remove a scan path
- `POST /admin/run-scan` - Trigger a library scan

**Qobuz Integration (requires `qobuz` feature):**

- `GET /admin/qobuz/settings` - Qobuz settings and login status (also supports OPTIONS, HEAD)
- `POST /admin/qobuz/auth/user-login` - Login to Qobuz
- `POST /admin/qobuz/run-scan` - Run Qobuz library scan (requires `scan` feature)

**Tidal Integration (requires `tidal` feature):**

- `GET /admin/tidal/settings` - Tidal settings and login status
- `POST /admin/tidal/auth/device-authorization` - Start Tidal device auth
- `POST /admin/tidal/auth/device-authorization/token` - Poll for auth token
- `POST /admin/tidal/run-scan` - Run Tidal library scan (requires `scan` feature)

### Web Interface

Access the admin interface by navigating to `http://localhost:8080/admin` in your web browser. The interface uses HTMX for dynamic updates without page reloads.

## Development

### Building

```bash
# Build with default features (api, qobuz, scan, tidal)
cargo build --package moosicbox_admin_htmx

# Build with specific features
cargo build --package moosicbox_admin_htmx --features api
cargo build --package moosicbox_admin_htmx --no-default-features --features api,scan
```

### Available Features

- `api` - Enables Actix Web integration and HTTP endpoints
- `scan` - Enables library scanning functionality and scan-related endpoints
- `qobuz` - Enables Qobuz service integration and authentication endpoints
- `tidal` - Enables Tidal service integration and authentication endpoints
- `base64` - Base64 encoding support (automatically enabled by `tidal` feature)

Default features: `api`, `qobuz`, `scan`, `tidal`

### Module Structure

The package is organized into modular endpoint handlers:

```rust
use moosicbox_admin_htmx::api;

// Available modules (depending on features):
// - profiles: Profile management
// - info: System information (no standalone endpoints, displayed in profile view)
// - scan: Library scanning (requires scan feature)
// - qobuz: Qobuz integration (requires qobuz feature)
// - tidal: Tidal integration (requires tidal feature)
```

Each module provides:

- `bind_services()` - Registers endpoints with Actix Web
- Public functions returning `Markup` for HTML rendering
- HTMX-compatible endpoints for dynamic updates

## Implementation Notes

- Provides a complete HTMX-based web interface with HTML rendering
- Uses Maud templating engine for type-safe HTML generation
- Returns HTML markup (not JSON) from all endpoints
- Modular structure allows selective feature usage via Cargo features
- Built with async/await support for non-blocking operations
- Requires Actix Web framework for HTTP server integration
- HTMX events are used for real-time UI updates and cross-component communication

## Future Development

Planned enhancements include:

- Additional profile configuration options
- User authentication and authorization
- Advanced configuration management interfaces
- Real-time scan progress monitoring
- Enhanced dashboard with statistics and metrics
- Additional streaming service integrations

## Contributing

To contribute to this package:

1. Focus on the endpoint modules in `src/api/`
2. Follow the existing pattern for new endpoints (return `Markup` for HTML)
3. Use Maud templating for HTML generation
4. Ensure proper error handling with Actix Web error types
5. Leverage HTMX attributes for dynamic updates
6. Add tests for new functionality
7. Update documentation for new endpoints

When adding new endpoints:

- Return `Result<Markup, actix_web::Error>` from endpoint handlers
- Use `#[route(...)]` attribute macro for endpoint registration
- Implement `bind_services()` function to register with Actix Web
- Utilize HTMX triggers and events for interactive features
