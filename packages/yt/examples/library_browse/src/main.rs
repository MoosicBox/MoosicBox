#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! `YouTube` Music library browsing example.
//!
//! This example demonstrates how to:
//! - Set up the `YouTube` Music API client
//! - Use the API methods for browsing content
//! - Handle the `YouTube` Music data structures
//! - Work with the `MusicApi` trait implementation

use moosicbox_yt::{API_SOURCE, YtMusicApi};
use std::sync::Arc;
use switchy_database::profiles::LibraryDatabase;
use switchy_database::simulator::SimulationDatabase;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== YouTube Music Library Browse Example ===\n");

    // Step 1: Initialize the database connection
    // Using SimulationDatabase for this example since the YouTube Music API is stubbed
    println!("Initializing database connection...");
    let sim_db = SimulationDatabase::new_for_path(None)?;
    let db = LibraryDatabase::from(Arc::new(
        Box::new(sim_db) as Box<dyn switchy_database::Database>
    ));
    println!("Database connection established.\n");

    // Step 2: Create the YouTube Music API client
    println!("Creating YouTube Music API client...");
    let _yt_api = YtMusicApi::builder().with_db(db.clone()).build().await?;
    println!("YouTube Music API client created successfully.\n");

    // Step 3: Demonstrate API structure
    println!("=== YouTube Music API Structure ===\n");

    println!("The YtMusicApi provides the following capabilities:");
    println!("  - Browse favorite artists, albums, and tracks");
    println!("  - Search across YouTube Music's catalog");
    println!("  - Manage favorites (add/remove)");
    println!("  - Get streaming URLs and playback information");
    println!("  - OAuth2 device flow authentication");
    println!("  - Implements the MusicApi trait for compatibility\n");

    println!("API Source: {}", *API_SOURCE);
    println!("API Client Type: YtMusicApi");
    println!("Database Backend: SimulationDatabase (in-memory)\n");

    // Step 4: Show available features
    println!("=== Available Features ===\n");

    println!("1. Artist Operations:");
    println!("   - favorite_artists() - Browse user's favorite artists");
    println!("   - artist() - Get a specific artist by ID");
    println!("   - artist_albums() - Get albums for an artist");
    println!("   - add_favorite_artist() / remove_favorite_artist()");

    println!("\n2. Album Operations:");
    println!("   - favorite_albums() - Browse user's favorite albums");
    println!("   - album() - Get a specific album by ID");
    println!("   - album_tracks() - Get tracks for an album");
    println!("   - add_favorite_album() / remove_favorite_album()");

    println!("\n3. Track Operations:");
    println!("   - favorite_tracks() - Browse user's favorite tracks");
    println!("   - track() - Get a specific track by ID");
    println!("   - track_file_url() - Get streaming URLs");
    println!("   - track_playback_info() - Get detailed playback metadata");
    println!("   - add_favorite_track() / remove_favorite_track()");

    println!("\n4. Search Operations:");
    println!("   - search() - Search across artists, albums, and tracks");
    println!("   - Returns YtSearchResults with formatted results");

    println!("\n5. Authentication:");
    println!("   - device_authorization() - Start OAuth2 device flow");
    println!("   - device_authorization_token() - Complete OAuth flow");

    // Step 5: Explain data structures
    println!("\n=== Data Structures ===\n");

    println!("YtArtist:");
    println!("  - id: String - YouTube Music artist ID");
    println!("  - name: String - Artist name");
    println!("  - picture: Option<String> - Profile picture URL");
    println!("  - popularity: u32 - Popularity score");

    println!("\nYtAlbum:");
    println!("  - id: String - YouTube Music album ID");
    println!("  - title: String - Album title");
    println!("  - artist: String - Artist name");
    println!("  - artist_id: String - Artist ID");
    println!("  - album_type: YtAlbumType - LP, EpsAndSingles, or Compilations");
    println!("  - cover: Option<String> - Cover artwork URL");
    println!("  - duration: u32 - Total duration in seconds");
    println!("  - release_date: Option<String> - ISO 8601 date");

    println!("\nYtTrack:");
    println!("  - id: String - YouTube Music track ID");
    println!("  - title: String - Track title");
    println!("  - artist: String - Artist name");
    println!("  - album: String - Album title");
    println!("  - track_number: u32 - Track number");
    println!("  - duration: u32 - Duration in seconds");
    println!("  - audio_quality: String - Quality level");

    println!("\n=== Example Complete ===\n");
    println!("This example demonstrated:");
    println!("- Setting up the YouTube Music API client with database");
    println!("- Understanding the available API methods");
    println!("- Learning about YouTube Music data structures");
    println!("- Integration with the MusicApi trait");
    println!("\nNote: The YouTube Music API endpoints are currently stubbed.");
    println!("Actual implementation requires connecting to YouTube Music services.");

    Ok(())
}
