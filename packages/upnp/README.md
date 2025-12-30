# MoosicBox UPnP

MoosicBox-specific UPnP player implementation for controlling media playback on UPnP/DLNA devices.

This crate provides the MoosicBox integration layer on top of `switchy_upnp`, including:

- **Player**: `UPnP` player implementation that integrates with `MoosicBox` playback system
- **Listener**: Event listener service for monitoring `UPnP` device state changes

## Features

- `api` - Actix-web API support
- `listener` - Event listener service for `UPnP` device monitoring
- `openapi` - OpenAPI/utoipa schema support
- `player` - `UPnP` player implementation
- `simulator` - Simulated `UPnP` devices for testing

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_upnp = { version = "0.1.0" }
```

## Usage

```rust,no_run
use moosicbox_upnp::player::UpnpPlayer;

// Create a UPnP player for a discovered device
// (requires device and service from switchy_upnp scanning)
```

## License

MPL-2.0
