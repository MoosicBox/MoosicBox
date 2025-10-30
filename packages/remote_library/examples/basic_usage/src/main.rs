#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for the `MoosicBox` Remote Library client.
//!
//! This example demonstrates how to:
//! - Create a remote library API client
//! - Fetch artists with pagination
//! - Get specific artist details
//! - Search the remote library
//! - Handle errors properly

use moosicbox_music_api::MusicApi;
use moosicbox_music_models::{ApiSource, id::Id};
use moosicbox_remote_library::RemoteLibraryMusicApi;

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("MoosicBox Remote Library - Basic Usage Example\n");

    // Step 1: Create a remote library client
    // ============================================
    // Replace with your actual MoosicBox server URL
    let server_url = std::env::var("MOOSICBOX_SERVER_URL")
        .unwrap_or_else(|_| "http://localhost:8000".to_string());

    // Use the default profile (you can customize this)
    let profile = std::env::var("MOOSICBOX_PROFILE").unwrap_or_else(|_| "default".to_string());

    println!("Connecting to MoosicBox server at: {server_url}");
    println!("Using profile: {profile}\n");

    let api = RemoteLibraryMusicApi::new(server_url, ApiSource::library(), profile);

    // Step 2: Fetch artists with pagination
    // ============================================
    println!("Fetching first 10 artists...");
    match api.artists(Some(0), Some(10), None, None).await {
        Ok(artists_page) => {
            let items = artists_page.items();
            println!("Found {} artists (showing 10)\n", items.len());

            for (i, artist) in items.iter().enumerate() {
                println!("  {}. {} (ID: {})", i + 1, artist.title, artist.id);
            }
            println!();

            // Step 3: Get details for the first artist (if any exist)
            // ============================================
            if let Some(first_artist) = items.first() {
                let artist_id = &first_artist.id;
                println!("Getting details for artist: {}", first_artist.title);

                match api.artist(artist_id).await {
                    Ok(Some(artist_detail)) => {
                        println!("  Title: {}", artist_detail.title);
                        println!("  ID: {}", artist_detail.id);
                        println!("  Source: {}", artist_detail.api_source);

                        // Get albums for this artist
                        println!("\n  Fetching albums for this artist...");
                        match api
                            .artist_albums(artist_id, None, Some(0), Some(5), None, None)
                            .await
                        {
                            Ok(albums_page) => {
                                let albums = albums_page.items();
                                println!("  Found {} albums (showing up to 5):", albums.len());
                                for album in albums {
                                    println!(
                                        "    - {} ({})",
                                        album.title,
                                        album.date_released.unwrap_or_default()
                                    );
                                }
                            }
                            Err(e) => println!("  Error fetching albums: {e}"),
                        }
                    }
                    Ok(None) => println!("  Artist not found (this shouldn't happen!)"),
                    Err(e) => println!("  Error fetching artist details: {e}"),
                }
                println!();
            }
        }
        Err(e) => {
            println!("Error fetching artists: {e}");
            println!("\nNote: Make sure your MoosicBox server is running and accessible.");
            println!(
                "You can set MOOSICBOX_SERVER_URL environment variable to configure the server URL."
            );
        }
    }

    // Step 4: Demonstrate search functionality
    // ============================================
    println!("\nSearching for 'rock'...");
    match api.search("rock", Some(0), Some(5)).await {
        Ok(search_results) => {
            println!(
                "Found {} total results (showing up to 5)\n",
                search_results.results.len()
            );

            for result in &search_results.results {
                match result {
                    moosicbox_music_api::models::search::api::ApiGlobalSearchResult::Artist(
                        artist,
                    ) => {
                        println!("  [Artist] {}", artist.title);
                    }
                    moosicbox_music_api::models::search::api::ApiGlobalSearchResult::Album(
                        album,
                    ) => {
                        println!("  [Album]  {}", album.title);
                    }
                    moosicbox_music_api::models::search::api::ApiGlobalSearchResult::Track(
                        track,
                    ) => {
                        println!("  [Track]  {} - {}", track.artist, track.title);
                    }
                }
            }
        }
        Err(e) => println!("Error searching: {e}"),
    }

    // Step 5: Demonstrate getting a specific album
    // ============================================
    println!("\n\nDemonstrating album operations...");

    // Note: In a real application, you would get this ID from the artists/albums
    // queries above. For demonstration, we'll try a sample ID.
    let sample_album_id = Id::Number(1);

    match api.album(&sample_album_id).await {
        Ok(Some(album)) => {
            println!("Album found:");
            println!("  Title: {}", album.title);
            println!("  Artist: {}", album.artist);
            println!("  Released: {}", album.date_released.unwrap_or_default());

            // Get tracks from this album
            println!("\n  Fetching tracks...");
            match api
                .album_tracks(&sample_album_id, Some(0), Some(10), None, None)
                .await
            {
                Ok(tracks_page) => {
                    let tracks = tracks_page.items();
                    println!("  Found {} tracks (showing up to 10):", tracks.len());
                    for track in tracks {
                        println!("    {}. {}", track.number, track.title);
                    }
                }
                Err(e) => println!("  Error fetching tracks: {e}"),
            }
        }
        Ok(None) => {
            println!("Album with ID {sample_album_id} not found.");
            println!("(This is expected if your library doesn't have an album with this ID)");
        }
        Err(e) => println!("Error fetching album: {e}"),
    }

    println!("\n\nExample completed successfully!");
    println!("\nTips:");
    println!("  - Set MOOSICBOX_SERVER_URL to point to your server");
    println!("  - Set MOOSICBOX_PROFILE to use a different profile");
    println!("  - All operations support pagination via offset/limit parameters");
    println!("  - Check the API documentation for more advanced features");

    Ok(())
}
