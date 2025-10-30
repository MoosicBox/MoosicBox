#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `moosicbox_downloader`.
//!
//! This example demonstrates the core functionality of the downloader package,
//! including:
//! - Creating download tasks programmatically
//! - Setting up a download queue with progress tracking
//! - Monitoring download progress in real-time
//! - Understanding the download workflow
//!
//! Note: This is a conceptual example showing the API structure. A complete
//! working example would require:
//! - A configured database connection (`LibraryDatabase`)
//! - Initialized music API instances (`MusicApis`)
//! - Valid track/album IDs from a music source
//! - Network access to download sources

use std::path::PathBuf;

use moosicbox_downloader::{DownloadApiSource, TrackAudioQuality};
use moosicbox_music_models::ApiSource;

/// Demonstrates the basic workflow for downloading music tracks.
///
/// This function shows how to:
/// 1. Set up the download directory and parameters
/// 2. Create download tasks for specific tracks
/// 3. Configure a download queue with progress tracking
/// 4. Process downloads with real-time progress monitoring
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("MoosicBox Downloader - Basic Usage Example");
    println!("===========================================\n");

    // STEP 1: Configure download parameters
    println!("Step 1: Configuring download parameters...");

    // Specify where downloaded files should be saved
    let download_path = PathBuf::from("./downloads");
    println!("  Download directory: {}", download_path.display());

    // Choose the audio quality for downloads
    // Options: Low, FlacLossless, FlacHiRes, FlacHighestRes
    let quality = TrackAudioQuality::FlacHighestRes;
    println!("  Audio quality: {quality:?}");

    // Specify the source API to download from
    // This example uses a generic API source
    let api_source = ApiSource::library();
    let source = DownloadApiSource::Api(api_source);
    println!("  Download source: {source:?}\n");

    // STEP 2: Set up database and music APIs
    println!("Step 2: Setting up database and music APIs...");
    println!("  (In a real application, initialize LibraryDatabase and MusicApis here)");

    // In a real application, you would initialize these with actual connections:
    //
    // let db = LibraryDatabase::new(/* database configuration */).await?;
    // let music_apis = MusicApis::new()
    //     .with_tidal(/* Tidal API config */)
    //     .with_qobuz(/* Qobuz API config */);
    //
    // For this example, we'll demonstrate the API structure conceptually.

    println!("  Note: This example requires a configured database and music APIs\n");

    // STEP 3: Create download tasks
    println!("Step 3: Creating download tasks...");

    // In a real application, you would get track IDs from your music library:
    //
    // let track_ids = vec![Id::Number(123), Id::Number(456)];
    //
    // Then create download tasks:
    //
    // let tasks = get_create_download_tasks(
    //     &*music_api,
    //     &download_path,
    //     None,                    // Single track ID
    //     Some(track_ids),         // Multiple track IDs
    //     None,                    // Single album ID
    //     None,                    // Multiple album IDs
    //     true,                    // Download album covers
    //     true,                    // Download artist covers
    //     Some(quality),
    //     Some(source.clone()),
    // ).await?;
    //
    // let tasks = create_download_tasks(&db, tasks).await?;

    println!("  Example track IDs: 123, 456, 789");
    println!("  Download album covers: true");
    println!("  Download artist covers: true\n");

    // STEP 4: Set up the download queue with progress tracking
    println!("Step 4: Setting up download queue with progress listener...");

    // Create a progress listener to monitor download progress
    // In a real application, this closure would be passed to the queue:
    //
    // let progress_listener = Box::new(|event: &ProgressEvent| {
    //     Box::pin(async move {
    //         match event {
    //             // Called when the total download size is known
    //             ProgressEvent::Size { task, bytes } => {
    //                 println!("  [Task {}] Size: {:?} bytes", task.id, bytes);
    //             }
    //             // Called with periodic speed updates
    //             ProgressEvent::Speed { task, bytes_per_second } => {
    //                 let mb_per_second = bytes_per_second / 1_000_000.0;
    //                 println!("  [Task {}] Speed: {:.2} MB/s", task.id, mb_per_second);
    //             }
    //             // Called as bytes are downloaded
    //             ProgressEvent::BytesRead { task, read, total } => {
    //                 let progress = ((*read as f64) / (*total as f64)) * 100.0;
    //                 println!("  [Task {}] Progress: {:.1}% ({}/{})",
    //                          task.id, progress, read, total);
    //             }
    //             // Called when task state changes
    //             ProgressEvent::State { task, state } => {
    //                 println!("  [Task {}] State changed to: {:?}", task.id, state);
    //             }
    //         }
    //     }) as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
    // }) as Box<dyn Fn(&ProgressEvent) -> _ + Send + Sync>;

    // In a real application, create and configure the download queue:
    //
    // let downloader = MoosicboxDownloader::new(db.clone(), music_apis.clone());
    //
    // let mut queue = DownloadQueue::new()
    //     .with_database(db)
    //     .with_downloader(Box::new(downloader))
    //     .add_progress_listener(progress_listener);
    //
    // // Add tasks to the queue
    // queue.add_tasks_to_queue(tasks).await;
    //
    // // Start processing the queue
    // queue.process();
    //
    // // Monitor download speed
    // if let Some(speed) = queue.speed() {
    //     println!("Current download speed: {:.2} MB/s", speed / 1_000_000.0);
    // }

    println!("  Progress listener configured");
    println!("  Queue will process tasks sequentially\n");

    // STEP 5: Explain the download workflow
    println!("Step 5: Understanding the download workflow...");
    println!("  1. Tasks are queued in the order they are added");
    println!("  2. The queue processes one task at a time");
    println!("  3. Each download includes:");
    println!("     - Fetching track metadata from the music API");
    println!("     - Downloading audio data with resume support");
    println!("     - Automatic ID3 tag writing (artist, album, title, etc.)");
    println!("     - Downloading cover art (if requested)");
    println!("     - Organizing files by artist/album/track");
    println!("  4. Progress events are fired throughout the download");
    println!("  5. Downloads can resume if interrupted\n");

    // STEP 6: Advanced features
    println!("Step 6: Advanced features...");
    println!("  - Resume Support: Downloads automatically resume from where they stopped");
    println!("  - Timeout Handling: Automatic retry with configurable timeouts");
    println!("  - Concurrent Operations: Multiple listeners can track progress");
    println!("  - Database Persistence: Tasks are stored and can survive restarts");
    println!("  - Automatic Scanning: Completed downloads are added to the library\n");

    // STEP 7: File organization
    println!("Step 7: File organization...");
    println!("  Downloaded files are organized as:");
    println!("  <download_path>/");
    println!("    <artist_name>/");
    println!("      <album_name>/");
    println!("        01_track_name.flac");
    println!("        02_track_name.flac");
    println!("        cover.jpg");
    println!("      artist.jpg\n");

    // STEP 8: API quality selection
    println!("Step 8: Audio quality options...");
    println!("  - TrackAudioQuality::Low          - Lower bitrate for faster downloads");
    println!("  - TrackAudioQuality::FlacLossless - CD-quality FLAC (16-bit/44.1kHz)");
    println!("  - TrackAudioQuality::FlacHiRes    - High-resolution FLAC (24-bit/96kHz)");
    println!("  - TrackAudioQuality::FlacHighestRes - Highest available quality (24-bit/192kHz)\n");

    println!("Example complete!");
    println!("\nTo use this in your application:");
    println!("1. Set up a LibraryDatabase connection");
    println!("2. Initialize MusicApis with your API credentials");
    println!("3. Get valid track/album IDs from your music source");
    println!("4. Call the functions as shown above with real data");
    println!("\nSee the README.md for more details and working code snippets.");

    Ok(())
}
