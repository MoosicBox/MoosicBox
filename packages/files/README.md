# MoosicBox Files

A comprehensive file handling and streaming utility library for the MoosicBox ecosystem. Provides functions for downloading, saving, and managing files with support for progress monitoring, cover art extraction, audio track streaming, and HTTP API endpoints for serving media content.

## Features

Core features (always available):

- **File Download**: Download files from remote URLs with HTTP client support
- **Stream Saving**: Save byte streams to files with progress monitoring
- **Progress Tracking**: Monitor download progress and speed during file operations
- **Cover Art Handling**: Extract and save album cover art from audio tags
- **Content Length Detection**: Get remote file sizes via HTTP HEAD requests
- **Filename Sanitization**: Clean filenames for filesystem compatibility
- **Error Handling**: Detailed error types for different failure scenarios

With optional features enabled:

- **Track Management**: Handle audio track files, metadata, and streaming with pooling support (requires `files` feature)
- **Album/Artist Artwork**: Fetch, cache, and manage album and artist cover images (requires `files` feature)
- **HTTP Range Support**: Parse and handle HTTP byte range requests (requires `range` feature)
- **REST API Endpoints**: Actix-web endpoints for serving files, tracks, and artwork (requires `api` feature)
- **Audio Codec Support**: Decode/encode various audio formats including AAC, FLAC, MP3, and Opus (requires decoder/encoder features)

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_files = "0.1.4"

# Enable additional features
moosicbox_files = { version = "0.1.4", features = ["files", "range", "api"] }
```

## Usage

### Basic File Operations

```rust
use moosicbox_files::{save_bytes_to_file, sanitize_filename};
use std::path::Path;

fn main() -> Result<(), std::io::Error> {
    // Sanitize filename for filesystem compatibility
    let safe_filename = sanitize_filename("My Song: The Best! (2023)");
    println!("Safe filename: {}", safe_filename); // "My_Song__The_Best___2023_"

    // Save bytes to file
    let data = b"Hello, world!";
    let path = Path::new("output.txt");
    save_bytes_to_file(data, path, None)?;

    // Save bytes starting at specific position
    let more_data = b"Additional content";
    save_bytes_to_file(more_data, path, Some(13))?; // Append after "Hello, world!"

    Ok(())
}
```

### Downloading Files from URLs

```rust
use moosicbox_files::{fetch_and_save_bytes_from_remote_url, get_content_length};
use switchy_http::Client;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let url = "https://example.com/file.mp3";
    let path = Path::new("downloaded_file.mp3");

    // Get file size before downloading
    let content_length = get_content_length(url, None, None).await?;
    if let Some(size) = content_length {
        println!("File size: {} bytes", size);
    }

    // Download and save file
    let saved_path = fetch_and_save_bytes_from_remote_url(&client, path, url, None).await?;
    println!("File saved to: {}", saved_path.display());

    Ok(())
}
```

### Streaming with Progress Monitoring

```rust
use moosicbox_files::{fetch_bytes_from_remote_url, save_bytes_stream_to_file_with_progress_listener};
use switchy_http::Client;
use std::path::Path;
use std::pin::Pin;
use std::future::Future;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let url = "https://example.com/large_file.zip";
    let path = Path::new("large_file.zip");

    // Get stream from URL
    let stream = fetch_bytes_from_remote_url(&client, url, None).await?;

    // Save with progress monitoring
    let progress_callback = Box::new(|bytes_read: usize, total_bytes: usize| {
        let percentage = if total_bytes > 0 {
            (bytes_read as f64 / total_bytes as f64) * 100.0
        } else {
            0.0
        };
        println!("Downloaded: {} / {} bytes ({:.1}%)", bytes_read, total_bytes, percentage);
        Box::pin(async {}) as Pin<Box<dyn Future<Output = ()> + Send>>
    });

    save_bytes_stream_to_file_with_progress_listener(
        stream,
        path,
        None,
        Some(progress_callback),
    ).await?;

    println!("Download completed!");
    Ok(())
}
```

### Speed Monitoring

```rust
use moosicbox_files::{fetch_bytes_from_remote_url, save_bytes_stream_to_file_with_speed_listener};
use switchy_http::Client;
use std::path::Path;
use std::pin::Pin;
use std::future::Future;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let url = "https://example.com/file.mp4";
    let path = Path::new("file.mp4");

    let stream = fetch_bytes_from_remote_url(&client, url, None).await?;

    // Monitor download speed
    let speed_callback = Box::new(|speed_bps: f64| {
        let speed_kbps = speed_bps / 1024.0;
        let speed_mbps = speed_kbps / 1024.0;
        println!("Download speed: {:.2} Mbps ({:.2} KB/s)", speed_mbps, speed_kbps);
        Box::pin(async {}) as Pin<Box<dyn Future<Output = ()> + Send>>
    });

    let progress_callback = Some(Box::new(|read: usize, total: usize| {
        println!("Progress: {} / {} bytes", read, total);
        Box::pin(async {}) as Pin<Box<dyn Future<Output = ()> + Send>>
    }));

    save_bytes_stream_to_file_with_speed_listener(
        stream,
        path,
        None,
        speed_callback,
        progress_callback,
    ).await?;

    Ok(())
}
```

### Cover Art Handling

```rust
use moosicbox_files::search_for_cover;
use moosicbox_audiotags::AudioTag;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let music_dir = PathBuf::from("/path/to/music/album");
    let filename = "album";
    let save_path = Some(PathBuf::from("/path/to/covers"));

    // Search for existing cover or extract from audio tags
    let cover_path = search_for_cover(
        music_dir,
        filename,
        save_path,
        None, // Could pass audio tag here for extraction
    ).await?;

    match cover_path {
        Some(path) => println!("Cover found/created at: {}", path.display()),
        None => println!("No cover art available"),
    }

    Ok(())
}
```

### Custom Headers and Range Requests

```rust
use moosicbox_files::{fetch_bytes_from_remote_url, get_content_length};
use switchy_http::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let url = "https://api.example.com/protected/file.mp3";

    // Custom headers for authentication
    let headers = vec![
        ("Authorization".to_string(), "Bearer token123".to_string()),
        ("User-Agent".to_string(), "MoosicBox/1.0".to_string()),
    ];

    // Get partial content length (range request)
    let partial_length = get_content_length(url, Some(0), Some(1023)).await?;
    println!("First 1024 bytes available: {:?}", partial_length);

    // Fetch with custom headers
    let stream = fetch_bytes_from_remote_url(&client, url, Some(&headers)).await?;

    // Process stream...
    Ok(())
}
```

## API Reference

### Core Functions (always available)

- `save_bytes_to_file()` - Save byte array to file with optional offset
- `save_bytes_stream_to_file()` - Save async byte stream to file
- `fetch_bytes_from_remote_url()` - Get byte stream from URL
- `fetch_and_save_bytes_from_remote_url()` - Download and save in one operation
- `get_content_length()` - Get remote file size via HTTP HEAD
- `sanitize_filename()` - Clean filename for filesystem use

### Progress Monitoring (always available)

- `save_bytes_stream_to_file_with_progress_listener()` - Save with progress callbacks
- `save_bytes_stream_to_file_with_speed_listener()` - Save with speed monitoring

### Cover Art Support (always available)

- `search_for_cover()` - Find existing cover or extract from audio tags

### Additional Modules (feature-gated)

With the `files` feature enabled:

- `files::track` - Track file management and streaming
- `files::album` - Album cover art handling
- `files::artist` - Artist artwork handling
- `files::track_pool` - Track pooling and caching

With the `api` feature enabled:

- `api` - Actix-web endpoints for file serving, track streaming, and artwork delivery

With the `range` feature enabled:

- `range` - HTTP byte range parsing utilities

## Error Handling

The library provides specific error types for different operations:

- `GetContentLengthError` - HTTP or parsing errors when getting content length
- `SaveBytesStreamToFileError` - IO errors during stream-to-file operations
- `FetchAndSaveBytesFromRemoteUrlError` - Network or file errors during downloads
- `FetchCoverError` - Errors when handling cover art operations

## Cargo Features

The package provides several optional features:

- `files` - Enable track management, album/artist handling, and audio file pooling
- `range` - Enable HTTP range request support
- `api` - Enable Actix-web API endpoints for file serving (implies `files` and `range`)
- `openapi` - Enable OpenAPI/utoipa documentation support
- `image` - Enable image processing with `moosicbox_image`
- `libvips` - Enable libvips backend for image processing
- `profiling` - Enable profiling support

Audio format features (decoder/encoder support):

- `all-decoders` / `all-encoders` - Enable all supported codecs
- `decoder-aac`, `decoder-flac`, `decoder-mp3`, `decoder-opus` - Individual decoder features
- `encoder-aac`, `encoder-flac`, `encoder-mp3`, `encoder-opus` - Individual encoder features

## Dependencies

Core dependencies:

- `switchy_http` - HTTP client functionality with streaming support
- `switchy_fs` - Cross-platform async filesystem operations
- `switchy_time` - Cross-platform time utilities
- `moosicbox_audiotags` - Audio tag parsing for cover art extraction
- `moosicbox_stream_utils` - Stream monitoring and utilities
- `moosicbox_config` - Configuration management
- `tokio` - Async runtime support
- `bytes` - Efficient byte buffer management
- `futures` - Asynchronous stream processing

Optional dependencies (enabled by features):

- `actix-web` / `actix-files` - Web server framework (with `api` feature)
- `moosicbox_music_api` / `moosicbox_music_models` - Music API integration (with `files` feature)
- `moosicbox_audio_decoder` / `moosicbox_audio_output` - Audio processing (with decoder/encoder features)
- `moosicbox_image` - Image processing (with `image` feature)
- `utoipa` - OpenAPI documentation (with `openapi` feature)
