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

// UpnpPlayer::new(
//     source_to_music_api,
//     device,
//     service,
//     source,
//     handle,
// )
```

### Public API

- `player::UpnpPlayer`
    - Main UPnP player type used by MoosicBox playback.
    - Constructor: `UpnpPlayer::new(source_to_music_api, device, service, source, handle)`.
    - Implements `moosicbox_player::Player` for playback control and `TryFrom<UpnpPlayer> for AudioOutputFactory` for audio output integration.
- `player::UpnpAvTransportService`
    - Wrapper around a discovered UPnP AVTransport service.
    - Implements `TryFrom<UpnpAvTransportService> for AudioOutputFactory`.
- `player::DEFAULT_SEEK_RETRY_OPTIONS`
    - Default retry policy used for UPnP seek behavior.
- `listener` module (enabled by `listener` feature)
    - `Handle::subscribe_media_info(interval, instance_id, udn, service_id, action)`
    - `Handle::subscribe_position_info(interval, instance_id, udn, service_id, action)`
    - `Handle::subscribe_transport_info(interval, instance_id, udn, service_id, action)`
    - `Handle::unsubscribe(subscription_id)`
    - Callback types: `MediaInfoSubscriptionAction`, `PositionInfoSubscriptionAction`, `TransportInfoSubscriptionAction`

## License

MPL-2.0
