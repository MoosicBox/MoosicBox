# MoosicBox Music API Helpers

Helper utilities for music API operations and scanning.

## Overview

The MoosicBox Music API Helpers package provides:

- **Scan Helpers**: Utilities for music API scanning operations
- **Database Integration**: Helper functions for database operations
- **Profile Support**: Multi-profile scanning utilities
- **Error Handling**: Comprehensive error management for API operations

## Features

### Scan Operations

- **Enable Scan**: Enable scanning for specific music APIs
- **Scan Status**: Check if scanning is enabled for APIs
- **Trigger Scan**: Execute scanning operations for music APIs
- **Authentication**: Handle authentication requirements for scanning

### Database Integration

- **Profile Management**: Work with multi-profile databases
- **Origin Tracking**: Track scan origins and sources
- **Error Handling**: Comprehensive error management

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_music_api_helpers = { path = "../music_api/helpers" }
```

Note: The `scan` feature is enabled by default.

## Usage

### Scan Operations

```rust
use moosicbox_music_api_helpers::scan::{enable_scan, scan_enabled, scan};
use moosicbox_music_api::MusicApi;

// Enable scanning for a music API
enable_scan(&*music_api, &db).await?;

// Check if scanning is enabled
let enabled = scan_enabled(&*music_api, &db).await?;

// Trigger a scan operation
if enabled {
    scan(&*music_api, &db).await?;
}
```

### Error Handling

All operations return `Result<T, moosicbox_music_api::Error>` with comprehensive error handling:

- **Database Errors**: Database operation failures
- **Scan Errors**: Scanning operation failures
- **Authentication Errors**: Unauthorized access attempts

## Feature Flags

- **`scan`**: Enable scanning helper utilities (enabled by default)
- **`fail-on-warnings`**: Treat warnings as errors during compilation

## Dependencies

- **moosicbox_music_api**: Core music API traits
- **moosicbox_scan**: Library scanning functionality (optional, enabled with `scan` feature)
- **switchy**: Database abstraction with `database` feature
- **log**: Logging functionality
