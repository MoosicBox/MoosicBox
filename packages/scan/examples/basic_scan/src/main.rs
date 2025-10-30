#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic example demonstrating how to scan a local music directory.
//!
//! This example shows how to:
//! - Set up an in-memory database for testing
//! - Add a scan path to the configuration
//! - Create a scanner for local files
//! - Track scan progress with event listeners
//! - Run a scan and handle results

use moosicbox_music_api::MusicApis;
use moosicbox_scan::event::{ProgressEvent, add_progress_listener};
use moosicbox_scan::{ScanOrigin, Scanner, add_scan_path};
use switchy_database::profiles::LibraryDatabase;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see scan progress
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("MoosicBox Scan - Basic Example");
    println!("===============================\n");

    // Step 1: Create an in-memory database for testing
    println!("1. Setting up in-memory database...");
    let db = create_test_database().await?;
    println!("   Database initialized\n");

    // Step 2: Configure a scan path
    // In a real application, this would be a path to your music directory
    // For this example, we'll use a temp directory to demonstrate the setup
    let scan_path = std::env::temp_dir()
        .join("moosicbox_scan_example")
        .display()
        .to_string();

    println!("2. Adding scan path: {scan_path}");
    add_scan_path(&db, &scan_path).await?;
    println!("   Scan path added\n");

    // Step 3: Set up progress tracking
    println!("3. Setting up progress listener...");
    add_progress_listener(Box::new(|event| {
        let event = event.clone();
        Box::pin(async move {
            match event {
                ProgressEvent::ScanCountUpdated { total, task, .. } => {
                    println!("   Scan started: {total} items to scan for {task:?}");
                }
                ProgressEvent::ItemScanned { scanned, total, .. } => {
                    if scanned % 10 == 0 || scanned == total {
                        println!("   Progress: {scanned}/{total} items scanned");
                    }
                }
                ProgressEvent::ScanFinished { scanned, task, .. } => {
                    println!("   Scan finished: {scanned} items processed for {task:?}");
                }
                ProgressEvent::State { .. } => {}
            }
        })
    }))
    .await;
    println!("   Progress listener registered\n");

    // Step 4: Create a scanner for the local origin
    println!("4. Creating scanner for local files...");
    let scanner = Scanner::from_origin(&db, ScanOrigin::Local).await?;
    println!("   Scanner created\n");

    // Step 5: Create an empty MusicApis instance (not needed for local scanning)
    let music_apis = MusicApis::default();

    // Step 6: Run the scan
    println!("5. Starting scan...");
    match scanner.scan(music_apis, &db).await {
        Ok(()) => {
            println!("\n✓ Scan completed successfully!");
            println!("\nNote: This example scanned an empty directory. To scan real music files:");
            println!("  1. Create a directory with audio files (.mp3, .flac, .m4a, .opus)");
            println!("  2. Update the scan_path variable to point to that directory");
            println!("  3. Re-run this example to see the scanner in action");
        }
        Err(e) => {
            eprintln!("\n✗ Scan failed: {e}");
            return Err(e.into());
        }
    }

    Ok(())
}

/// Creates an in-memory `SQLite` database for testing.
///
/// In a production application, you would typically use a persistent database
/// configured through the `MoosicBox` server setup.
async fn create_test_database() -> Result<LibraryDatabase, Box<dyn std::error::Error>> {
    // Create an in-memory SQLite database
    let db = switchy_database_connection::init_sqlite_sqlx(None).await?;

    // Run schema migrations to set up the database structure
    moosicbox_schema::migrate_config(&*db).await?;
    moosicbox_schema::migrate_library(&*db).await?;

    // Wrap in LibraryDatabase
    Ok(LibraryDatabase::from(std::sync::Arc::new(db)))
}
