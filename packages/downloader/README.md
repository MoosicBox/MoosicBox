# MoosicBox Downloader

Music download management system for the MoosicBox ecosystem.

## Overview

The MoosicBox Downloader package provides functionality for downloading music tracks, album covers, and artist covers from various music API sources (local library, Tidal, Qobuz, etc.) with queue management, progress tracking, and automatic file tagging.

## Features

### Core Functionality

- **Track Downloads**: Download music tracks with automatic metadata tagging
- **Album Downloads**: Download entire albums including all tracks
- **Cover Art Downloads**: Download album and artist cover images
- **Queue Management**: Background download queue with progress tracking
- **Resume Support**: Resume interrupted downloads automatically
- **Progress Tracking**: Real-time download progress and speed monitoring
- **Audio Quality Selection**: Choose audio quality (FLAC, AAC, MP3, Opus)
- **File Organization**: Automatic file organization by artist/album/track
- **Local Scanning**: Automatic library scanning after downloads complete

### Supported Sources

- **Local Library**: Direct file downloads from local MoosicBox instances
- **Music APIs**: Downloads from integrated music API sources (via `moosicbox_music_api`)
- **Remote Library**: Downloads from remote MoosicBox servers

## Installation

### From Source

```bash
# Install system dependencies
sudo apt update
sudo apt install build-essential pkg-config libssl-dev

# Clone and build
git clone https://github.com/MoosicBox/MoosicBox.git
cd MoosicBox
cargo build --release --package moosicbox_downloader
```

### Cargo Dependencies

```toml
[dependencies]
moosicbox_downloader = { path = "../downloader" }
```

## Usage

### Basic Track Download

```rust
use moosicbox_downloader::{download, DownloadRequest, DownloadApiSource, TrackAudioQuality};
use moosicbox_music_api::MusicApis;
use moosicbox_music_models::id::Id;
use switchy_database::profiles::LibraryDatabase;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = LibraryDatabase { /* ... */ };
    let music_apis = MusicApis::default();

    // Download a single track
    let request = DownloadRequest {
        directory: PathBuf::from("./downloads"),
        track_id: Some(Id::from(123)),
        track_ids: None,
        album_id: None,
        album_ids: None,
        download_album_cover: Some(true),
        download_artist_cover: Some(true),
        quality: Some(TrackAudioQuality::FlacHighestRes),
        source: DownloadApiSource::Api(api_source),
    };

    download(request, db, music_apis).await?;

    Ok(())
}
```

### Download Multiple Tracks

```rust
use moosicbox_downloader::{download, DownloadRequest, DownloadApiSource};
use moosicbox_music_models::id::Id;
use std::path::PathBuf;

async fn download_tracks() -> Result<(), Box<dyn std::error::Error>> {
    let db = /* ... */;
    let music_apis = /* ... */;

    let request = DownloadRequest {
        directory: PathBuf::from("./downloads"),
        track_id: None,
        track_ids: Some(vec![Id::from(1), Id::from(2), Id::from(3)]),
        album_id: None,
        album_ids: None,
        download_album_cover: Some(true),
        download_artist_cover: Some(true),
        quality: Some(TrackAudioQuality::FlacHighestRes),
        source: DownloadApiSource::Api(api_source),
    };

    download(request, db, music_apis).await?;

    Ok(())
}
```

### Download Albums

```rust
use moosicbox_downloader::{download, DownloadRequest, DownloadApiSource};
use moosicbox_music_models::id::Id;
use std::path::PathBuf;

async fn download_album() -> Result<(), Box<dyn std::error::Error>> {
    let db = /* ... */;
    let music_apis = /* ... */;

    // Download entire album with covers
    let request = DownloadRequest {
        directory: PathBuf::from("./downloads"),
        track_id: None,
        track_ids: None,
        album_id: Some(Id::from(456)),
        album_ids: None,
        download_album_cover: Some(true),
        download_artist_cover: Some(true),
        quality: Some(TrackAudioQuality::FlacHighestRes),
        source: DownloadApiSource::Api(api_source),
    };

    download(request, db, music_apis).await?;

    Ok(())
}
```

### Download Queue with Progress Tracking

```rust
use moosicbox_downloader::queue::{DownloadQueue, ProgressEvent};
use std::sync::Arc;

async fn queue_downloads() -> Result<(), Box<dyn std::error::Error>> {
    let db = /* ... */;
    let downloader = /* ... */;

    // Create a download queue with progress listener
    let mut queue = DownloadQueue::new()
        .with_database(db)
        .with_downloader(Box::new(downloader))
        .add_progress_listener(Box::new(|event: &ProgressEvent| {
            Box::pin(async move {
                match event {
                    ProgressEvent::Size { task, bytes } => {
                        println!("Task {}: Size = {:?}", task.id, bytes);
                    }
                    ProgressEvent::Speed { task, bytes_per_second } => {
                        println!("Task {}: Speed = {:.2} MB/s",
                                 task.id, bytes_per_second / 1_000_000.0);
                    }
                    ProgressEvent::BytesRead { task, read, total } => {
                        let progress = (*read as f64 / *total as f64) * 100.0;
                        println!("Task {}: Progress = {:.1}%", task.id, progress);
                    }
                    ProgressEvent::State { task, state } => {
                        println!("Task {}: State = {:?}", task.id, state);
                    }
                }
            })
        }));

    // Add tasks to queue
    queue.add_tasks_to_queue(vec![/* DownloadTask instances */]).await;

    // Start processing
    queue.process();

    // Get current download speed
    if let Some(speed) = queue.speed() {
        println!("Current speed: {:.2} MB/s", speed / 1_000_000.0);
    }

    Ok(())
}
```

### Manual Download Operations

```rust
use moosicbox_downloader::{
    download_track_id, download_album_cover, download_artist_cover,
    DownloadApiSource, TrackAudioQuality,
};
use moosicbox_music_api::MusicApi;
use moosicbox_music_models::id::Id;
use std::sync::Arc;
use atomic_float::AtomicF64;

async fn manual_downloads(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
) -> Result<(), Box<dyn std::error::Error>> {
    let speed = Arc::new(AtomicF64::new(0.0));

    let on_progress = Arc::new(tokio::sync::Mutex::new(Box::new(|event| {
        Box::pin(async move {
            // Handle progress events
        }) as Pin<Box<dyn Future<Output = ()> + Send>>
    }) as Box<dyn FnMut(_) -> _ + Send + Sync>));

    // Download a single track
    let track = download_track_id(
        api,
        "./downloads/track.flac",
        &Id::from(123),
        TrackAudioQuality::FlacHighestRes,
        DownloadApiSource::Api(api_source),
        on_progress.clone(),
        speed.clone(),
        Some(Duration::from_secs(30)), // timeout
    ).await?;

    // Download album cover
    let album = download_album_cover(
        api,
        db,
        "./downloads/cover.jpg",
        &Id::from(456),
        on_progress.clone(),
        speed.clone(),
    ).await?;

    // Download artist cover
    let artist = download_artist_cover(
        api,
        db,
        "./downloads/artist.jpg",
        &Id::from(456), // album_id used to get artist
        on_progress.clone(),
        speed.clone(),
    ).await?;

    Ok(())
}
```

### Download Location Management

```rust
use moosicbox_downloader::{
    get_download_locations, get_download_location, create_download_location,
    delete_download_location, get_download_path, get_default_download_path,
};
use switchy_database::profiles::LibraryDatabase;

async fn manage_locations(db: &LibraryDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Get default download path
    let default_path = get_default_download_path()?;
    println!("Default download path: {:?}", default_path);

    // List all download locations
    let locations = get_download_locations(db).await?;
    for location in locations {
        println!("Location {}: {}", location.id, location.path);
    }

    // Get specific location
    if let Some(location) = get_download_location(db, 1).await? {
        println!("Location path: {}", location.path);
    }

    // Create new download location
    let new_location = create_download_location(db, "/path/to/downloads").await?;
    println!("Created location: {}", new_location.id);

    // Delete download location
    delete_download_location(db, "/path/to/downloads").await?;

    // Get download path (from location or default)
    let path = get_download_path(db, Some(1)).await?;
    println!("Download path: {:?}", path);

    Ok(())
}
```

## Configuration

### Download Directory

The default download directory is `~/.local/moosicbox/downloads`.

### Feature Flags

```toml
[dependencies.moosicbox_downloader]
path = "../downloader"
default-features = false
features = [
    "api",           # REST API endpoints
    "openapi",       # OpenAPI documentation
    "all-formats",   # All audio format support
    "decoder-aac",   # AAC decoder
    "decoder-flac",  # FLAC decoder
    "decoder-mp3",   # MP3 decoder
    "decoder-opus",  # Opus decoder
]
```

Available features:

- `api` - Enables REST API endpoints for download management
- `openapi` - Includes OpenAPI/Swagger documentation
- `all-formats` / `all-os-formats` - Enables all audio format support
- `format-aac` / `format-flac` / `format-mp3` / `format-opus` - Individual format support
- `decoder-aac` / `decoder-flac` / `decoder-mp3` / `decoder-opus` - Audio decoders
- `fail-on-warnings` - Treat compiler warnings as errors

## Programming Interface

### Core Types

```rust
// Download request structure
pub struct DownloadRequest {
    pub directory: PathBuf,
    pub track_id: Option<Id>,
    pub track_ids: Option<Vec<Id>>,
    pub album_id: Option<Id>,
    pub album_ids: Option<Vec<Id>>,
    pub download_album_cover: Option<bool>,
    pub download_artist_cover: Option<bool>,
    pub quality: Option<TrackAudioQuality>,
    pub source: DownloadApiSource,
}

// Download API source
pub enum DownloadApiSource {
    MoosicBox(String),  // Remote MoosicBox host URL
    Api(ApiSource),     // Integrated music API source
}

// Download item types
pub enum DownloadItem {
    Track {
        source: DownloadApiSource,
        track_id: Id,
        quality: TrackAudioQuality,
        artist_id: Id,
        artist: String,
        album_id: Id,
        album: String,
        title: String,
        contains_cover: bool,
    },
    AlbumCover {
        source: DownloadApiSource,
        artist_id: Id,
        artist: String,
        album_id: Id,
        title: String,
        contains_cover: bool,
    },
    ArtistCover {
        source: DownloadApiSource,
        artist_id: Id,
        album_id: Id,
        title: String,
        contains_cover: bool,
    },
}

// Download task state
pub enum DownloadTaskState {
    Pending,
    Paused,
    Cancelled,
    Started,
    Finished,
    Error,
}

// Download task
pub struct DownloadTask {
    pub id: u64,
    pub state: DownloadTaskState,
    pub item: DownloadItem,
    pub file_path: String,
    pub total_bytes: Option<u64>,
    pub created: String,
    pub updated: String,
}

// Progress events
pub enum ProgressEvent {
    Size { task: DownloadTask, bytes: Option<u64> },
    Speed { task: DownloadTask, bytes_per_second: f64 },
    BytesRead { task: DownloadTask, read: usize, total: usize },
    State { task: DownloadTask, state: DownloadTaskState },
}

// Audio quality options
pub enum TrackAudioQuality {
    Low,           // Low quality
    FlacLossless,  // FLAC lossless
    FlacHiRes,     // FLAC high resolution
    FlacHighestRes, // FLAC highest resolution
}
```

### Key Functions

```rust
// Main download function
pub async fn download(
    request: DownloadRequest,
    db: LibraryDatabase,
    music_apis: MusicApis,
) -> Result<(), DownloadError>

// Download individual track
pub async fn download_track_id(
    api: &dyn MusicApi,
    path: &str,
    track_id: &Id,
    quality: TrackAudioQuality,
    source: DownloadApiSource,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
    timeout_duration: Option<Duration>,
) -> Result<Track, DownloadTrackError>

// Download album
pub async fn download_album_id(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    path: &str,
    album_id: &Id,
    try_download_album_cover: bool,
    try_download_artist_cover: bool,
    quality: TrackAudioQuality,
    source: DownloadApiSource,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
    timeout_duration: Option<Duration>,
) -> Result<(), DownloadAlbumError>

// Download album cover
pub async fn download_album_cover(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    path: &str,
    album_id: &Id,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
) -> Result<Album, DownloadAlbumError>

// Download artist cover
pub async fn download_artist_cover(
    api: &dyn MusicApi,
    db: &LibraryDatabase,
    path: &str,
    album_id: &Id,
    on_progress: Arc<tokio::sync::Mutex<ProgressListener>>,
    speed: Arc<AtomicF64>,
) -> Result<Artist, DownloadAlbumError>

// Create download tasks
pub async fn create_download_tasks(
    db: &LibraryDatabase,
    tasks: Vec<CreateDownloadTask>,
) -> Result<Vec<DownloadTask>, CreateDownloadTasksError>

// Get download path
pub async fn get_download_path(
    db: &LibraryDatabase,
    location_id: Option<u64>,
) -> Result<PathBuf, GetDownloadPathError>

// Location management
pub async fn get_download_locations(
    db: &LibraryDatabase
) -> Result<Vec<DownloadLocation>, DatabaseFetchError>

pub async fn get_download_location(
    db: &LibraryDatabase,
    id: u64
) -> Result<Option<DownloadLocation>, DatabaseFetchError>

pub async fn create_download_location(
    db: &LibraryDatabase,
    path: &str
) -> Result<DownloadLocation, DatabaseFetchError>

pub async fn delete_download_location(
    db: &LibraryDatabase,
    path: &str
) -> Result<Option<DownloadLocation>, DatabaseFetchError>
```

## Implementation Details

### File Organization

Downloaded files are automatically organized in the following structure:

```
{download_path}/
  {artist_name}/
    {album_name}/
      01_track_name.flac
      02_track_name.flac
      cover.jpg
    artist.jpg
```

Filenames are automatically sanitized to be filesystem-safe.

### Audio Tagging

Downloaded audio files are automatically tagged with metadata:

- Title
- Track number
- Album title
- Artist name
- Album artist
- Release date (when available)

### Resume Support

Downloads automatically resume from where they left off if interrupted. The downloader:

- Checks existing file size
- Uses HTTP range requests to resume from the last byte
- Retries on timeout (via recursive retry logic)

### Automatic Scanning

When `scan` is enabled on the download queue (default), completed downloads are automatically scanned and added to the local music library.

## Architecture

The downloader consists of several key components:

1. **Download Queue** (`queue.rs`): Manages background download processing with progress tracking
2. **Downloader Trait**: Abstract interface for different download implementations
3. **MoosicboxDownloader**: Concrete implementation using MoosicBox music APIs
4. **Database Models** (`db/models.rs`): Download tasks, locations, and state tracking
5. **API Models** (`api/models.rs`): REST API request/response types (when `api` feature enabled)

## See Also

- [MoosicBox Files](../files/README.md) - File handling and streaming
- [MoosicBox Music API](../music_api/README.md) - Music API abstractions
- [MoosicBox Server](../server/README.md) - Main server with download support
- [MoosicBox Scan](../scan/README.md) - Library scanning functionality
