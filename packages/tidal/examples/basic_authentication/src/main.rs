#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions, clippy::too_many_lines)]

//! Basic Tidal authentication and favorites retrieval example.
//!
//! This example demonstrates:
//! - `OAuth2` device authorization flow
//! - Retrieving favorite artists
//! - Retrieving favorite albums
//! - Basic API usage patterns

use moosicbox_music_api::MusicApi;
use moosicbox_music_api_models::{AlbumsRequest, search::api::ApiGlobalSearchResult};
use moosicbox_paging::PagingRequest;
use moosicbox_tidal::{TidalMusicApi, device_authorization, device_authorization_token};
use serde_json::Value;
use std::io::{self, Write};
use std::sync::Arc;
use switchy_database::{Database, profiles::LibraryDatabase, simulator::SimulationDatabase};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging for better visibility
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("=== MoosicBox Tidal Authentication Example ===\n");

    // Step 1: Get client credentials from user
    // In production, these would be securely stored configuration values
    let client_id = get_input("Enter your Tidal Client ID: ")?;
    let client_secret = get_input("Enter your Tidal Client Secret: ")?;

    println!("\n--- Starting OAuth2 Device Authorization Flow ---\n");

    // Step 2: Initiate device authorization
    // This will return a device code and user code, and open the authorization URL in browser
    let auth_response = device_authorization(client_id.clone(), true).await?;

    println!("Device authorization initiated!");
    println!(
        "Device Code: {}",
        auth_response
            .get("deviceCode")
            .and_then(Value::as_str)
            .unwrap_or("N/A")
    );
    println!(
        "User Code: {}",
        auth_response
            .get("userCode")
            .and_then(Value::as_str)
            .unwrap_or("N/A")
    );
    println!(
        "Verification URI: {}",
        auth_response
            .get("verificationUri")
            .and_then(Value::as_str)
            .unwrap_or("N/A")
    );
    println!(
        "Complete Verification URI: {}",
        auth_response
            .get("verificationUriComplete")
            .and_then(Value::as_str)
            .unwrap_or("N/A")
    );
    println!(
        "Authorization expires in {} seconds",
        auth_response
            .get("expiresIn")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );

    println!("\nThe authorization page should have opened in your browser.");
    println!("Please complete the authorization process...\n");

    // Wait for user to complete authorization in browser
    wait_for_enter("Press Enter after completing authorization in your browser...")?;

    // Step 3: Create an in-memory database for this example
    // In production, you would use a real database with persistence
    let sim_db = SimulationDatabase::new_for_path(None)?;
    let db = LibraryDatabase::from(Arc::new(Box::new(sim_db) as Box<dyn Database>));

    println!("\n--- Exchanging Device Code for Access Token ---\n");

    // Step 4: Exchange device code for access token
    // This will persist the token to the database if persist_to_db is true
    let device_code = auth_response
        .get("deviceCode")
        .and_then(Value::as_str)
        .ok_or("Missing device code")?
        .to_string();

    let token_response = device_authorization_token(
        &db,
        client_id.clone(),
        client_secret.clone(),
        device_code,
        Some(true), // persist to database
    )
    .await?;

    println!("✓ Authentication successful!");
    if let Some(access_token) = token_response.get("access_token").and_then(Value::as_str) {
        let preview = if access_token.len() > 20 {
            &access_token[..20]
        } else {
            access_token
        };
        println!("Access token obtained: {preview}...");
    }
    println!(
        "Token expires in: {} seconds",
        token_response
            .get("expires_in")
            .and_then(Value::as_u64)
            .unwrap_or(0)
    );
    println!(
        "Token type: {}",
        token_response
            .get("token_type")
            .and_then(Value::as_str)
            .unwrap_or("N/A")
    );

    // Step 5: Create Tidal API instance using the database
    // This will automatically use the persisted credentials
    println!("\n--- Initializing Tidal API Client ---\n");

    let tidal_api = TidalMusicApi::builder().with_db(db).build().await?;

    println!("✓ Tidal API client initialized");

    // Step 6: Retrieve favorite artists
    println!("\n--- Fetching Favorite Artists ---\n");

    let artists = tidal_api
        .artists(
            Some(0),  // offset
            Some(10), // limit - fetch first 10
            None,     // order
            None,     // order direction
        )
        .await?;

    println!("✓ Retrieved {} favorite artists", artists.items().len());
    println!("Total favorite artists: {}", artists.total().unwrap_or(0));

    if artists.items().is_empty() {
        println!("  (No favorite artists found)");
    } else {
        for (idx, artist) in artists.items().iter().enumerate() {
            println!("  {}. {} (ID: {})", idx + 1, artist.title, artist.id);
            if let Some(cover) = &artist.cover {
                println!("     Cover: {cover}");
            }
        }
    }

    // Step 7: Retrieve favorite albums
    println!("\n--- Fetching Favorite Albums ---\n");

    let albums_request = AlbumsRequest {
        sources: None,
        page: Some(PagingRequest {
            offset: 0,
            limit: 10, // fetch first 10
        }),
        sort: None,
        filters: None,
    };

    let albums = tidal_api.albums(&albums_request).await?;

    println!("✓ Retrieved {} favorite albums", albums.items().len());
    println!("Total favorite albums: {}", albums.total().unwrap_or(0));

    if albums.items().is_empty() {
        println!("  (No favorite albums found)");
    } else {
        for (idx, album) in albums.items().iter().enumerate() {
            println!("  {}. {} (ID: {})", idx + 1, album.title, album.id);
            println!("     Artist: {}", album.artist);
            if let Some(date) = &album.date_released {
                println!("     Released: {}", date.format("%Y-%m-%d"));
            }
            if let Some(artwork) = &album.artwork {
                println!("     Artwork: {artwork}");
            }
        }
    }

    // Step 8: Demonstrate search functionality
    println!("\n--- Searching for 'Pink Floyd' ---\n");

    let search_results = tidal_api
        .search(
            "Pink Floyd",
            Some(0), // offset
            Some(5), // limit - top 5 results
        )
        .await?;

    println!("✓ Search completed");
    println!("\nSearch Results:");

    // The search results contain mixed items (artists, albums, tracks)
    for (idx, result) in search_results.results.iter().enumerate() {
        match result {
            ApiGlobalSearchResult::Artist(artist) => {
                println!("  {}. [Artist] {}", idx + 1, artist.title);
                if artist.contains_cover {
                    println!("     Has cover art");
                }
            }
            ApiGlobalSearchResult::Album(album) => {
                println!("  {}. [Album] {} - {}", idx + 1, album.title, album.artist);
                if let Some(year) = &album.date_released {
                    println!("     Released: {year}");
                }
            }
            ApiGlobalSearchResult::Track(track) => {
                println!("  {}. [Track] {} - {}", idx + 1, track.title, &track.artist);
                println!("     Album: {}", &track.album);
            }
        }
    }

    println!("\n=== Example Completed Successfully ===");
    println!("\nNote: In this example, we used an in-memory database.");
    println!("In production, use a persistent database to store credentials across runs.");

    Ok(())
}

/// Helper function to read input from stdin
fn get_input(prompt: &str) -> io::Result<String> {
    print!("{prompt}");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().to_string())
}

/// Helper function to wait for user to press Enter
fn wait_for_enter(prompt: &str) -> io::Result<()> {
    print!("{prompt}");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(())
}
