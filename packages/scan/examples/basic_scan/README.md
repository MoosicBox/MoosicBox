# Basic Scan Example

A comprehensive example demonstrating how to scan a local music directory using the `moosicbox_scan` package.

## Summary

This example shows how to set up a music library scanner, configure scan paths, track progress, and execute a local filesystem scan to discover and index audio files.

## What This Example Demonstrates

- Setting up an in-memory SQLite database with MoosicBox schema
- Adding local filesystem paths to the scan configuration
- Creating a `Scanner` instance for local file scanning
- Registering progress event listeners to track scan status
- Running a complete scan operation with error handling
- Understanding the scan lifecycle and events

## Prerequisites

- Basic understanding of async Rust and tokio
- Familiarity with SQLite databases
- Understanding of the MoosicBox music library structure
- Audio files to scan (optional - example works with empty directories)

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/scan/examples/basic_scan/Cargo.toml
```

Or from the example directory:

```bash
cd packages/scan/examples/basic_scan
cargo run
```

## Expected Output

When you run the example, you should see output similar to:

```
MoosicBox Scan - Basic Example
===============================

1. Setting up in-memory database...
   Database initialized

2. Adding scan path: /tmp/moosicbox_scan_example
   Scan path added

3. Setting up progress listener...
   Progress listener registered

4. Creating scanner for local files...
   Scanner created

5. Starting scan...
   Scan started: 0 items to scan for Local { paths: ["/tmp/moosicbox_scan_example"] }
   Scan finished: 0 items processed for Local { paths: ["/tmp/moosicbox_scan_example"] }

✓ Scan completed successfully!

Note: This example scanned an empty directory. To scan real music files:
  1. Create a directory with audio files (.mp3, .flac, .m4a, .opus)
  2. Update the scan_path variable to point to that directory
  3. Re-run this example to see the scanner in action
```

If you populate a directory with music files and update the `scan_path` variable, you'll see progress updates:

```
   Scan started: 45 items to scan for Local { paths: ["/path/to/music"] }
   Progress: 10/45 items scanned
   Progress: 20/45 items scanned
   Progress: 30/45 items scanned
   Progress: 40/45 items scanned
   Progress: 45/45 items scanned
   Scan finished: 45 items processed for Local { paths: ["/path/to/music"] }
```

## Code Walkthrough

### 1. Database Setup

The example creates an in-memory SQLite database for testing:

```rust
let db = create_test_database().await?;
```

The `create_test_database()` function:

- Creates an in-memory SQLite database connection pool
- Runs schema migrations for Library, MusicSource, Scan, Menu, and Session schemas
- Returns a `LibraryDatabase` instance ready for use

### 2. Configuring Scan Paths

Before scanning, we need to tell the scanner where to look for music files:

```rust
let scan_path = std::env::temp_dir()
    .join("moosicbox_scan_example")
    .display()
    .to_string();

add_scan_path(&db, &scan_path).await?;
```

The `add_scan_path()` function:

- Adds the path to the database's scan configuration
- Prevents duplicate paths from being added
- Associates the path with the `ScanOrigin::Local` origin

### 3. Progress Tracking

To monitor scan progress, we register an event listener:

```rust
add_progress_listener(Box::new(|event| {
    Box::pin(async move {
        match event {
            ProgressEvent::ScanCountUpdated { total, task, .. } => {
                println!("Scan started: {total} items to scan for {task:?}");
            }
            ProgressEvent::ItemScanned { scanned, total, .. } => {
                println!("Progress: {scanned}/{total} items scanned");
            }
            ProgressEvent::ScanFinished { scanned, task, .. } => {
                println!("Scan finished: {scanned} items for {task:?}");
            }
            _ => {}
        }
    })
}))
.await;
```

Progress events include:

- **`ScanCountUpdated`**: Fired when the total number of items to scan is known
- **`ItemScanned`**: Fired each time an audio file is processed
- **`ScanFinished`**: Fired when the scan completes (successfully or not)

### 4. Creating the Scanner

The scanner is created from a scan origin, which looks up the configured paths:

```rust
let scanner = Scanner::from_origin(&db, ScanOrigin::Local).await?;
```

This:

- Queries the database for all paths associated with `ScanOrigin::Local`
- Creates a `Scanner` instance configured with those paths
- Returns an error if the database query fails

### 5. Running the Scan

Finally, we execute the scan:

```rust
let music_apis = MusicApis::default();
scanner.scan(music_apis, &db).await?;
```

During the scan:

- Each configured path is recursively walked
- Audio files are identified by extension (`.mp3`, `.flac`, `.m4a`, `.opus`)
- Metadata is extracted from each file (artist, album, title, duration, etc.)
- Cover art is searched for and cached
- Database entries are created/updated for artists, albums, and tracks
- Progress events are fired at each step

## Key Concepts

### Scanner Architecture

The `Scanner` struct manages the scanning process:

- **Task-based**: Each scanner is associated with a specific `ScanTask` (Local or API)
- **Stateful**: Tracks `scanned` and `total` counts during execution
- **Observable**: Notifies listeners of progress events
- **Concurrent**: Can spawn multiple scan tasks in parallel

### Scan Origins

A `ScanOrigin` identifies the source of music files:

- **`ScanOrigin::Local`**: Local filesystem paths
- **`ScanOrigin::Tidal`**: Tidal streaming service
- **`ScanOrigin::Qobuz`**: Qobuz streaming service
- And others...

Each origin can be enabled/disabled independently.

### Progress Events

The event system provides real-time feedback:

- Events are fired asynchronously
- Multiple listeners can be registered
- Events include context (task, counts) for detailed tracking
- Listeners receive immutable event references

### Database Integration

The scanner integrates with MoosicBox's database:

- **Schema**: Uses `Library`, `MusicSource`, and `Scan` schemas
- **Deduplication**: Prevents duplicate artists/albums/tracks
- **Relationships**: Links tracks to albums, albums to artists
- **Search Index**: Updates global search index after scanning

## Testing the Example

### With Real Music Files

1. Create a test directory with audio files:

    ```bash
    mkdir -p ~/test_music
    cp ~/Music/*.mp3 ~/test_music/
    ```

2. Update the `scan_path` variable in `main.rs`:

    ```rust
    let scan_path = "/home/user/test_music";
    ```

3. Run the example and observe the scan progress

### With Sample Files

You can create test audio files using tools like `ffmpeg`:

```bash
# Create a silent 30-second MP3
ffmpeg -f lavfi -i anullsrc=r=44100:cl=stereo -t 30 -q:a 9 test.mp3

# Create a silent 30-second FLAC
ffmpeg -f lavfi -i anullsrc=r=44100:cl=stereo -t 30 test.flac
```

## Troubleshooting

### "Database migration failed"

**Problem**: The schema migrations couldn't be applied.

**Solution**: Ensure all required MoosicBox schema packages are available. For in-memory databases, this shouldn't occur. For persistent databases, check file permissions.

### "Scan path already exists"

**Problem**: The path is already in the scan configuration.

**Solution**: This is not an error - `add_scan_path()` is idempotent and will skip duplicate paths.

### "Permission denied" during scan

**Problem**: The scanner doesn't have read access to the music directory.

**Solution**: Ensure the directory and all files have appropriate read permissions:

```bash
chmod -R a+r /path/to/music
```

### No files scanned

**Problem**: The scan completes but shows 0 items.

**Solution**: Check that:

- The directory exists and contains audio files
- Files have supported extensions (`.mp3`, `.flac`, `.m4a`, `.opus`)
- The path is correctly specified (absolute paths recommended)

## Related Examples

- [packages/audio_decoder/examples/basic_usage](../../../audio_decoder/examples/basic_usage/) - Audio file decoding
- [packages/files/examples/download_with_progress](../../../files/examples/download_with_progress/) - File downloading with progress
- [packages/database/examples/turso_basic](../../../database/examples/turso_basic/) - Database operations
