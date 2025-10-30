# Basic Usage Example

This example demonstrates the fundamental usage patterns for the `moosicbox_downloader` package, showing how to set up and use the download queue system for managing music downloads.

## Summary

A comprehensive walkthrough of setting up a download queue, creating download tasks, and monitoring progress in real-time. This example illustrates the complete workflow from configuration to execution, including progress tracking and file organization.

## What This Example Demonstrates

- Setting up download parameters (directory, quality, source)
- Creating download tasks for tracks and albums programmatically
- Configuring a download queue with database and music APIs
- Implementing progress listeners for real-time monitoring
- Understanding the download workflow and state management
- File organization patterns used by the downloader
- Audio quality options and their implications
- Advanced features like resume support and timeout handling

## Prerequisites

- Basic understanding of Rust async/await patterns
- Familiarity with the MoosicBox ecosystem architecture
- Understanding of music metadata and audio formats
- Knowledge of database concepts for task persistence

**Note**: This example is conceptual and demonstrates the API structure. A fully functional implementation requires:

- A configured `LibraryDatabase` connection
- Initialized `MusicApis` with valid API credentials
- Valid track/album IDs from a music source
- Network access to download sources

## Running the Example

```bash
cargo run --manifest-path packages/downloader/examples/basic_usage/Cargo.toml
```

Or from the repository root:

```bash
cargo run -p moosicbox_downloader_basic_usage_example
```

## Expected Output

The example will print a step-by-step walkthrough of the download workflow:

```
MoosicBox Downloader - Basic Usage Example
===========================================

Step 1: Configuring download parameters...
  Download directory: "./downloads"
  Audio quality: FlacHighestRes
  Download source: Api(Library)

Step 2: Setting up database and music APIs...
  (In a real application, initialize LibraryDatabase and MusicApis here)
  Note: This example requires a configured database and music APIs

Step 3: Creating download tasks...
  Example track IDs: 123, 456, 789
  Download album covers: true
  Download artist covers: true

Step 4: Setting up download queue with progress listener...
  Progress listener configured
  Queue will process tasks sequentially

Step 5: Understanding the download workflow...
  1. Tasks are queued in the order they are added
  2. The queue processes one task at a time
  3. Each download includes:
     - Fetching track metadata from the music API
     - Downloading audio data with resume support
     - Automatic ID3 tag writing (artist, album, title, etc.)
     - Downloading cover art (if requested)
     - Organizing files by artist/album/track
  4. Progress events are fired throughout the download
  5. Downloads can resume if interrupted

Step 6: Advanced features...
  - Resume Support: Downloads automatically resume from where they stopped
  - Timeout Handling: Automatic retry with configurable timeouts
  - Concurrent Operations: Multiple listeners can track progress
  - Database Persistence: Tasks are stored and can survive restarts
  - Automatic Scanning: Completed downloads are added to the library

Step 7: File organization...
  Downloaded files are organized as:
  {download_path}/
    {artist_name}/
      {album_name}/
        01_track_name.flac
        02_track_name.flac
        cover.jpg
      artist.jpg

Step 8: Audio quality options...
  - TrackAudioQuality::Low          - Lower bitrate for faster downloads
  - TrackAudioQuality::FlacLossless - CD-quality FLAC (16-bit/44.1kHz)
  - TrackAudioQuality::FlacHiRes    - High-resolution FLAC (24-bit/96kHz)
  - TrackAudioQuality::FlacHighestRes - Highest available quality (24-bit/192kHz)

Example complete!

To use this in your application:
1. Set up a LibraryDatabase connection
2. Initialize MusicApis with your API credentials
3. Get valid track/album IDs from your music source
4. Call the functions as shown above with real data

See the README.md for more details and working code snippets.
```

## Code Walkthrough

### Setting Up Download Parameters

The first step is configuring where files should be downloaded and at what quality:

```rust
let download_path = PathBuf::from("./downloads");
let quality = TrackAudioQuality::FlacHighestRes;
let api_source = ApiSource::library();
let source = DownloadApiSource::Api(api_source.clone());
```

**Key Points**:

- `download_path` determines the root directory for all downloads
- `quality` affects file size, download time, and audio fidelity
- `source` specifies which music API to use (local library, Tidal, Qobuz, etc.)

### Creating Download Tasks

Download tasks are created by specifying what to download:

```rust
// In a real application:
let tasks = get_create_download_tasks(
    &*music_api,
    &download_path,
    None,                    // Single track ID
    Some(track_ids),         // Multiple track IDs
    None,                    // Single album ID
    None,                    // Multiple album IDs
    true,                    // Download album covers
    true,                    // Download artist covers
    Some(quality),
    Some(source.clone()),
).await?;

let tasks = create_download_tasks(&db, tasks).await?;
```

**Key Points**:

- You can download individual tracks or entire albums
- Album and artist covers are optional but recommended
- Tasks are persisted to the database for reliability

### Configuring the Download Queue

The download queue manages task processing with progress tracking:

```rust
let progress_listener = Box::new(|event: &ProgressEvent| {
    Box::pin(async move {
        match event {
            ProgressEvent::Size { task, bytes } => {
                println!("Size: {:?} bytes", bytes);
            }
            ProgressEvent::Speed { task, bytes_per_second } => {
                println!("Speed: {:.2} MB/s", bytes_per_second / 1_000_000.0);
            }
            ProgressEvent::BytesRead { task, read, total } => {
                let progress = ((*read as f64) / (*total as f64)) * 100.0;
                println!("Progress: {:.1}%", progress);
            }
            ProgressEvent::State { task, state } => {
                println!("State: {:?}", state);
            }
        }
    }) as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
}) as Box<dyn Fn(&ProgressEvent) -> _ + Send + Sync>;

let mut queue = DownloadQueue::new()
    .with_database(db)
    .with_downloader(Box::new(downloader))
    .add_progress_listener(progress_listener);

queue.add_tasks_to_queue(tasks).await;
queue.process();
```

**Key Points**:

- Progress listeners receive events for each download task
- The queue processes tasks sequentially in FIFO order
- Multiple progress listeners can be attached to the same queue

### Progress Event Types

The example demonstrates four types of progress events:

1. **Size Event**: Fired when the total download size is determined
2. **Speed Event**: Fired periodically with current download speed
3. **BytesRead Event**: Fired as data is downloaded, showing progress
4. **State Event**: Fired when task state changes (Pending → Started → Finished)

## Key Concepts

### Download Queue Architecture

The download queue is the central component for managing downloads:

- **Sequential Processing**: Tasks are processed one at a time to manage resources
- **State Management**: Tasks progress through states (Pending → Started → Finished)
- **Progress Tracking**: Real-time updates on download progress and speed
- **Database Persistence**: Tasks survive application restarts

### File Organization

Downloaded files are automatically organized in a hierarchical structure:

```
downloads/
  Artist Name/
    Album Name/
      01_Track_Name.flac
      02_Track_Name.flac
      cover.jpg
    artist.jpg
```

This structure:

- Makes browsing files easy
- Matches common music library conventions
- Automatically handles filename sanitization
- Groups related content together

### Audio Quality Selection

The `TrackAudioQuality` enum provides different quality levels:

| Quality        | Typical Resolution | File Size | Use Case                  |
| -------------- | ------------------ | --------- | ------------------------- |
| Low            | Variable           | Small     | Mobile, bandwidth-limited |
| FlacLossless   | 16-bit/44.1kHz     | ~30MB     | CD-quality, general use   |
| FlacHiRes      | 24-bit/96kHz       | ~100MB    | High-fidelity listening   |
| FlacHighestRes | 24-bit/192kHz      | ~200MB    | Audiophile, archival      |

### Resume Support

Downloads automatically resume if interrupted:

1. The downloader checks if a partial file exists
2. If found, it requests remaining bytes using HTTP range headers
3. Download continues from where it left off
4. This works across application restarts (tasks are in the database)

### Automatic Tagging

Downloaded audio files are automatically tagged with metadata:

- **Title**: Track name
- **Track Number**: Position in album
- **Album**: Album name
- **Artist**: Artist name
- **Album Artist**: Album artist name
- **Date**: Release date (when available)

This ensures files are properly identified by media players.

## Testing the Example

While this example is conceptual, you can test the actual functionality by:

1. **Setting up a test database**:

```rust
use switchy_database::profiles::LibraryDatabase;

let db = LibraryDatabase::new(/* configure your database */).await?;
```

2. **Initializing music APIs**:

```rust
use moosicbox_music_api::MusicApis;

let music_apis = MusicApis::default();
// Add your API configurations
```

3. **Using real track IDs** from your music library or API source

4. **Running the download** with actual data

5. **Checking the output directory** for downloaded files

## Troubleshooting

### Common Issues

**"No database" error**:

- Ensure `LibraryDatabase` is properly initialized
- Verify database connection string is correct
- Check database permissions

**"No downloader" error**:

- Ensure `MoosicboxDownloader` is created and added to queue
- Verify music APIs are initialized

**Downloads fail to start**:

- Check network connectivity
- Verify API credentials are valid
- Ensure track IDs exist in the source API

**Files not found after download**:

- Check the download path exists and is writable
- Verify disk space is available
- Look for error events in progress listener

**Download speeds are slow**:

- Check network bandwidth
- Consider using a lower quality setting
- Verify the source API isn't rate-limiting

### Debug Logging

Enable debug logging to see detailed download information:

```bash
RUST_LOG=debug cargo run --manifest-path packages/downloader/examples/basic_usage/Cargo.toml
```

This will show:

- API requests and responses
- Download progress details
- File I/O operations
- Error messages and stack traces

## Related Examples

- **packages/files/examples/** - File handling and streaming examples
- **packages/database/examples/** - Database connection examples
- **packages/async/examples/** - Async patterns used in the downloader

For more information, see the [moosicbox_downloader README](../../README.md).
