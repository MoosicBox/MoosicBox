# MoosicBox Scan

Music library scanning system for discovering, analyzing, and indexing audio files from local storage and music API sources.

## Overview

The MoosicBox Scan package provides music library scanning capabilities for the MoosicBox server. It supports scanning both local audio files and remote music APIs (Tidal, Qobuz, etc.) to build and maintain the music library database.

## Features

### Core Scanning

- **Local File Scanning**: Recursive directory scanning for audio files
- **Music API Scanning**: Integration with external music services (via `moosicbox_music_api`)
- **Multi-Format Support**: MP3, FLAC, AAC, and Opus audio files
- **Metadata Extraction**: Basic metadata from ID3, Vorbis, and MP4 tags
- **Cover Art Handling**: Album and artist cover image extraction and caching
- **Progress Tracking**: Scan progress events and listener system

### Supported Operations

- Enable/disable scan origins (Local, Tidal, Qobuz, etc.)
- Manage local scan paths (add, remove, list)
- Run scans on-demand via API
- Track scan progress through event listeners

## Architecture

The package is organized into several modules:

- **`lib.rs`**: Core scanner implementation and origin management
- **`local.rs`**: Local filesystem scanning (requires `local` feature)
- **`music_api.rs`**: Remote music API scanning
- **`api.rs`**: REST API endpoints for scan operations (requires `api` feature)
- **`event.rs`**: Progress event system for scan tracking
- **`output.rs`**: Scan result processing and database updates
- **`db/`**: Database operations for scan locations and origins

## Usage

### Basic Scanning

```rust
use moosicbox_scan::{Scanner, ScanOrigin, event::ScanTask};
use moosicbox_music_api::MusicApis;
use switchy_database::profiles::LibraryDatabase;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = LibraryDatabase::new(/* ... */);
    let music_apis = MusicApis::default();

    // Create scanner for local files
    let scanner = Scanner::from_origin(&db, ScanOrigin::Local).await?;

    // Run scan
    scanner.scan(music_apis, &db).await?;

    Ok(())
}
```

### Managing Scan Paths (Local)

```rust
use moosicbox_scan::{add_scan_path, get_scan_paths, remove_scan_path};

async fn manage_paths(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Add a path to scan
    add_scan_path(db, "/home/user/Music").await?;

    // Get all configured paths
    let paths = get_scan_paths(db).await?;
    println!("Scan paths: {:?}", paths);

    // Remove a path
    remove_scan_path(db, "/home/user/Music").await?;

    Ok(())
}
```

### Managing Scan Origins

```rust
use moosicbox_scan::{enable_scan_origin, disable_scan_origin, get_scan_origins, ScanOrigin};

async fn manage_origins(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Enable a music API origin
    enable_scan_origin(db, &ScanOrigin::Tidal).await?;

    // Get all enabled origins
    let origins = get_scan_origins(db).await?;

    // Disable an origin
    disable_scan_origin(db, &ScanOrigin::Tidal).await?;

    Ok(())
}
```

### Running Scans

```rust
use moosicbox_scan::{run_scan, ScanOrigin};

async fn scan_library(
    db: &LibraryDatabase,
    music_apis: MusicApis,
) -> Result<(), Box<dyn std::error::Error>> {
    // Scan specific origins
    run_scan(
        Some(vec![ScanOrigin::Local, ScanOrigin::Tidal]),
        db,
        music_apis,
    ).await?;

    // Scan all enabled origins
    run_scan(None, db, music_apis).await?;

    Ok(())
}
```

### Progress Tracking

```rust
use moosicbox_scan::event::{add_progress_listener, ProgressEvent};

async fn track_progress() {
    add_progress_listener(Box::new(|event| {
        Box::pin(async move {
            match event {
                ProgressEvent::ScanCountUpdated { scanned, total, .. } => {
                    println!("Total files to scan: {}", total);
                }
                ProgressEvent::ItemScanned { scanned, total, .. } => {
                    println!("Progress: {}/{}", scanned, total);
                }
                ProgressEvent::ScanFinished { scanned, total, .. } => {
                    println!("Scan complete: {} items scanned", scanned);
                }
                _ => {}
            }
        })
    }))
    .await;
}
```

### Cancellation

```rust
use moosicbox_scan::cancel;

async fn cancel_scan() {
    // Cancel any running scans
    cancel();
}
```

## REST API Endpoints

When the `api` feature is enabled, the following endpoints are available:

- `POST /run-scan?origins=Local,Tidal` - Run a scan synchronously
- `POST /start-scan?origins=Local` - Start a scan asynchronously
- `GET /scan-origins` - Get enabled scan origins
- `POST /scan-origins?origin=Tidal` - Enable a scan origin
- `DELETE /scan-origins?origin=Tidal` - Disable a scan origin
- `GET /scan-paths` - Get local scan paths (requires `local` feature)
- `POST /scan-paths?path=/music` - Add a local scan path (requires `local` feature)
- `DELETE /scan-paths?path=/music` - Remove a local scan path (requires `local` feature)
- `POST /run-scan-path?path=/music` - Scan a specific path (requires `local` feature)

## Feature Flags

- `default`: `["all-formats", "api", "local", "openapi"]`
- `api`: Enables REST API endpoints
- `local`: Enables local filesystem scanning
- `openapi`: Enables OpenAPI documentation
- `all-formats`: Enables all audio format support
- `all-os-formats`: `["aac", "flac", "opus"]`
- `aac`: AAC/M4A format support
- `flac`: FLAC format support
- `mp3`: MP3 format support
- `opus`: Opus format support
- `fail-on-warnings`: Treat warnings as errors

## Dependencies

Key dependencies from `Cargo.toml`:

```toml
[dependencies]
moosicbox_audiotags = { workspace = true, optional = true }  # Metadata extraction
moosicbox_lofty = { workspace = true, optional = true }      # Audio properties
mp3-duration = { workspace = true, optional = true }         # MP3 duration calculation
moosicbox_files = { workspace = true }                       # File utilities
moosicbox_library = { workspace = true }                     # Database models
moosicbox_music_api = { workspace = true }                   # Music API integration
moosicbox_search = { workspace = true }                      # Search index updates
```

## Implementation Details

### Local Scanning

The local scanner (`local.rs`):

1. Recursively walks directory trees
2. Identifies audio files by extension (`.flac`, `.m4a`, `.mp3`, `.opus`)
3. Extracts metadata using `moosicbox_audiotags` and `moosicbox_lofty`
4. Searches for cover art in album directories or embedded in files
5. Updates the library database with discovered tracks

### Music API Scanning

The music API scanner (`music_api.rs`):

1. Fetches albums from configured music services
2. Retrieves tracks for each album
3. Downloads and caches cover artwork
4. Creates database entries for artists, albums, and tracks
5. Stores API source mappings for external content

### Database Integration

Scan results are processed through `ScanOutput` which:

- Deduplicates artists and albums
- Handles cover art caching
- Batch inserts/updates library database
- Rebuilds global search index

## Error Handling

```rust
use moosicbox_scan::ScanError;

match scanner.scan(music_apis, &db).await {
    Ok(()) => println!("Scan completed successfully"),
    Err(ScanError::DatabaseFetch(e)) => eprintln!("Database error: {}", e),
    Err(ScanError::Local(e)) => eprintln!("Local scan error: {}", e),
    Err(ScanError::MusicApi(e)) => eprintln!("Music API error: {}", e),
    Err(e) => eprintln!("Scan error: {}", e),
}
```

## See Also

- [moosicbox_scan_models](../scan_models/README.md) - Data models for scan operations
- [moosicbox_library](../library/README.md) - Library database management
- [moosicbox_music_api](../music_api/README.md) - Music API integrations
- [moosicbox_files](../files/README.md) - File handling utilities
