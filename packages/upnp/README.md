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
switchy_upnp = "0.1.3"
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
    println!("Found device: {} ({})", device.name, device.device_type);
    println!("  UDN: {}", device.udn);
    println!("  URL: {}", device.url);
}

// Get specific device by UDN
let device = get_device("uuid:device-udn")?;
```

### Media Transport Control

```rust
use switchy_upnp::{play, pause, stop, seek, get_service};

// Get the AVTransport service
let service = get_service("device-udn", "urn:upnp-org:serviceId:AVTransport")?;
let device_url = device.url().clone();

// Control playback
play(&service, &device_url, 0, 1.0).await?;  // Play at normal speed
pause(&service, &device_url, 0).await?;      // Pause
stop(&service, &device_url, 0).await?;       // Stop
seek(&service, &device_url, 0, "REL_TIME", 120).await?; // Seek to 2 minutes
```

### Setting Media URI

```rust
use switchy_upnp::set_av_transport_uri;

// Set the media to play
set_av_transport_uri(
    &service,
    &device_url,
    0,  // instance_id
    "http://server/track.mp3",  // transport_uri
    "audio/mpeg",  // format
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
let volume_info = get_volume(&service, &device_url, 0, "Master").await?;
println!("Current volume: {}", volume_info.get("CurrentVolume").unwrap_or(&"0".to_string()));

// Set volume to 75%
set_volume(&service, &device_url, 0, "Master", 75).await?;
```

### Getting Device Information

```rust
use switchy_upnp::{get_transport_info, get_position_info, get_media_info};

// Get transport state
let transport = get_transport_info(&service, &device_url, 0).await?;
println!("Transport state: {}", transport.current_transport_state);
println!("Transport status: {}", transport.current_transport_status);

// Get position information
let position = get_position_info(&service, &device_url, 0).await?;
println!("Current track: {}", position.track);
println!("Position: {}s / {}s", position.rel_time, position.track_duration);

// Get media information
let media = get_media_info(&service, &device_url, 0).await?;
println!("Current URI: {}", media.current_uri);
println!("Media duration: {}s", media.media_duration);
```

### Event Subscriptions

```rust
use switchy_upnp::subscribe_events;
use futures::StreamExt;

// Subscribe to device events
let (subscription_id, mut event_stream) = subscribe_events(&service, &device_url).await?;

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
    pub device_type: String,
    pub url: String,
    pub services: Vec<UpnpService>,
}

pub struct TransportInfo {
    pub current_transport_status: String,
    pub current_transport_state: String,
    pub current_speed: String,
}

pub struct PositionInfo {
    pub track: u32,
    pub rel_time: u32,
    pub track_uri: String,
    pub track_duration: u32,
    // ... additional fields
}
```

## Programming Interface

### Core Types

```rust
pub struct UpnpClient {
    discovery: DiscoveryClient,
    http_client: HttpClient,
}

impl UpnpClient {
    pub async fn new() -> Result<Self, UpnpError>;
    pub async fn discover_devices(&self, filter: Option<DeviceFilter>) -> Result<Vec<Device>, UpnpError>;
    pub async fn find_media_servers(&self) -> Result<Vec<MediaServer>, UpnpError>;
    pub async fn find_media_renderers(&self) -> Result<Vec<MediaRenderer>, UpnpError>;
}

#[derive(Debug, Clone)]
pub struct Device {
    pub device_type: String,
    pub friendly_name: String,
    pub manufacturer: String,
    pub model_name: String,
    pub model_number: Option<String>,
    pub serial_number: Option<String>,
    pub udn: String,
    pub location: String,
    pub services: Vec<Service>,
}

#[derive(Debug, Clone)]
pub struct Service {
    pub service_type: String,
    pub service_id: String,
    pub control_url: String,
    pub event_sub_url: Option<String>,
    pub scpd_url: String,
}
```

### Media Server Types

```rust
pub struct MediaServer {
    device: Device,
    content_directory: ContentDirectoryService,
    connection_manager: ConnectionManagerService,
}

impl MediaServer {
    pub fn content_directory(&self) -> Result<&ContentDirectoryService, UpnpError>;
    pub async fn browse_root(&self) -> Result<BrowseResult, UpnpError>;
    pub async fn search(&self, container_id: &str, search_criteria: &str) -> Result<SearchResult, UpnpError>;
}

#[derive(Debug, Clone)]
pub struct MediaItem {
    pub id: String,
    pub parent_id: String,
    pub title: String,
    pub creator: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub date: Option<String>,
    pub resources: Vec<Resource>,
}

#[derive(Debug, Clone)]
pub struct Resource {
    pub uri: String,
    pub protocol_info: String,
    pub size: Option<u64>,
    pub duration: Option<Duration>,
    pub bitrate: Option<u32>,
    pub sample_frequency: Option<u32>,
    pub bits_per_sample: Option<u8>,
    pub nr_audio_channels: Option<u8>,
}
```

### Media Renderer Types

```rust
pub struct MediaRenderer {
    device: Device,
    av_transport: AVTransportService,
    rendering_control: RenderingControlService,
    connection_manager: ConnectionManagerService,
}

impl MediaRenderer {
    pub async fn set_av_transport_uri(&self, uri: &str, metadata: &str) -> Result<(), UpnpError>;
    pub async fn play(&self) -> Result<(), UpnpError>;
    pub async fn pause(&self) -> Result<(), UpnpError>;
    pub async fn stop(&self) -> Result<(), UpnpError>;
    pub async fn seek(&self, target: SeekTarget) -> Result<(), UpnpError>;
    pub async fn set_volume(&self, volume: Volume) -> Result<(), UpnpError>;
    pub async fn get_volume(&self) -> Result<Volume, UpnpError>;
    pub async fn get_transport_info(&self) -> Result<TransportInfo, UpnpError>;
}

#[derive(Debug, Clone)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
    Transitioning,
    NoMediaPresent,
}

#[derive(Debug, Clone)]
pub enum Volume {
    Percentage(u8),
    Decibels(i16),
    Mute(bool),
}

#[derive(Debug, Clone)]
pub enum SeekTarget {
    RelTime(Duration),
    AbsTime(Duration),
    RelCount(i32),
    AbsCount(u32),
    TrackNr(u32),
}
```

## Configuration

### Environment Variables

- `UPNP_DISCOVERY_TIMEOUT`: Device discovery timeout in seconds (default: 30)
- `UPNP_SUBSCRIPTION_TIMEOUT`: Event subscription timeout in seconds (default: 1800)
- `UPNP_HTTP_TIMEOUT`: HTTP request timeout in seconds (default: 10)
- `UPNP_BIND_ADDRESS`: Network interface to bind to (default: 0.0.0.0)
- `UPNP_MEDIA_SERVER_PORT`: Port for media server (default: 8200)

### Feature Flags

- `api`: Enable Actix Web API endpoints
- `listener`: Enable UPnP event listener functionality
- `player`: Enable media player integration
- `openapi`: Enable OpenAPI documentation
- `simulator`: Enable device simulation for testing

## Web API Endpoints

When the `api` feature is enabled:

```
GET    /upnp/devices
GET    /upnp/devices/{udn}
GET    /upnp/media-servers
GET    /upnp/media-renderers
POST   /upnp/renderers/{udn}/play
POST   /upnp/renderers/{udn}/pause
POST   /upnp/renderers/{udn}/stop
PUT    /upnp/renderers/{udn}/volume
GET    /upnp/servers/{udn}/browse?container_id={id}
POST   /upnp/servers/{udn}/search
```

### API Usage Examples

```bash
# Discover UPnP devices
curl http://localhost:8000/upnp/devices

# Browse media server content
curl "http://localhost:8000/upnp/servers/uuid:server-123/browse?container_id=0"

# Control media renderer
curl -X POST http://localhost:8000/upnp/renderers/uuid:renderer-456/play

# Set volume
curl -X PUT http://localhost:8000/upnp/renderers/uuid:renderer-456/volume \
  -H "Content-Type: application/json" \
  -d '{"percentage": 75}'
```

## Advanced Usage

### Custom Media Server

```rust
use switchy_upnp::{MediaServer, ContentProvider, MediaMetadata};

struct CustomContentProvider {
    music_library: MusicLibrary,
}

#[async_trait]
impl ContentProvider for CustomContentProvider {
    async fn browse(&self, container_id: &str, filter: &BrowseFilter) -> Result<Vec<MediaItem>, UpnpError> {
        // Implement custom content browsing
        let items = self.music_library.get_items(container_id, filter).await?;
        Ok(items.into_iter().map(|item| item.into()).collect())
    }

    async fn search(&self, container_id: &str, search_criteria: &str) -> Result<Vec<MediaItem>, UpnpError> {
        // Implement custom search functionality
        let results = self.music_library.search(search_criteria).await?;
        Ok(results.into_iter().map(|item| item.into()).collect())
    }
}

// Use custom provider
let provider = CustomContentProvider::new(music_library);
let server = MediaServer::with_content_provider("Custom Server", provider)?;
server.start().await?;
```

### Multi-Zone Audio Control

```rust
use switchy_upnp::{AudioZone, ZoneConfiguration};

// Create audio zones
let living_room = AudioZone::new("Living Room")
    .add_renderer("uuid:living-room-speaker")
    .add_renderer("uuid:living-room-soundbar");

let kitchen = AudioZone::new("Kitchen")
    .add_renderer("uuid:kitchen-speaker");

// Synchronize playback across zones
let zone_config = ZoneConfiguration::new()
    .add_zone(living_room)
    .add_zone(kitchen)
    .with_sync_tolerance(Duration::from_millis(50));

// Play synchronized audio
zone_config.play_synchronized("http://server/track.mp3").await?;
```

## Error Handling

```rust
use switchy_upnp::UpnpError;

match client.discover_devices(None).await {
    Ok(devices) => {
        println!("Found {} devices", devices.len());
    }
    Err(UpnpError::NetworkError(e)) => {
        eprintln!("Network error during discovery: {}", e);
    }
    Err(UpnpError::ParseError(e)) => {
        eprintln!("Failed to parse device description: {}", e);
    }
    Err(UpnpError::TimeoutError) => {
        eprintln!("Discovery timed out");
    }
    Err(UpnpError::DeviceNotFound(udn)) => {
        eprintln!("Device not found: {}", udn);
    }
    Err(e) => eprintln!("UPnP error: {}", e),
}
```

## Testing

```bash
# Run all tests
cargo test

# Run with specific features
cargo test --features "api,player"

# Run integration tests with real devices
cargo test --test integration -- --ignored

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
