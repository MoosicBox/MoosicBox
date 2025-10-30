#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic example demonstrating music API scanning operations.
//!
//! This example shows the conceptual usage of the music API helpers to:
//! - Enable scanning for a music source
//! - Check if scanning is enabled
//! - Trigger a scan operation
//!
//! Note: This is a simplified conceptual demonstration. In a real application,
//! you would use actual `MusicApi` implementations and a configured database.

//! Imports you would use in a real application:
//! ```ignore
//! use moosicbox_music_api_helpers::scan::{enable_scan, scan_enabled, scan};
//! use moosicbox_music_api::MusicApi;
//! use switchy::database::profiles::LibraryDatabase;
//! ```

/// Demonstrates how the music API helpers would be used in a real application.
///
/// This is a conceptual example showing the API patterns. In production:
/// 1. You would have a real `MusicApi` implementation (e.g., Spotify, Tidal, etc.)
/// 2. You would connect to a persistent database
/// 3. You would have proper profile and authentication setup
fn demonstrate_api_usage() {
    println!("=== MoosicBox Music API Helpers - Basic Scan Operations Example ===\n");
    println!("This example demonstrates the conceptual usage of the scan helpers.\n");

    println!("Step 1: Create a database connection");
    println!("  let db = LibraryDatabase::new_sqlite(\"path/to/db.sqlite\")?;");
    println!("  Creates a connection to the music library database\n");

    println!("Step 2: Get or create your music API implementation");
    println!("  let music_api: &dyn MusicApi = get_music_api_implementation();");
    println!("  This would be a real implementation like SpotifyMusicApi, etc.\n");

    println!("Step 3: Enable scanning for the music source");
    println!("  enable_scan(music_api, &db).await?;");
    println!("  Marks this music source as enabled for library scanning\n");

    println!("Step 4: Check if scanning is enabled");
    println!("  let enabled = scan_enabled(music_api, &db).await?;");
    println!("  Returns true if scanning is enabled for this source\n");

    println!("Step 5: Perform a scan operation (if enabled)");
    println!("  if enabled {{");
    println!("      scan(music_api, &db).await?;");
    println!("  }}");
    println!("  Fetches artists, albums, and tracks from the source\n");

    println!("=== Real-World Usage Pattern ===\n");
    println!("In a complete application:");
    println!("1. Initialize your database connection");
    println!("2. Set up music API implementations (Spotify, Tidal, Library, etc.)");
    println!("3. Register APIs with the profile system");
    println!("4. Handle authentication for services that require it");
    println!("5. Use the helpers to manage scanning operations\n");

    println!("Example code structure:");
    println!();
    println!("```rust");
    println!("use moosicbox_music_api_helpers::scan::{{enable_scan, scan_enabled, scan}};");
    println!("use moosicbox_music_api::MusicApi;");
    println!("use switchy::database::profiles::LibraryDatabase;");
    println!();
    println!("async fn manage_scanning(");
    println!("    music_api: &dyn MusicApi,");
    println!("    db: &LibraryDatabase,");
    println!(") -> Result<(), moosicbox_music_api::Error> {{");
    println!("    // Enable scanning for this source");
    println!("    enable_scan(music_api, db).await?;");
    println!();
    println!("    // Verify it's enabled");
    println!("    let enabled = scan_enabled(music_api, db).await?;");
    println!("    println!(\"Scanning enabled: {{}}\", enabled);");
    println!();
    println!("    // Perform the scan if enabled");
    println!("    if enabled {{");
    println!("        scan(music_api, db).await?;");
    println!("        println!(\"Scan completed successfully\");");
    println!("    }}");
    println!();
    println!("    Ok(())");
    println!("}}");
    println!("```\n");
}

/// Demonstrates database setup conceptually.
fn demonstrate_database_setup() {
    println!("=== Database Setup ===\n");

    println!("In a real application, you would create a database connection:");
    println!("  let db = LibraryDatabase::new(...)?;");
    println!();
    println!("Configuration steps:");
    println!("1. Choose your database backend (SQLite, PostgreSQL, etc.)");
    println!("2. Set up connection parameters (path, host, credentials)");
    println!("3. Run database migrations to create the schema");
    println!("4. Configure connection pooling and timeouts");
    println!("5. Set up proper error handling and recovery\n");
}

fn main() {
    // Show conceptual API usage
    demonstrate_api_usage();

    println!();
    println!("==========================================================\n");
    println!();

    // Demonstrate database setup
    demonstrate_database_setup();

    println!("=== Summary ===\n");
    println!("The moosicbox_music_api_helpers package provides three main functions:");
    println!();
    println!("1. enable_scan(music_api, db) - Enable scanning for a music source");
    println!("2. scan_enabled(music_api, db) - Check if scanning is enabled");
    println!("3. scan(music_api, db) - Perform a library scan");
    println!();
    println!("These helpers simplify music library management by providing");
    println!("high-level operations that integrate with the MoosicBox");
    println!("database and music API system.");
}
