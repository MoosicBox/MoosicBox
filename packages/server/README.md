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
- **Database Flexibility**: SQLite, PostgreSQL, and MySQL support
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
- **Database**: SQLite (included), PostgreSQL, or MySQL

## Usage

### Basic Usage

Start the server on port 8001:
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
STATIC_TOKEN='your_static_token' \
moosicbox_server 8001
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `BIND_INTERFACE` | Network interface to bind to | `0.0.0.0` |
| `DATABASE_URL` | Database connection string | SQLite file |
| `STATIC_TOKEN` | Static authentication token | - |
| `WS_HOST` | WebSocket tunnel host | - |
| `TUNNEL_ACCESS_TOKEN` | Tunnel server access token | - |

### Database Setup

#### SQLite (Default)
No additional setup required. Database file created automatically.

#### PostgreSQL
```bash
export DATABASE_URL="postgres://username:password@localhost/moosicbox"
```

#### MySQL
```bash
export DATABASE_URL="mysql://username:password@localhost/moosicbox"
```

## Features

The server supports various feature flags for customization:

### Audio Formats
- `format-aac` - AAC/M4A support
- `format-flac` - FLAC support
- `format-mp3` - MP3 support
- `format-opus` - Opus support
- `all-formats` - Enable all formats

### Audio Sources
- `qobuz` - Qobuz streaming integration
- `tidal` - Tidal streaming integration
- `yt` - YouTube Music integration
- `all-sources` - Enable all sources

### Audio Outputs
- `cpal` - Cross-platform audio library
- `pulseaudio` - PulseAudio support
- `jack` - JACK audio support
- `asio` - ASIO audio support (Windows)

### APIs
- `library` - Local music library API
- `scan` - Music library scanning API
- `player` - Audio player API
- `search` - Global search API
- `downloader` - Download management API
- `all-apis` - Enable all APIs

## API Documentation

When running with the `openapi` feature, API documentation is available at:
- Swagger UI: `http://localhost:8001/swagger-ui/`
- ReDoc: `http://localhost:8001/redoc/`
- Scalar: `http://localhost:8001/scalar/`
- OpenAPI JSON: `http://localhost:8001/openapi.json`

## Library Scanning

The server can automatically scan and index your music library:

```bash
# Scan a directory
curl -X POST "http://localhost:8001/scan" \
  -H "Content-Type: application/json" \
  -d '{"path": "/path/to/music"}'
```

## Multi-Zone Audio

Configure audio zones for different rooms/devices:

```bash
# List audio zones
curl "http://localhost:8001/audio-zones"

# Create audio zone
curl -X POST "http://localhost:8001/audio-zones" \
  -H "Content-Type: application/json" \
  -d '{"name": "Living Room", "players": ["player-1"]}'
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
