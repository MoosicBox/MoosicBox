# MoosicBox Server

The main music server component of MoosicBox - a music streaming server with support for multiple audio sources and formats.

## Overview

The MoosicBox Server is the core component that provides:

- **Music Library Management**: Index and serve your local music collection
- **Multi-Format Audio Support**: AAC, FLAC, MP3, Opus encoding on-the-fly
- **Streaming Sources Integration**: Tidal, Qobuz, and YouTube Music support
- **Multi-Zone Audio**: Control playback across multiple devices and zones
- **Web API**: RESTful API with OpenAPI documentation
- **WebSocket Support**: Real-time communication for clients
- **Database Flexibility**: SQLite and PostgreSQL support
- **Tunnel Integration**: Remote access through tunnel server
- **High-Quality Audio**: Hi-Fi audio playback with configurable quality

## Installation

### From Source

```bash
cargo install --path packages/server --features "all-apis,all-formats,all-sources"
```

### Dependencies

- **pkg-config** (optional, for OPUS support)
- **libtool** (optional, for OPUS support)
- **libvips** (optional, for image optimization)
- **Database**: SQLite (included) or PostgreSQL

## Usage

### Basic Usage

Start the server (defaults to port 8000):

```bash
moosicbox_server
```

Or specify a custom port:

```bash
moosicbox_server 8001
```

Or using cargo:

```bash
cargo run --bin moosicbox_server --features "all-apis,all-formats,all-sources" -- 8001
```

### Development Mode

Run with debug logging:

```bash
RUST_BACKTRACE=1 RUST_LOG="moosicbox=debug" moosicbox_server 8001
```

### Production Deployment

With tunnel server integration:

```bash
WS_HOST="wss://tunnel.moosicbox.com/ws" \
TUNNEL_ACCESS_TOKEN='your_access_token' \
moosicbox_server 8001
```

Note: `STATIC_TOKEN` must be set at compile-time when using the `static-token-auth` feature.

## Configuration

### Environment Variables

| Variable              | Description                                                                      | Default   |
| --------------------- | -------------------------------------------------------------------------------- | --------- |
| `BIND_ADDR`           | Network address to bind to                                                       | `0.0.0.0` |
| `PORT`                | Service port (can also be passed as first argument)                              | `8000`    |
| `ACTIX_WORKERS`       | Number of Actix worker threads                                                   | Auto      |
| `MAX_THREADS`         | Maximum blocking threads                                                         | `64`      |
| `STATIC_TOKEN`        | Static authentication token (compile-time; requires `static-token-auth` feature) | -         |
| `WS_HOST`             | WebSocket tunnel host (requires `tunnel` feature)                                | -         |
| `TUNNEL_ACCESS_TOKEN` | Tunnel server access token (requires `tunnel` feature)                           | -         |

### Database Setup

#### SQLite (Default)

No additional setup required. Database file created automatically.

#### PostgreSQL

```bash
# Enable PostgreSQL support with features
cargo run --bin moosicbox_server --features "all-apis,all-formats,all-sources,postgres-sqlx" -- 8001

# Connection string configured via switchy_database_connection
# See documentation for database connection configuration
```

## Features

The server supports various feature flags for customization:

### Audio Formats

- `format-aac` - AAC/M4A support (requires OS encoders/decoders)
- `format-flac` - FLAC support (requires OS encoders/decoders)
- `format-mp3` - MP3 support
- `format-opus` - Opus support (requires OS encoders/decoders)
- `all-formats` - Enable all formats (includes `all-os-formats` and `format-mp3`)

### Audio Sources

- `qobuz` - Qobuz streaming integration
- `tidal` - Tidal streaming integration
- `yt` - YouTube Music integration
- `all-sources` - Enable all sources

### Audio Outputs

- `cpal` - Cross-platform audio library
- `jack` - JACK audio support
- `asio` - ASIO audio support (Windows)

### Database

- `sqlite-sqlx` - SQLite support via sqlx (default)
- `sqlite-rusqlite` - SQLite support via rusqlite
- `postgres-sqlx` - PostgreSQL support via sqlx
- `postgres-raw` - PostgreSQL support via raw connections
- `postgres-openssl` - PostgreSQL with OpenSSL
- `postgres-native-tls` - PostgreSQL with native TLS

### APIs

- `admin-htmx-api` - Admin web interface with HTMX
- `audio-output-api` - Audio output configuration API
- `audio-zone-api` - Audio zone management API
- `auth-api` - Authentication API
- `config-api` - Configuration API
- `downloader-api` - Download management API
- `files-api` - File serving API
- `library-api` - Local music library API
- `menu-api` - Menu API
- `music-api-api` - Music API integration
- `player-api` - Audio player API
- `qobuz-api` - Qobuz-specific API endpoints
- `scan-api` - Music library scanning API
- `search-api` - Global search API
- `session-api` - Session management API
- `tidal-api` - Tidal-specific API endpoints
- `upnp-api` - UPnP/DLNA API
- `yt-api` - YouTube Music-specific API endpoints
- `all-apis` - Enable all APIs (includes `app-apis`, `player-api`, `upnp-api`)

### Additional Features

- `openapi` - Enable OpenAPI documentation endpoints (enabled by default)
- `tunnel` - Enable tunnel server integration for remote access (enabled by default)
- `static-token-auth` - Enable static token authentication
- `tls` - Enable TLS support
- `telemetry` - Enable telemetry and metrics (enabled by default)
- `profiling` - Enable profiling support (enabled by default)
- `profiling-puffin` - Enable Puffin profiler
- `profiling-tracy` - Enable Tracy profiler
- `profiling-tracing` - Enable tracing profiler

## API Documentation

When running with the `openapi` feature, API documentation is available at:

- Swagger UI: `http://localhost:8001/openapi/swagger-ui/`
- ReDoc: `http://localhost:8001/openapi/redoc/`
- RapiDoc: `http://localhost:8001/openapi/rapidoc/`
- Scalar: `http://localhost:8001/openapi/scalar/`
- OpenAPI JSON: `http://localhost:8001/openapi/swagger-ui/api-docs/openapi.json`

## Library Scanning

The server can automatically scan and index your music library:

```bash
# Scan a directory
curl -X POST "http://localhost:8001/scan/run-scan-path?path=/path/to/music"
```

## Multi-Zone Audio

Configure audio zones for different rooms/devices:

```bash
# Audio zone API is available at /audio-zone
# Requires the audio-zone-api feature (enabled by default with all-apis)
curl "http://localhost:8001/audio-zone"
```

## Troubleshooting

### Common Issues

1. **Port already in use**: Choose a different port or stop other services
2. **Permission denied**: Ensure user has access to music directories
3. **Database connection failed**: Check database URL and credentials
4. **Audio playback issues**: Verify audio output configuration

### Logs

Enable detailed logging:

```bash
RUST_LOG="moosicbox_server=debug,moosicbox_audio=debug" moosicbox_server 8001
```

## See Also

- [MoosicBox Tunnel Server](../tunnel_server/README.md) - Remote access proxy
- [MoosicBox Load Balancer](../load_balancer/README.md) - Load balancing proxy
- [MoosicBox Native App](../app/native/README.md) - Desktop client application
