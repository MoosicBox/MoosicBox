# MoosicBox Configuration

Configuration utilities and file-based configuration system for MoosicBox applications.

## Overview

The MoosicBox Config package provides:

- **File-based Configuration**: JSON5-based configuration files with support for comments and trailing commas
- **Directory Management**: Configuration and cache directory path utilities
- **Profile Support**: Multi-profile configuration directory management
- **Database Integration**: Basic profile and server identity management
- **App Type Support**: Configuration for different application types (app, server, local)
- **Path Utilities**: Helper functions for creating and managing config directories
- **Config Merging**: Merge global and profile-specific configurations

## Features

### Core Functionality

- **File-based Config**: Load and parse JSON5 configuration files with comments and trailing commas
- **Path Management**: Get and create configuration directory paths
- **Profile Directories**: Manage profile-specific configuration directories
- **Cache Directories**: Handle cache directory creation and access
- **App Type Support**: Support for app, server, and local application types
- **Root Directory Configuration**: Configurable root directory for all configs
- **Automatic File Discovery**: Prefer `.json5` files but fall back to `.json` (both parsed with JSON5)

### Available Operations

- **Directory Creation**: Automatically create config and cache directories
- **Profile Management**: Create, read, upsert, and delete user profiles
- **Server Identity**: Manage unique server identity for distributed setups
- **Path Resolution**: Resolve paths for different configuration contexts

### Optional Features

- **API Module**: REST API endpoints for profile management (requires `api` feature)
- **Database Module**: Profile and identity storage (requires `db` feature)

## Installation

### From Source

```bash
git clone https://github.com/MoosicBox/MoosicBox.git
cd MoosicBox
cargo build --release --package moosicbox_config
```

### Cargo Dependencies

```toml
[dependencies]
# Default features include api, db, file, and openapi
moosicbox_config = { path = "../config" }

# Or with specific features only
moosicbox_config = {
    path = "../config",
    default-features = false,
    features = ["db"]  # Choose specific features: api, db, openapi
}
```

## Configuration File Format

MoosicBox uses **JSON5** for configuration files, which allows:

- **Comments**: Both single-line (`//`) and multi-line (`/* */`) comments
- **Trailing commas**: In objects and arrays
- **Unquoted keys**: For valid JavaScript identifiers
- **Single quotes**: For strings

Configuration files can use either `.json5` or `.json` extensions. The system **prefers `.json5` files** but will also load `.json` files. Both formats are parsed using the JSON5 parser, so you can use JSON5 features even in `.json` files.

### Directory Structure

```
~/.local/moosicbox/
├── server/
│   ├── config.json5          # Global server configuration (preferred)
│   ├── config.json           # Alternative format (also supported)
│   └── profiles/
│       ├── bob/
│       │   └── config.json5  # Bob's profile configuration
│       └── larry/
│           └── config.json5  # Larry's profile configuration
├── app/
│   ├── config.json5          # Global app configuration
│   └── profiles/
│       └── default/
│           └── config.json5  # Default app profile
└── local/
    └── config.json5          # Local configuration
```

## Usage

### File-based Configuration

```rust
use moosicbox_config::{AppType, file::{load_global_config, load_profile_config, load_merged_config}};

fn load_configurations() -> Result<(), Box<dyn std::error::Error>> {
    // Load global configuration
    let global_config = load_global_config(AppType::Server)?;

    if let Some(server) = global_config.server {
        println!("Server host: {:?}", server.host);
        println!("Server port: {:?}", server.port);
    }

    // Load profile-specific configuration
    let profile_config = load_profile_config(AppType::Server, "bob")?;

    if let Some(paths) = profile_config.library_paths {
        println!("Library paths: {:?}", paths);
    }

    // Load merged configuration (global + profile)
    let merged = load_merged_config(AppType::Server, "bob")?;
    println!("Default profile: {:?}", merged.global.default_profile);

    Ok(())
}
```

### Example Configuration Files

**Global Configuration** (`~/.local/moosicbox/server/config.json5`):

```json5
{
    // Server configuration
    server: {
        host: '0.0.0.0',
        port: 8080,
    },

    // Automatic backup settings
    backup: {
        enabled: true,
        schedule: '0 0 * * *', // Daily at midnight
        retentionDays: 30,
    },

    // Logging configuration
    logging: {
        level: 'info',
        file: '/var/log/moosicbox/server.log',
    },

    // Feature flags
    features: {
        experimental: false,
    },

    // Default profile to use
    defaultProfile: 'default',
}
```

**Profile Configuration** (`~/.local/moosicbox/server/profiles/bob/config.json5`):

```json5
{
    // Music library paths
    libraryPaths: [
        '/music/library1',
        '/music/library2', // Trailing comma is fine!
    ],

    // Streaming service credentials
    services: {
        tidal: {
            accessToken: 'your-tidal-token',
            refreshToken: 'your-refresh-token',
        },
        qobuz: {
            appId: 'your-app-id',
            userAuthToken: 'your-auth-token',
        },
    },

    // Playback preferences
    playback: {
        gapless: true,
        crossfadeDuration: 2.5,
    },

    // Audio quality settings
    audioQuality: {
        preferredFormat: 'FLAC',
        bitDepth: 24,
        sampleRate: 96000,
    },

    // Player settings
    player: {
        volume: 0.8,
        bufferSize: 4096,
    },
}
```

### Basic Path Management

```rust
use moosicbox_config::{
    AppType, get_config_dir_path, get_profile_dir_path,
    make_config_dir_path, make_profile_dir_path
};

fn setup_directories() -> Result<(), Box<dyn std::error::Error>> {
    // Get configuration directory path
    if let Some(config_dir) = get_config_dir_path() {
        println!("Config directory: {:?}", config_dir);
    }

    // Create configuration directory if it doesn't exist
    if let Some(config_dir) = make_config_dir_path() {
        println!("Config directory created: {:?}", config_dir);
    }

    // Get profile-specific directory
    if let Some(profile_dir) = get_profile_dir_path(AppType::Server, "default") {
        println!("Profile directory: {:?}", profile_dir);
    }

    // Create profile directory
    if let Some(profile_dir) = make_profile_dir_path(AppType::Server, "default") {
        println!("Profile directory created: {:?}", profile_dir);
    }

    Ok(())
}
```

### Root Directory Configuration

```rust
use moosicbox_config::set_root_dir;
use std::path::PathBuf;

fn configure_root_directory() {
    // Set custom root directory (before any other operations)
    let custom_root = PathBuf::from("/opt/moosicbox");
    set_root_dir(custom_root);

    // Now all config paths will be relative to /opt/moosicbox
}
```

### Profile Management (with db feature)

```rust
use moosicbox_config::{upsert_profile, delete_profile, get_profiles};
use switchy_database::config::ConfigDatabase;

async fn manage_profiles(db: &ConfigDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Create or get existing profile
    let profile = upsert_profile(db, "my_profile").await?;
    println!("Profile: {}", profile.name);

    // Get all profiles
    let profiles = get_profiles(db).await?;
    for profile in profiles {
        println!("Found profile: {}", profile.name);
    }

    // Delete profile
    let deleted_profiles = delete_profile(db, "my_profile").await?;
    println!("Deleted {} profiles", deleted_profiles.len());

    Ok(())
}
```

### Server Identity Management (with db feature)

```rust
use moosicbox_config::{get_server_identity, get_or_init_server_identity};
use switchy_database::config::ConfigDatabase;

async fn manage_server_identity(db: &ConfigDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Get existing server identity
    if let Some(identity) = get_server_identity(db).await? {
        println!("Server identity: {}", identity);
    }

    // Get or create server identity
    let identity = get_or_init_server_identity(db).await?;
    println!("Server identity: {}", identity);

    Ok(())
}
```

### API Integration (with api feature)

```rust
use actix_web::{App, HttpServer};
use moosicbox_config::api;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(api::bind_services(
                actix_web::web::scope("/config")
            ))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
```

## Programming Interface

### File-based Configuration Functions

```rust
// Load configuration files (JSON5 format)
pub fn load_global_config(app_type: AppType) -> Result<GlobalConfig, ConfigError>;
pub fn load_profile_config(app_type: AppType, profile: &str) -> Result<ProfileConfig, ConfigError>;
pub fn load_merged_config(app_type: AppType, profile: &str) -> Result<MergedConfig, ConfigError>;
```

### Core Functions

```rust
// Path management
pub fn set_root_dir(path: PathBuf);
pub fn get_config_dir_path() -> Option<PathBuf>;
pub fn get_app_config_dir_path(app_type: AppType) -> Option<PathBuf>;
pub fn get_profiles_dir_path(app_type: AppType) -> Option<PathBuf>;
pub fn get_profile_dir_path(app_type: AppType, profile: &str) -> Option<PathBuf>;
pub fn get_cache_dir_path() -> Option<PathBuf>;

// Directory creation
pub fn make_config_dir_path() -> Option<PathBuf>;
pub fn make_profile_dir_path(app_type: AppType, profile: &str) -> Option<PathBuf>;
pub fn make_cache_dir_path() -> Option<PathBuf>;
```

### Database Functions (with db feature)

```rust
// Profile management
pub async fn create_profile(db: &ConfigDatabase, name: &str) -> Result<models::Profile, DatabaseFetchError>;
pub async fn upsert_profile(db: &ConfigDatabase, name: &str) -> Result<models::Profile, DatabaseFetchError>;
pub async fn delete_profile(db: &ConfigDatabase, name: &str) -> Result<Vec<models::Profile>, DatabaseFetchError>;
pub async fn get_profiles(db: &ConfigDatabase) -> Result<Vec<models::Profile>, DatabaseFetchError>;

// Server identity
pub async fn get_server_identity(db: &ConfigDatabase) -> Result<Option<String>, DatabaseError>;
pub async fn get_or_init_server_identity(db: &ConfigDatabase) -> Result<String, GetOrInitServerIdentityError>;
```

### API Functions (with api feature)

```rust
// Service binding
pub fn bind_services<T>(scope: Scope<T>) -> Scope<T>;
```

## Data Types

### AppType Enum

```rust
#[derive(Copy, Clone, Debug)]
pub enum AppType {
    App,    // Application configuration
    Server, // Server configuration
    Local,  // Local configuration
}
```

## Feature Flags

- **`api`**: Enable REST API endpoints for profile management
- **`db`**: Enable database functionality for profiles and server identity
- **`file`**: Enable file-based configuration loading with JSON5 support
- **`openapi`**: Enable OpenAPI/utoipa schema generation

## Default Behavior

- **Root Directory**: Defaults to `~/.local/moosicbox` if not set
- **Directory Creation**: Automatically creates directories when using `make_*` functions
- **Profile Support**: Each app type can have multiple named profiles
- **Cache Management**: Separate cache directory for temporary data

## Dependencies

- **Database**: Optional database integration via `switchy_database` for profiles (with `db` feature)
- **Home Directory**: Uses `home` crate for default root directory resolution
- **Path Management**: Cross-platform path handling

## Error Handling

All database operations return `Result` types with appropriate error handling:

- **DatabaseError**: Database operation failures
- **DatabaseFetchError**: Data fetching failures
- **GetOrInitServerIdentityError**: Server identity initialization errors
