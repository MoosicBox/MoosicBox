# Switchy UPnP

UPnP/DLNA library for the MoosicBox ecosystem, providing device discovery and basic media renderer control functionality for Universal Plug and Play devices.

## Features

- **UPnP Device Discovery**: Automatic discovery of UPnP devices on the network
- **Device Caching**: Cache discovered devices and services for efficient access
- **Media Renderer Control**: Basic control of UPnP/DLNA media renderers
- **Transport Control**: Play, pause, stop, and seek operations
- **Volume Control**: Get and set volume levels on UPnP devices
- **Service Management**: Access and interact with UPnP services
- **Event Subscriptions**: Subscribe to UPnP device state changes
- **Metadata Handling**: Basic media metadata support

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
switchy_upnp = "0.1.4"
```

## Usage

### Device Discovery and Caching

```rust
use switchy_upnp::{scan_devices, devices, get_device};

// Scan for UPnP devices
scan_devices().await?;

// Get all discovered devices
let device_list = devices().await;

for device in device_list {
    println!("Found device: {}", device.name);
    println!("  UDN: {}", device.udn);
}

// Get specific device by UDN
let device = get_device("uuid:device-udn")?;
```

### Media Transport Control

```rust
use switchy_upnp::{play, pause, stop, seek, get_service};

// Get the AVTransport service
let service = get_service("device-udn", "urn:upnp-org:serviceId:AVTransport")?;
let device_url = device.url();

// Control playback
play(&service, device_url, 0, 1.0).await?;  // Play at normal speed
pause(&service, device_url, 0).await?;      // Pause
stop(&service, device_url, 0).await?;       // Stop
seek(&service, device_url, 0, "REL_TIME", 120).await?; // Seek to 2 minutes
```

### Setting Media URI

```rust
use switchy_upnp::set_av_transport_uri;

// Set the media to play
set_av_transport_uri(
    &service,
    device_url,
    0,  // instance_id
    "http://server/track.mp3",  // transport_uri
    "flac",  // format
    Some("Track Title"),    // title
    Some("Artist Name"),    // creator
    Some("Artist Name"),    // artist
    Some("Album Name"),     // album
    Some(1),               // track_number
    Some(240),             // duration in seconds
    Some(5_000_000),       // size in bytes
).await?;
```

### Volume Control

```rust
use switchy_upnp::{get_volume, set_volume};

// Get current volume
let volume_info = get_volume(&service, device_url, 0, "Master").await?;
println!("Current volume: {}", volume_info.get("CurrentVolume").unwrap_or(&"0".to_string()));

// Set volume to 75
set_volume(&service, device_url, 0, "Master", 75).await?;
```

### Getting Device Information

```rust
use switchy_upnp::{get_transport_info, get_position_info, get_media_info};

// Get transport state
let transport = get_transport_info(&service, device_url, 0).await?;
println!("Transport state: {}", transport.current_transport_state);
println!("Transport status: {}", transport.current_transport_status);

// Get position information
let position = get_position_info(&service, device_url, 0).await?;
println!("Current track: {}", position.track);
println!("Position: {}s / {}s", position.rel_time, position.track_duration);

// Get media information
let media = get_media_info(&service, device_url, 0).await?;
println!("Current URI: {}", media.current_uri);
println!("Media duration: {}s", media.media_duration);
```

### Event Subscriptions

```rust
use switchy_upnp::subscribe_events;
use futures::StreamExt;

// Subscribe to device events
let (subscription_id, mut event_stream) = subscribe_events(&service, device_url).await?;

// Handle events
while let Some(event) = event_stream.next().await {
    match event {
        Ok(event_data) => {
            for (key, value) in event_data {
                println!("Event: {} = {}", key, value);
            }
        }
        Err(e) => eprintln!("Event error: {}", e),
    }
}
```

## Error Types

The library provides several error types:

- `ActionError`: Errors when performing UPnP actions
- `ScanError`: Errors during device discovery and scanning
- `UpnpDeviceScannerError`: Errors in the device scanner

## Core Types

```rust
pub struct UpnpDevice {
    pub name: String,
    pub udn: String,
    pub volume: Option<String>,
    pub services: Vec<UpnpService>,
}

pub struct UpnpService {
    pub id: String,
    pub r#type: String,
}

pub struct TransportInfo {
    current_transport_status: String,
    current_transport_state: String,
    current_speed: String,
}

pub struct PositionInfo {
    track: u32,
    rel_time: u32,
    abs_time: u32,
    track_uri: String,
    track_metadata: TrackMetadata,
    rel_count: u32,
    abs_count: u32,
    track_duration: u32,
}

pub struct MediaInfo {
    media_duration: u32,
    record_medium: String,
    write_status: String,
    current_uri_metadata: TrackMetadata,
    nr_tracks: u32,
    play_medium: String,
    current_uri: String,
}
```

## Feature Flags

- `api`: Enable Actix Web API endpoints
- `listener`: Enable UPnP event listener functionality
- `player`: Enable media player integration
- `openapi`: Enable OpenAPI documentation
- `simulator`: Enable device simulation for testing

## Web API Endpoints

When the `api` feature is enabled:

```
GET    /scan-devices       - Scan and return list of UPnP devices
GET    /transport-info     - Get transport information
GET    /media-info         - Get media information
GET    /position-info      - Get position information
GET    /volume             - Get volume
POST   /volume             - Set volume
POST   /subscribe          - Subscribe to device events
POST   /pause              - Pause playback
POST   /play               - Play
POST   /seek               - Seek to position
```

### API Usage Examples

```bash
# Discover UPnP devices
curl http://localhost:8000/scan-devices

# Get transport info
curl "http://localhost:8000/transport-info?deviceUdn=uuid:device-123&instanceId=0"

# Control playback
curl -X POST "http://localhost:8000/play?deviceUdn=uuid:device-123&instanceId=0&speed=1.0"

# Set volume
curl -X POST "http://localhost:8000/volume?deviceUdn=uuid:device-123&instanceId=0&value=75"
```

## Testing

```bash
# Run all tests
cargo test

# Run with specific features
cargo test --features "api,player"

# Test with simulator
cargo test --features "simulator"
```

## Troubleshooting

### Common Issues

**No Devices Found**

- Check network connectivity and firewall settings
- Ensure devices are on the same network subnet
- Verify multicast is enabled on network interface
- Try increasing discovery timeout

**Connection Refused**

- Verify device URLs are accessible
- Check if devices require authentication
- Ensure correct protocol (HTTP vs HTTPS)
- Validate device descriptions and service URLs

**Playback Issues**

- Verify media format compatibility
- Check network bandwidth and stability
- Ensure media URLs are accessible from renderer
- Validate media metadata and MIME types

**Event Subscription Failures**

- Check if device supports event subscriptions
- Verify callback URL is accessible from device
- Ensure subscription timeout is reasonable
- Check for network address translation issues

## See Also

- [`moosicbox_player`](../player/README.md) - Audio playback engine
- [`moosicbox_audio_output`](../audio_output/README.md) - Audio output backends
- [`moosicbox_session`](../session/README.md) - Session management
- [`moosicbox_music_api`](../music_api/README.md) - Music API abstractions
- [`switchy_http`](../http/README.md) - HTTP client utilities
