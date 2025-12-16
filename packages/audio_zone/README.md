# MoosicBox Audio Zone

Basic audio zone database management for MoosicBox applications.

## Overview

The MoosicBox Audio Zone package provides:

- **Database Operations**: CRUD operations for audio zone configuration
- **Zone Management**: Create, read, update, and delete audio zone records
- **Session Integration**: Optional integration with audio zones and sessions
- **Profile Support**: Multi-profile audio zone configurations
- **Event System**: Optional events for zone configuration changes

## Features

### Core Functionality

- **Zone CRUD**: Basic create, read, update, delete operations for audio zones
- **Database Integration**: Database abstraction layer for zone data storage
- **Zone Queries**: Fetch zones with optional session information
- **Profile Integration**: Support for profile-specific zone configurations
- **Player Management**: Associate multiple players with audio zones

### Available Operations

- **Get Zones**: Retrieve all configured audio zones
- **Get Zone**: Fetch a specific zone by ID
- **Create Zone**: Add new audio zone configuration
- **Update Zone**: Modify existing zone settings
- **Delete Zone**: Remove zone configuration
- **Zones with Sessions**: Get zones with associated session data

### Optional Features

- **API Module**: REST API endpoints for zone management (enabled by default, requires `api` feature)
- **Events Module**: Zone update events via subscription system (enabled by default, requires `events` feature)
- **OpenAPI**: OpenAPI documentation support (enabled by default, requires `openapi` feature)

## Installation

### From Source

```bash
git clone https://github.com/MoosicBox/MoosicBox.git
cd MoosicBox
cargo build --release --package moosicbox_audio_zone
```

### Cargo Dependencies

```toml
[dependencies]
moosicbox_audio_zone = { path = "../audio_zone" }

# Optional: Enable API endpoints
moosicbox_audio_zone = {
    path = "../audio_zone",
    features = ["api"]
}

# Optional: Enable event system
moosicbox_audio_zone = {
    path = "../audio_zone",
    features = ["events"]
}
```

## Usage

### Basic Zone Management

```rust
use moosicbox_audio_zone::{zones, get_zone, create_audio_zone, models::CreateAudioZone};
use switchy_database::config::ConfigDatabase;

async fn manage_zones(db: &ConfigDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Get all zones
    let all_zones = zones(db).await?;
    println!("Found {} zones", all_zones.len());

    // Get specific zone
    if let Some(zone) = get_zone(db, 1).await? {
        println!("Zone: {} (ID: {})", zone.name, zone.id);
    }

    // Create new zone
    let new_zone = CreateAudioZone {
        name: "Living Room".to_string(),
    };
    let created = create_audio_zone(db, &new_zone).await?;
    println!("Created zone: {}", created.name);

    Ok(())
}
```

### Zone Updates and Deletion

```rust
use moosicbox_audio_zone::{update_audio_zone, delete_audio_zone, models::UpdateAudioZone};

async fn modify_zones(db: &ConfigDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Update zone
    let update = UpdateAudioZone {
        id: 1,
        name: Some("Updated Living Room".to_string()),
        players: None,
    };
    let updated = update_audio_zone(db, update).await?;
    println!("Updated zone: {}", updated.name);

    // Delete zone
    if let Some(deleted) = delete_audio_zone(db, 1).await? {
        println!("Deleted zone: {}", deleted.name);
    }

    Ok(())
}
```

### Zones with Sessions

```rust
use moosicbox_audio_zone::zones_with_sessions;
use switchy_database::{config::ConfigDatabase, profiles::LibraryDatabase};

async fn get_zones_with_sessions(
    config_db: &ConfigDatabase,
    library_db: &LibraryDatabase,
) -> Result<(), Box<dyn std::error::Error>> {
    let zones = zones_with_sessions(config_db, library_db).await?;

    for zone in zones {
        println!("Zone: {} - Session: {:?}", zone.name, zone.session_id);
    }

    Ok(())
}
```

## Programming Interface

### Available Functions

```rust
// Core zone operations
pub async fn zones(db: &ConfigDatabase) -> Result<Vec<AudioZone>, DatabaseFetchError>;
pub async fn get_zone(db: &ConfigDatabase, id: u64) -> Result<Option<AudioZone>, DatabaseFetchError>;
pub async fn create_audio_zone(db: &ConfigDatabase, zone: &CreateAudioZone) -> Result<AudioZone, DatabaseFetchError>;
pub async fn update_audio_zone(db: &ConfigDatabase, update: UpdateAudioZone) -> Result<AudioZone, DatabaseFetchError>;
pub async fn delete_audio_zone(db: &ConfigDatabase, id: u64) -> Result<Option<AudioZone>, DatabaseFetchError>;

// Session integration
pub async fn zones_with_sessions(
    config_db: &ConfigDatabase,
    library_db: &LibraryDatabase,
) -> Result<Vec<AudioZoneWithSession>, DatabaseFetchError>;
```

## Data Models

The package uses models from `moosicbox_audio_zone_models`:

- **AudioZone**: Basic audio zone configuration
- **AudioZoneWithSession**: Zone with associated session data
- **CreateAudioZone**: Data for creating new zones
- **UpdateAudioZone**: Data for updating existing zones

## Feature Flags

- **`api`**: Enable REST API endpoints for zone management (enabled by default)
- **`events`**: Enable event system for zone configuration changes (enabled by default)
- **`openapi`**: Enable OpenAPI documentation support (enabled by default)

## Dependencies

- **Database**: Uses `switchy_database` for database abstraction (supports multiple backends)
- **Models**: Uses `moosicbox_audio_zone_models` for data structures
- **Profiles**: Integration with MoosicBox profile system

## Error Handling

All operations return `Result` types with appropriate error handling:

- **DatabaseFetchError**: Database operation failures
- **Configuration Errors**: Invalid zone configuration data

## Web API Endpoints

When the `api` feature is enabled (default), REST endpoints are available for zone management through the web interface:

- **GET** `/`: Get all audio zones (with pagination)
- **GET** `/with-session`: Get zones with associated sessions
- **POST** `/`: Create a new audio zone
- **PATCH** `/`: Update an existing audio zone
- **DELETE** `/`: Delete an audio zone by ID

## Events

When the `events` feature is enabled (default), zone configuration changes trigger events that can be consumed by other parts of the MoosicBox system.

### Available Events

- **`on_audio_zones_updated_event`**: Triggered when zones are created, updated, or deleted

### Event Usage

```rust
use moosicbox_audio_zone::events::on_audio_zones_updated_event;

async fn setup_event_listeners() {
    on_audio_zones_updated_event(|| async {
        println!("Audio zones were updated!");
        Ok(())
    }).await;
}
```
