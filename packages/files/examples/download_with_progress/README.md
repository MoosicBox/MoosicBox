# Download with Progress Example

A comprehensive example demonstrating how to download files from remote URLs with real-time progress and speed monitoring using the `moosicbox_files` package.

## Summary

This example shows how to download a file from a remote URL while monitoring both the download progress (bytes downloaded) and download speed (bytes per second, KB/s, MB/s). It demonstrates the core functionality of the `moosicbox_files` package for handling remote file operations with progress tracking.

## What This Example Demonstrates

- Downloading files from remote URLs using `fetch_bytes_from_remote_url()`
- Saving byte streams to files with `save_bytes_stream_to_file_with_speed_listener()`
- Real-time download speed monitoring (bytes/second, KB/s, MB/s)
- Progress tracking with per-chunk and cumulative byte counts
- Proper async/await patterns with tokio runtime
- Error handling for network and file I/O operations
- Working with callback functions that return futures

## Prerequisites

- Rust 1.70 or later
- Basic understanding of async/await in Rust
- Internet connection for downloading test files
- Familiarity with tokio async runtime

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/files/examples/download_with_progress/Cargo.toml
```

Or from the example directory:

```bash
cd packages/files/examples/download_with_progress
cargo run
```

## Expected Output

When you run the example, you should see output similar to:

```
=== MoosicBox Files: Download with Progress Example ===

Downloading from: https://httpbin.org/bytes/1048576
Saving to: /tmp/moosicbox_download_example.bin

Progress: 16384 bytes in this chunk, 0.02 MB total downloaded
Progress: 16384 bytes in this chunk, 0.03 MB total downloaded
Download speed: 1.45 MB/s (1484.80 KB/s, 1520435 bytes/s)
Progress: 16384 bytes in this chunk, 0.05 MB total downloaded
...
Progress: 16384 bytes in this chunk, 1.00 MB total downloaded

✓ Download completed successfully!
File saved to: /tmp/moosicbox_download_example.bin
File size: 1.00 MB (1024.00 KB, 1048576 bytes)
```

The example downloads a 1 MB test file and displays:

- Progress updates showing bytes in each chunk and total bytes downloaded
- Speed measurements approximately once per second
- Final file size verification

## Code Walkthrough

### Setting Up the Download

```rust
let url = "https://httpbin.org/bytes/1048576"; // 1 MB test file
let output_path = Path::new("/tmp/moosicbox_download_example.bin");
let client = Client::new();
```

We use httpbin.org's `/bytes/N` endpoint which returns exactly N random bytes, making it perfect for testing downloads of known sizes.

### Fetching the Byte Stream

```rust
let stream = fetch_bytes_from_remote_url(&client, url, None).await?;
```

The `fetch_bytes_from_remote_url()` function returns a stream of bytes from the remote URL. The stream implements `Stream<Item = Result<Bytes, std::io::Error>>`, allowing efficient processing of large files without loading everything into memory.

### Speed Monitoring Callback

```rust
let speed_callback = Box::new(|speed_bps: f64| {
    let speed_kbps = speed_bps / 1024.0;
    let speed_mbps = speed_kbps / 1024.0;
    println!("Download speed: {:.2} MB/s ({:.2} KB/s, {:.0} bytes/s)",
             speed_mbps, speed_kbps, speed_bps);
    Box::pin(async {}) as Pin<Box<dyn Future<Output = ()> + Send>>
});
```

The speed callback is invoked approximately once per second with the current download speed in bytes per second. We convert to KB/s and MB/s for readability. The callback must return a pinned future, allowing for async operations within the callback if needed.

### Progress Monitoring Callback

```rust
let progress_callback = Some(Box::new(|bytes_in_chunk: usize, total_bytes: usize| {
    let mb_read = total_bytes as f64 / 1_048_576.0;
    println!("Progress: {} bytes in this chunk, {:.2} MB total downloaded",
             bytes_in_chunk, mb_read);
    Box::pin(async {}) as Pin<Box<dyn Future<Output = ()> + Send>>
}));
```

The progress callback is invoked after each chunk is written to the file, receiving:

- `bytes_in_chunk`: The number of bytes in the current chunk
- `total_bytes`: The cumulative total of all bytes downloaded so far

### Saving with Monitoring

```rust
save_bytes_stream_to_file_with_speed_listener(
    stream,
    output_path,
    None, // Start from beginning (no offset)
    speed_callback,
    progress_callback,
).await?;
```

The `save_bytes_stream_to_file_with_speed_listener()` function combines streaming, progress tracking, and speed monitoring in a single operation. It handles all the complexity of tracking download metrics while efficiently saving the file.

## Key Concepts

### Streaming Downloads

Rather than loading an entire file into memory, the library uses Rust's `Stream` trait to process data incrementally. This allows downloading files of any size with minimal memory usage.

### Progress Callbacks with Futures

The callback signature `Box<dyn (FnMut(...) -> Pin<Box<dyn Future<Output = ()> + Send>>) + Send>` allows callbacks to perform async operations, such as:

- Updating a database with progress
- Making async API calls
- Communicating with other async tasks

Even simple synchronous callbacks must return `Box::pin(async {})` to satisfy the type signature.

### Speed Calculation

The library internally tracks:

- Bytes downloaded since last speed update
- Time elapsed using high-resolution timers
- Speed = bytes / time, computed approximately once per second

### Error Handling

The example uses `?` operator for error propagation, returning `Result<(), Box<dyn std::error::Error>>`. The library provides specific error types:

- `FetchAndSaveBytesFromRemoteUrlError` for network/HTTP errors
- `SaveBytesStreamToFileError` for file I/O errors

### File System Operations

The library automatically:

- Creates parent directories if they don't exist
- Handles file truncation (when starting from offset 0)
- Uses buffered I/O for efficiency
- Flushes data to ensure it's written to disk

## Testing the Example

1. **Verify the download completed**:

    ```bash
    ls -lh /tmp/moosicbox_download_example.bin
    ```

    Should show a file of exactly 1,048,576 bytes (1 MB).

2. **Check file contents**:

    ```bash
    hexdump -C /tmp/moosicbox_download_example.bin | head
    ```

    Should show random binary data.

3. **Test with different file sizes**:
   Modify the URL in `main.rs`:

    ```rust
    let url = "https://httpbin.org/bytes/10485760"; // 10 MB
    ```

4. **Test with a real file**:
   Replace with any publicly accessible file URL:

    ```rust
    let url = "https://example.com/sample.mp3";
    let output_path = Path::new("/tmp/sample.mp3");
    ```

5. **Enable detailed logging**:
    ```bash
    RUST_LOG=debug cargo run --manifest-path packages/files/examples/download_with_progress/Cargo.toml
    ```

## Troubleshooting

### Network Errors

If you see "HTTP request failed" errors:

- Check your internet connection
- Verify the URL is accessible (try opening in a browser)
- Check if you need to configure a proxy
- Try a different test URL

### Permission Errors

If you see "Permission denied" when saving:

- Ensure the output directory exists and is writable
- Try a different output path (e.g., your home directory)
- Check filesystem permissions

### Compile Errors

If the example fails to compile:

- Ensure you're using a recent Rust version: `rustc --version`
- Update dependencies: `cargo update`
- Check that the workspace is properly configured

### No Progress Output

If you don't see progress updates:

- Initialize logging: The example calls `env_logger::init()`
- The default log level is `info`, set `RUST_LOG=debug` for more output
- Very fast downloads may complete before speed updates are generated

## Related Examples

This is currently the only example for `moosicbox_files`. For related functionality, see:

- The inline examples in the package documentation
- The README.md in `packages/files/` for additional code snippets
