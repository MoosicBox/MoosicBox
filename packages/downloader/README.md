# MoosicBox Downloader

High-performance file downloading and transfer management system for the MoosicBox ecosystem.

## Overview

The MoosicBox Downloader package provides:

- **Multi-Protocol Support**: HTTP/HTTPS, FTP, SFTP, and custom protocols
- **Parallel Downloads**: Concurrent downloading with connection pooling
- **Resume Support**: Resume interrupted downloads automatically
- **Progress Tracking**: Real-time download progress and statistics
- **Queue Management**: Download queue with prioritization and scheduling
- **Bandwidth Control**: Rate limiting and bandwidth management
- **Integrity Verification**: Checksum validation and error detection
- **Retry Logic**: Configurable retry strategies for failed downloads

## Features

### Download Protocols
- **HTTP/HTTPS**: Standard web downloads with authentication support
- **FTP/SFTP**: File transfer protocol support
- **Custom Protocols**: Extensible protocol system
- **Streaming Support**: Stream downloads without disk buffering
- **Range Requests**: Partial content downloads and resumption

### Performance Features
- **Concurrent Downloads**: Multiple simultaneous downloads
- **Connection Pooling**: Reuse connections for efficiency
- **Chunk Downloading**: Split large files into chunks
- **Compression Support**: Automatic decompression of compressed content
- **Memory Efficient**: Configurable memory usage limits

### Management Features
- **Download Queue**: Organized queue with priorities
- **Scheduling**: Schedule downloads for specific times
- **Progress Monitoring**: Detailed progress reporting
- **Error Handling**: Comprehensive error recovery
- **Metadata Extraction**: Extract file information during download

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

# Optional: Enable specific features
moosicbox_downloader = {
    path = "../downloader",
    features = ["ftp", "sftp", "compression", "encryption"]
}
```

## Usage

### Basic Downloads

```rust
use moosicbox_downloader::{Downloader, DownloadConfig, DownloadRequest};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create downloader with default configuration
    let downloader = Downloader::new(DownloadConfig::default()).await?;

    // Simple download
    let request = DownloadRequest {
        url: "https://example.com/file.mp3".to_string(),
        destination: Path::new("./downloads/file.mp3").to_path_buf(),
        ..Default::default()
    };

    let download_id = downloader.start_download(request).await?;

    // Wait for completion
    let result = downloader.wait_for_completion(download_id).await?;

    match result {
        Ok(info) => {
            println!("Download completed: {} bytes in {:.2}s",
                     info.bytes_downloaded,
                     info.duration.as_secs_f64());
        },
        Err(e) => {
            eprintln!("Download failed: {}", e);
        }
    }

    Ok(())
}
```

### Advanced Configuration

```rust
use moosicbox_downloader::{
    Downloader, DownloadConfig, DownloadRequest, RetryStrategy,
    ProgressCallback, AuthConfig
};

async fn advanced_download() -> Result<(), Box<dyn std::error::Error>> {
    // Configure downloader
    let config = DownloadConfig {
        max_concurrent_downloads: 4,
        max_connections_per_host: 2,
        connection_timeout: Duration::from_secs(30),
        read_timeout: Duration::from_secs(60),
        max_redirects: 10,
        user_agent: "MoosicBox/1.0".to_string(),
        buffer_size: 64 * 1024, // 64KB buffer
        max_memory_usage: 100 * 1024 * 1024, // 100MB limit
        enable_compression: true,
        verify_ssl: true,
    };

    let downloader = Downloader::new(config).await?;

    // Advanced download request
    let request = DownloadRequest {
        url: "https://example.com/large-file.zip".to_string(),
        destination: Path::new("./downloads/large-file.zip").to_path_buf(),

        // Authentication
        auth: Some(AuthConfig::Bearer {
            token: "your-access-token".to_string(),
        }),

        // Custom headers
        headers: vec![
            ("X-Custom-Header".to_string(), "custom-value".to_string()),
        ],

        // Retry configuration
        retry_strategy: RetryStrategy {
            max_attempts: 3,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
            retry_on_status: vec![408, 429, 500, 502, 503, 504],
        },

        // Progress tracking
        progress_callback: Some(Box::new(|progress| {
            println!("Progress: {:.1}% ({}/{} bytes) - Speed: {}/s",
                     progress.percentage * 100.0,
                     progress.bytes_downloaded,
                     progress.total_bytes.unwrap_or(0),
                     format_bytes(progress.speed_bps));
        })),

        // Integrity verification
        expected_checksum: Some("sha256:abc123...".to_string()),

        // Resume support
        resume_if_exists: true,

        // Priority and scheduling
        priority: 5, // Higher numbers = higher priority
        scheduled_time: None, // Download immediately
    };

    let download_id = downloader.start_download(request).await?;

    // Monitor download
    let mut progress_stream = downloader.get_progress_stream(download_id).await?;

    while let Some(progress) = progress_stream.next().await {
        match progress {
            DownloadEvent::Progress(info) => {
                println!("Downloaded: {:.1}%", info.percentage * 100.0);
            },
            DownloadEvent::Completed(result) => {
                println!("Download completed successfully");
                break;
            },
            DownloadEvent::Failed(error) => {
                eprintln!("Download failed: {}", error);
                break;
            },
            DownloadEvent::Paused => {
                println!("Download paused");
            },
            DownloadEvent::Resumed => {
                println!("Download resumed");
            },
        }
    }

    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    format!("{:.1} {}", size, UNITS[unit_index])
}
```

### Batch Downloads

```rust
use moosicbox_downloader::{BatchDownloader, BatchConfig, DownloadBatch};

async fn batch_download() -> Result<(), Box<dyn std::error::Error>> {
    let config = BatchConfig {
        max_concurrent: 3,
        continue_on_error: true,
        create_subdirectories: true,
        overwrite_existing: false,
    };

    let batch_downloader = BatchDownloader::new(config).await?;

    // Create download batch
    let mut batch = DownloadBatch::new("music_collection");

    // Add multiple downloads
    let urls = vec![
        "https://example.com/song1.mp3",
        "https://example.com/song2.mp3",
        "https://example.com/song3.mp3",
        "https://example.com/album.zip",
    ];

    for (i, url) in urls.iter().enumerate() {
        batch.add_download(DownloadRequest {
            url: url.to_string(),
            destination: Path::new(&format!("./downloads/file_{}.mp3", i + 1)).to_path_buf(),
            priority: 10 - i, // Higher priority for earlier files
            ..Default::default()
        });
    }

    // Start batch download
    let batch_id = batch_downloader.start_batch(batch).await?;

    // Monitor batch progress
    let mut batch_progress = batch_downloader.get_batch_progress(batch_id).await?;

    while let Some(update) = batch_progress.next().await {
        match update {
            BatchEvent::Progress { completed, total, bytes } => {
                println!("Batch progress: {}/{} files, {} total bytes",
                         completed, total, format_bytes(bytes));
            },
            BatchEvent::FileCompleted { filename, size } => {
                println!("Completed: {} ({} bytes)", filename, format_bytes(size));
            },
            BatchEvent::FileFailed { filename, error } => {
                eprintln!("Failed: {} - {}", filename, error);
            },
            BatchEvent::Completed { stats } => {
                println!("Batch completed: {} files, {} bytes, {:.2}s",
                         stats.files_completed,
                         format_bytes(stats.total_bytes),
                         stats.total_duration.as_secs_f64());
                break;
            },
        }
    }

    Ok(())
}
```

### Queue Management

```rust
use moosicbox_downloader::{DownloadQueue, QueueConfig, QueueCommand};

async fn manage_download_queue() -> Result<(), Box<dyn std::error::Error>> {
    let config = QueueConfig {
        max_concurrent_downloads: 2,
        max_queue_size: 100,
        auto_start: true,
        priority_scheduling: true,
        bandwidth_limit: Some(1024 * 1024), // 1 MB/s limit
    };

    let mut queue = DownloadQueue::new(config).await?;

    // Add downloads to queue
    for i in 1..=10 {
        let request = DownloadRequest {
            url: format!("https://example.com/file{}.mp3", i),
            destination: Path::new(&format!("./downloads/file{}.mp3", i)).to_path_buf(),
            priority: if i <= 3 { 10 } else { 5 }, // High priority for first 3
            ..Default::default()
        };

        queue.enqueue(request).await?;
    }

    // Control queue
    queue.send_command(QueueCommand::Pause).await?;
    println!("Queue paused");

    tokio::time::sleep(Duration::from_secs(2)).await;

    queue.send_command(QueueCommand::Resume).await?;
    println!("Queue resumed");

    // Monitor queue status
    let mut status_stream = queue.get_status_stream().await?;

    while let Some(status) = status_stream.next().await {
        println!("Queue status: {} active, {} pending, {} completed",
                 status.active_downloads,
                 status.pending_downloads,
                 status.completed_downloads);

        if status.pending_downloads == 0 && status.active_downloads == 0 {
            break;
        }
    }

    Ok(())
}
```

### Custom Protocols

```rust
use moosicbox_downloader::{Protocol, ProtocolHandler, DownloadStream};
use async_trait::async_trait;

// Custom protocol handler
struct CustomProtocolHandler;

#[async_trait]
impl ProtocolHandler for CustomProtocolHandler {
    async fn can_handle(&self, url: &str) -> bool {
        url.starts_with("custom://")
    }

    async fn start_download(
        &self,
        url: &str,
        config: &DownloadConfig
    ) -> Result<DownloadStream, DownloadError> {
        // Parse custom URL
        let path = url.strip_prefix("custom://").unwrap();

        // Create custom download stream
        let stream = create_custom_stream(path).await?;

        Ok(DownloadStream {
            content_length: stream.size(),
            content_type: Some("application/octet-stream".to_string()),
            stream: Box::pin(stream),
        })
    }

    async fn supports_resume(&self) -> bool {
        true // Custom protocol supports resume
    }

    async fn get_file_info(&self, url: &str) -> Result<FileInfo, DownloadError> {
        let path = url.strip_prefix("custom://").unwrap();

        // Get file information from custom source
        Ok(FileInfo {
            size: get_custom_file_size(path).await?,
            last_modified: get_custom_file_modified(path).await?,
            content_type: Some("application/octet-stream".to_string()),
            supports_ranges: true,
        })
    }
}

async fn register_custom_protocol() -> Result<(), Box<dyn std::error::Error>> {
    let mut downloader = Downloader::new(DownloadConfig::default()).await?;

    // Register custom protocol
    downloader.register_protocol("custom", Box::new(CustomProtocolHandler)).await?;

    // Use custom protocol
    let request = DownloadRequest {
        url: "custom://path/to/file".to_string(),
        destination: Path::new("./downloads/custom_file").to_path_buf(),
        ..Default::default()
    };

    let download_id = downloader.start_download(request).await?;
    let result = downloader.wait_for_completion(download_id).await?;

    println!("Custom protocol download completed: {:?}", result);

    Ok(())
}

async fn create_custom_stream(path: &str) -> Result<impl AsyncRead, DownloadError> {
    // Your custom stream implementation
    todo!()
}

async fn get_custom_file_size(path: &str) -> Result<u64, DownloadError> {
    // Your custom file size implementation
    todo!()
}

async fn get_custom_file_modified(path: &str) -> Result<Option<SystemTime>, DownloadError> {
    // Your custom file modified time implementation
    todo!()
}
```

### Streaming Downloads

```rust
use moosicbox_downloader::{StreamingDownloader, StreamConfig};
use tokio::io::AsyncWriteExt;

async fn streaming_download() -> Result<(), Box<dyn std::error::Error>> {
    let config = StreamConfig {
        buffer_size: 32 * 1024, // 32KB buffer
        max_memory_usage: 10 * 1024 * 1024, // 10MB limit
        enable_compression: true,
    };

    let streaming_downloader = StreamingDownloader::new(config);

    // Stream download without saving to disk
    let mut stream = streaming_downloader.stream_url(
        "https://example.com/large-file.mp3"
    ).await?;

    // Process stream data
    let mut total_bytes = 0;
    let mut buffer = vec![0u8; 8192];

    while let Ok(bytes_read) = stream.read(&mut buffer).await {
        if bytes_read == 0 {
            break;
        }

        total_bytes += bytes_read;

        // Process the downloaded chunk
        process_audio_chunk(&buffer[..bytes_read]).await?;

        if total_bytes % (1024 * 1024) == 0 {
            println!("Streamed {} MB", total_bytes / (1024 * 1024));
        }
    }

    println!("Streaming completed: {} total bytes", total_bytes);

    Ok(())
}

async fn process_audio_chunk(chunk: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    // Process audio chunk (decode, play, etc.)
    println!("Processing {} bytes", chunk.len());
    Ok(())
}
```

### Download Resumption

```rust
use moosicbox_downloader::{ResumeManager, ResumeInfo};

async fn resumable_downloads() -> Result<(), Box<dyn std::error::Error>> {
    let downloader = Downloader::new(DownloadConfig::default()).await?;
    let resume_manager = ResumeManager::new("./downloads/.resume").await?;

    // Start download with resume support
    let request = DownloadRequest {
        url: "https://example.com/large-file.zip".to_string(),
        destination: Path::new("./downloads/large-file.zip").to_path_buf(),
        resume_if_exists: true,
        ..Default::default()
    };

    let download_id = downloader.start_download(request).await?;

    // Simulate interruption after 5 seconds
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(5)).await;
        let _ = downloader.cancel_download(download_id).await;
        println!("Download interrupted");
    });

    // Wait a bit, then resume
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Check for resumable downloads
    let resumable = resume_manager.list_resumable().await?;

    for resume_info in resumable {
        println!("Found resumable download: {} ({} bytes downloaded)",
                 resume_info.url, resume_info.bytes_downloaded);

        // Resume the download
        let resume_request = DownloadRequest {
            url: resume_info.url,
            destination: resume_info.destination,
            resume_if_exists: true,
            ..Default::default()
        };

        let new_download_id = downloader.start_download(resume_request).await?;
        let result = downloader.wait_for_completion(new_download_id).await?;

        match result {
            Ok(info) => {
                println!("Resume completed: {} total bytes", info.bytes_downloaded);
            },
            Err(e) => {
                eprintln!("Resume failed: {}", e);
            }
        }
    }

    Ok(())
}
```

## Configuration

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `MOOSICBOX_DOWNLOAD_DIR` | Default download directory | `./downloads` |
| `MOOSICBOX_DOWNLOAD_CONCURRENT` | Max concurrent downloads | `4` |
| `MOOSICBOX_DOWNLOAD_TIMEOUT` | Connection timeout (seconds) | `30` |
| `MOOSICBOX_DOWNLOAD_RETRIES` | Max retry attempts | `3` |
| `MOOSICBOX_DOWNLOAD_BUFFER_SIZE` | Buffer size (bytes) | `65536` |
| `MOOSICBOX_DOWNLOAD_BANDWIDTH_LIMIT` | Bandwidth limit (bytes/sec) | `0` (unlimited) |

### Configuration File

```toml
# ~/.config/moosicbox/downloader.toml
[downloader]
max_concurrent_downloads = 4
max_connections_per_host = 2
connection_timeout = 30
read_timeout = 60
max_redirects = 10
user_agent = "MoosicBox/1.0"
buffer_size = 65536
max_memory_usage = 104857600
enable_compression = true
verify_ssl = true

[retry]
max_attempts = 3
initial_delay = 1000
max_delay = 30000
backoff_multiplier = 2.0
retry_on_status = [408, 429, 500, 502, 503, 504]

[queue]
max_concurrent_downloads = 2
max_queue_size = 100
auto_start = true
priority_scheduling = true
bandwidth_limit = 1048576

[resume]
enabled = true
resume_directory = "./downloads/.resume"
cleanup_completed = true
cleanup_after_days = 7

[protocols.http]
enabled = true
follow_redirects = true
max_redirects = 10

[protocols.ftp]
enabled = true
passive_mode = true
connection_timeout = 30

[protocols.sftp]
enabled = false
key_file = "~/.ssh/id_rsa"
```

### Feature Flags

```toml
[dependencies.moosicbox_downloader]
path = "../downloader"
default-features = false
features = [
    "http",          # HTTP/HTTPS support
    "ftp",           # FTP protocol support
    "sftp",          # SFTP protocol support
    "compression",   # Automatic decompression
    "encryption",    # Encryption support
    "resume",        # Download resumption
    "progress",      # Progress tracking
    "queue",         # Download queue management
    "batch",         # Batch downloads
    "streaming",     # Streaming downloads
]
```

## Programming Interface

### Core Types

```rust
use moosicbox_downloader::*;

// Download configuration
pub struct DownloadConfig {
    pub max_concurrent_downloads: usize,
    pub max_connections_per_host: usize,
    pub connection_timeout: Duration,
    pub read_timeout: Duration,
    pub max_redirects: usize,
    pub user_agent: String,
    pub buffer_size: usize,
    pub max_memory_usage: usize,
    pub enable_compression: bool,
    pub verify_ssl: bool,
}

// Download request
pub struct DownloadRequest {
    pub url: String,
    pub destination: PathBuf,
    pub auth: Option<AuthConfig>,
    pub headers: Vec<(String, String)>,
    pub retry_strategy: RetryStrategy,
    pub progress_callback: Option<ProgressCallback>,
    pub expected_checksum: Option<String>,
    pub resume_if_exists: bool,
    pub priority: u8,
    pub scheduled_time: Option<SystemTime>,
}

// Authentication configuration
pub enum AuthConfig {
    Basic { username: String, password: String },
    Bearer { token: String },
    ApiKey { key: String, header: String },
    Custom { headers: Vec<(String, String)> },
}

// Download events
pub enum DownloadEvent {
    Started { download_id: DownloadId },
    Progress(ProgressInfo),
    Paused,
    Resumed,
    Completed(DownloadResult),
    Failed(DownloadError),
}
```

## Troubleshooting

### Common Issues

1. **Downloads failing with SSL errors**
   ```rust
   // Disable SSL verification for testing
   let config = DownloadConfig {
       verify_ssl: false,
       ..Default::default()
   };
   ```

2. **Slow download speeds**
   ```rust
   // Increase concurrent connections
   let config = DownloadConfig {
       max_connections_per_host: 4,
       buffer_size: 128 * 1024, // Larger buffer
       ..Default::default()
   };
   ```

3. **Downloads timing out**
   ```rust
   // Increase timeouts
   let config = DownloadConfig {
       connection_timeout: Duration::from_secs(60),
       read_timeout: Duration::from_secs(120),
       ..Default::default()
   };
   ```

4. **Resume not working**
   ```bash
   # Check resume directory permissions
   ls -la ./downloads/.resume

   # Verify server supports range requests
   curl -I -H "Range: bytes=0-1023" https://example.com/file.zip
   ```

## See Also

- [MoosicBox Files](../files/README.md) - File handling and streaming
- [MoosicBox HTTP](../http/README.md) - HTTP client utilities
- [MoosicBox Server](../server/README.md) - Main server with download support
