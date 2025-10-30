#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![allow(clippy::too_many_lines)]

//! Basic usage example for `LibraryMusicApi`
//!
//! This example demonstrates:
//! - Creating a `LibraryMusicApi` instance
//! - Using the `MusicApi` trait methods
//! - Querying artists, albums, and tracks
//! - Search functionality
//! - Managing favorites

use std::sync::Arc;

use moosicbox_library_music_api::LibraryMusicApi;
use moosicbox_music_api::{
    MusicApi,
    models::{AlbumFilters, AlbumsRequest},
};
use moosicbox_music_models::id::Id;
use moosicbox_paging::PagingRequest;
use switchy_database::{Database, profiles::LibraryDatabase, turso::TursoDatabase};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init_from_env(env_logger::Env::default().default_filter_or("info"));

    println!("LibraryMusicApi Basic Usage Example");
    println!("====================================\n");

    // Step 1: Create an in-memory database for demonstration
    println!("Step 1: Setting up in-memory library database...");
    let db = TursoDatabase::new(":memory:").await?;
    let library_db = LibraryDatabase::from(Arc::new(Box::new(db) as Box<dyn Database>));
    println!("✓ Database created\n");

    // Step 2: Initialize the LibraryMusicApi
    println!("Step 2: Creating LibraryMusicApi instance...");
    let api = LibraryMusicApi::new(library_db);
    println!("✓ API instance created");
    println!("  API Source: {:?}\n", api.source());

    // Step 3: Query favorite artists with pagination
    println!("Step 3: Fetching favorite artists...");
    let artists_result = api
        .artists(
            Some(0),  // offset
            Some(10), // limit
            None,     // order
            None,     // order_direction
        )
        .await?;

    println!("✓ Artists retrieved");
    println!("  Total artists: {}", artists_result.total().unwrap_or(0));
    println!("  Artists in this page: {}", artists_result.items().len());

    // Display first few artists if any exist
    for (i, artist) in artists_result.items().iter().take(3).enumerate() {
        let num = i + 1;
        println!("  {num}. {} (ID: {})", artist.title, artist.id);
    }
    println!();

    // Step 4: Query favorite albums
    println!("Step 4: Fetching favorite albums...");
    let albums_request = AlbumsRequest {
        sources: None,
        sort: None,
        filters: Some(AlbumFilters {
            name: None,
            artist: None,
            search: None,
            album_type: None,
            artist_id: None,
            artist_api_id: None,
        }),
        page: Some(PagingRequest {
            offset: 0,
            limit: 10,
        }),
    };

    let albums_result = api.albums(&albums_request).await?;

    println!("✓ Albums retrieved");
    println!("  Total albums: {}", albums_result.total().unwrap_or(0));
    println!("  Albums in this page: {}", albums_result.items().len());

    // Display first few albums if any exist
    for (i, album) in albums_result.items().iter().take(3).enumerate() {
        let num = i + 1;
        let artist = &album.artist;
        println!("  {num}. {} (ID: {})", album.title, album.id);
        println!("     Artist: {artist}");
    }
    println!();

    // Step 5: Demonstrate search functionality
    if api.supports_search() {
        println!("Step 5: Testing search functionality...");
        let search_query = "example";

        match api.search(search_query, Some(0), Some(5)).await {
            Ok(search_results) => {
                println!("✓ Search completed for query: '{search_query}'");
                let count = search_results.results.len();
                let position = search_results.position;
                println!("  Total results found: {count}");
                println!("  Position: {position}");
            }
            Err(e) => {
                println!("  Search error: {e}");
            }
        }
        println!();
    }

    // Step 6: Check scan support
    println!("Step 6: Checking library scan support...");
    if api.supports_scan() {
        println!("✓ Library scanning is supported");

        match api.scan_enabled().await {
            Ok(enabled) => {
                println!("  Scan enabled: {enabled}");
            }
            Err(e) => {
                println!("  Could not check scan status: {e}");
            }
        }
    } else {
        println!("  Library scanning not supported");
    }
    println!();

    // Step 7: Demonstrate adding/removing favorites
    println!("Step 7: Managing favorites...");

    // For demonstration, we'll use a sample ID
    let sample_artist_id = Id::Number(999_999);

    println!("  Attempting to add artist to favorites (ID: {sample_artist_id})...");
    match api.add_artist(&sample_artist_id).await {
        Ok(()) => println!("  ✓ Artist added to favorites"),
        Err(e) => println!("  Note: {e}"),
    }

    println!("  Attempting to remove artist from favorites...");
    match api.remove_artist(&sample_artist_id).await {
        Ok(()) => println!("  ✓ Artist removed from favorites"),
        Err(e) => println!("  Note: {e}"),
    }
    println!();

    // Summary
    println!("Example completed successfully!");
    println!("\nKey takeaways:");
    println!("- LibraryMusicApi implements the MusicApi trait");
    println!("- Provides access to local library content (artists, albums, tracks)");
    println!("- Supports pagination for efficient data retrieval");
    println!("- Includes search functionality across library content");
    println!("- Manages favorites (add/remove artists, albums, tracks)");
    println!("- Integrates with library scanning capabilities");

    Ok(())
}
