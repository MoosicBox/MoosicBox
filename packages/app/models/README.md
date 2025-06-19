# MoosicBox App Models

Data models and structures for MoosicBox native applications.

## Overview

The MoosicBox App Models package provides:

- **Connection Models**: API connection configuration and management
- **Music API Settings**: Music service authentication and configuration
- **Download Settings**: Download location and management configuration
- **Scan Settings**: Music library scan path configuration
- **Feature Integration**: Optional integration with music API authentication

## Models

### Connection
- **name**: Display name for the connection
- **api_url**: MoosicBox server API endpoint URL

### MusicApiSettings
- **id**: Unique identifier for the music service
- **name**: Display name of the music service
- **logged_in**: Authentication status
- **supports_scan**: Whether the service supports library scanning
- **scan_enabled**: Whether scanning is currently enabled
- **run_scan_endpoint**: Optional API endpoint for triggering scans
- **auth_method**: Optional authentication method configuration

### DownloadSettings
- **download_locations**: List of available download locations with IDs
- **default_download_location**: Default download path

### ScanSettings
- **scan_paths**: List of local filesystem paths to scan for music

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_app_models = { path = "../app/models" }

# Optional: Enable music API authentication integration
moosicbox_app_models = {
    path = "../app/models",
    features = ["music-api-api"]
}
```

## Usage

### Connection Management

```rust
use moosicbox_app_models::Connection;

let connection = Connection {
    name: "Home Server".to_string(),
    api_url: "https://moosicbox.local:8686".to_string(),
};
```

### Music API Configuration

```rust
use moosicbox_app_models::MusicApiSettings;

let tidal_settings = MusicApiSettings {
    id: "tidal".to_string(),
    name: "Tidal".to_string(),
    logged_in: true,
    supports_scan: false,
    scan_enabled: false,
    run_scan_endpoint: None,
    auth_method: Some(auth_method),
};
```

### Download Configuration

```rust
use moosicbox_app_models::DownloadSettings;

let download_settings = DownloadSettings {
    download_locations: vec![
        (1, "/home/user/Music".to_string()),
        (2, "/mnt/storage/Music".to_string()),
    ],
    default_download_location: Some("/home/user/Music".to_string()),
};
```

## Feature Flags

- **`music-api-api`**: Enable integration with MoosicBox music API authentication

## Dependencies

- **Serde**: Serialization and deserialization
- **MoosicBox Music API API**: Optional authentication integration
