# MoosicBox Configuration

Basic configuration utilities for MoosicBox applications.

## Overview

The MoosicBox Config package provides:

- **Directory Management**: Configuration and cache directory path utilities
- **Profile Support**: Multi-profile configuration directory management
- **Database Integration**: Basic profile and server identity management
- **App Type Support**: Configuration for different application types (app, server, local)
- **Path Utilities**: Helper functions for creating and managing config directories

## Features

### Core Functionality
- **Path Management**: Get and create configuration directory paths
- **Profile Directories**: Manage profile-specific configuration directories
- **Cache Directories**: Handle cache directory creation and access
- **App Type Support**: Support for app, server, and local application types
- **Root Directory Configuration**: Configurable root directory for all configs

### Available Operations
- **Directory Creation**: Automatically create config and cache directories
- **Profile Management**: Create, read, update, delete user profiles
- **Server Identity**: Manage unique server identity for distributed setups
- **Path Resolution**: Resolve paths for different configuration contexts

### Optional Features
- **API Module**: REST API endpoints (requires `api` feature)
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
moosicbox_config = { path = "../config" }

# Optional: Enable database functionality
moosicbox_config = {
    path = "../config",
    features = ["db"]
}

# Optional: Enable API endpoints
moosicbox_config = {
    path = "../config",
    features = ["api"]
}
```

## Usage

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
    println!("Profile: {} (ID: {})", profile.name, profile.id);

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

## Programming Interface

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
pub async fn upsert_profile(db: &ConfigDatabase, name: &str) -> Result<models::Profile, DatabaseFetchError>;
pub async fn delete_profile(db: &ConfigDatabase, name: &str) -> Result<Vec<models::Profile>, DatabaseFetchError>;
pub async fn get_profiles(db: &ConfigDatabase) -> Result<Vec<models::Profile>, DatabaseFetchError>;

// Server identity
pub async fn get_server_identity(db: &ConfigDatabase) -> Result<Option<String>, DatabaseError>;
pub async fn get_or_init_server_identity(db: &ConfigDatabase) -> Result<String, GetOrInitServerIdentityError>;
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

- **`api`**: Enable REST API endpoints for configuration management
- **`db`**: Enable database functionality for profiles and server identity
- **`test`**: Enable test utilities for temporary directories

## Default Behavior

- **Root Directory**: Defaults to `~/.local/moosicbox` if not set
- **Directory Creation**: Automatically creates directories when using `make_*` functions
- **Profile Support**: Each app type can have multiple named profiles
- **Cache Management**: Separate cache directory for temporary data

## Dependencies

- **Database**: Optional PostgreSQL integration for profiles (with `db` feature)
- **Home Directory**: Uses `home` crate for default root directory resolution
- **Path Management**: Cross-platform path handling

## Error Handling

All database operations return `Result` types with appropriate error handling:

- **DatabaseError**: Database operation failures
- **DatabaseFetchError**: Data fetching failures
- **GetOrInitServerIdentityError**: Server identity initialization errors
