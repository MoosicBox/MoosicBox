#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::similar_names)] // Allow similar variable names in unit conversions

//! Example demonstrating file download with progress and speed monitoring.
//!
//! This example shows how to:
//! - Download a file from a remote URL
//! - Monitor download progress (bytes downloaded)
//! - Track download speed in real-time (bytes per second, KB/s, MB/s)
//! - Handle errors gracefully

use moosicbox_files::{fetch_bytes_from_remote_url, save_bytes_stream_to_file_with_speed_listener};
use std::{future::Future, path::Path, pin::Pin};
use switchy_http::Client;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see progress updates
    env_logger::init();

    println!("=== MoosicBox Files: Download with Progress Example ===\n");

    // Configure the download
    // Using a test file from httpbin.org (a reliable test service)
    let url = "https://httpbin.org/bytes/1048576"; // 1 MB test file
    let output_path = Path::new("/tmp/moosicbox_download_example.bin");

    println!("Downloading from: {url}");
    println!("Saving to: {}\n", output_path.display());

    // Create HTTP client
    let client = Client::new();

    // Fetch the byte stream from the remote URL
    let stream = fetch_bytes_from_remote_url(&client, url, None).await?;

    // Define speed monitoring callback
    // This is called approximately once per second with the current download speed
    let speed_callback = Box::new(|speed_bps: f64| {
        let speed_kb_per_sec = speed_bps / 1024.0;
        let speed_mb_per_sec = speed_kb_per_sec / 1024.0;

        println!(
            "Download speed: {speed_mb_per_sec:.2} MB/s ({speed_kb_per_sec:.2} KB/s, {speed_bps:.0} bytes/s)"
        );

        // Return a future (required by the API)
        Box::pin(async {}) as Pin<Box<dyn Future<Output = ()> + Send>>
    });

    // Define progress monitoring callback
    // This is called after each chunk is written to the file
    #[allow(clippy::type_complexity, clippy::cast_precision_loss)]
    let progress_callback: Option<
        Box<dyn (FnMut(usize, usize) -> Pin<Box<dyn Future<Output = ()> + Send>>) + Send>,
    > = Some(Box::new(|bytes_in_chunk: usize, total_bytes: usize| {
        let mb_read = total_bytes as f64 / 1_048_576.0;
        println!(
            "Progress: {bytes_in_chunk} bytes in this chunk, {mb_read:.2} MB total downloaded"
        );

        // Return a future (required by the API)
        Box::pin(async {}) as Pin<Box<dyn Future<Output = ()> + Send>>
    }));

    // Save the stream to file with both speed and progress monitoring
    save_bytes_stream_to_file_with_speed_listener(
        stream,
        output_path,
        None, // Start from beginning (no offset)
        speed_callback,
        progress_callback,
    )
    .await?;

    println!("\n✓ Download completed successfully!");
    println!("File saved to: {}", output_path.display());

    // Verify the file was created and show its size
    #[allow(clippy::cast_precision_loss)]
    if let Ok(metadata) = std::fs::metadata(output_path) {
        let size_bytes = metadata.len();
        let size_kb = size_bytes as f64 / 1024.0;
        let size_mb = size_kb / 1024.0;
        println!("File size: {size_mb:.2} MB ({size_kb:.2} KB, {size_bytes} bytes)");
    }

    Ok(())
}
