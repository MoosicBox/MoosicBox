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
- **Database Integration**: PostgreSQL backend for zone data storage
- **Zone Queries**: Fetch zones with optional session information
- **Profile Integration**: Support for profile-specific zone configurations

### Available Operations
- **Get Zones**: Retrieve all configured audio zones
- **Get Zone**: Fetch a specific zone by ID
- **Create Zone**: Add new audio zone configuration
- **Update Zone**: Modify existing zone settings
- **Delete Zone**: Remove zone configuration
- **Zones with Sessions**: Get zones with associated session data

### Optional Features
- **API Module**: REST API endpoints (requires `api` feature)
- **Events Module**: Zone update events (requires `events` feature)

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
use moosicbox_audio_zone::{zones, get_zone, create_audio_zone, CreateAudioZone};
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
        // ... other zone configuration
    };
    let created = create_audio_zone(db, &new_zone).await?;
    println!("Created zone: {}", created.name);

    Ok(())
}
```

### Zone Updates and Deletion

```rust
use moosicbox_audio_zone::{update_audio_zone, delete_audio_zone, UpdateAudioZone};

async fn modify_zones(db: &ConfigDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Update zone
    let update = UpdateAudioZone {
        id: 1,
        name: Some("Updated Living Room".to_string()),
        // ... other fields to update
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
        println!("Zone: {} - Session: {:?}", zone.name, zone.session);
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

- **`api`**: Enable REST API endpoints for zone management
- **`events`**: Enable event system for zone configuration changes

## Dependencies

- **Database**: Requires PostgreSQL database for zone storage
- **Models**: Uses `moosicbox_audio_zone_models` for data structures
- **Profiles**: Integration with MoosicBox profile system

## Error Handling

All operations return `Result` types with appropriate error handling:

- **DatabaseFetchError**: Database operation failures
- **Configuration Errors**: Invalid zone configuration data

## Web API Endpoints

When the `api` feature is enabled, REST endpoints are available for zone management through the web interface.

## Events

When the `events` feature is enabled, zone configuration changes trigger events that can be consumed by other parts of the MoosicBox system.
