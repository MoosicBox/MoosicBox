#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `moosicbox_menu` demonstrating artist and album queries.
//!
//! This example shows how to:
//! - Initialize a database connection
//! - Query artists with filtering and sorting
//! - Retrieve albums for an artist
//! - Display results

use std::sync::Arc;

use moosicbox_menu::library::artists::{ArtistFilters, ArtistsRequest, get_all_artists};
use moosicbox_music_models::ArtistSort;
use switchy_database::{Database, profiles::LibraryDatabase, turso::TursoDatabase};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see debug output
    env_logger::init();

    println!("MoosicBox Menu - Basic Usage Example");
    println!("=====================================\n");

    // Step 1: Create an in-memory database for demonstration
    // In a real application, you would connect to an existing database
    println!("Step 1: Initializing database...");
    let db = TursoDatabase::new(":memory:").await?;
    println!("✓ Database initialized\n");

    // Step 2: Set up database schema
    // Create tables needed for the menu functionality
    println!("Step 2: Creating database schema...");
    setup_schema(&db).await?;
    println!("✓ Schema created\n");

    // Step 3: Insert sample data
    println!("Step 3: Inserting sample data...");
    insert_sample_data(&db).await?;
    println!("✓ Sample data inserted\n");

    // Wrap in LibraryDatabase for menu operations
    let library_db: LibraryDatabase = Arc::new(Box::new(db) as Box<dyn Database>).into();

    // Step 4: Query all artists with default settings
    println!("Step 4: Querying all artists...");
    let request = ArtistsRequest {
        sources: None,
        sort: Some(ArtistSort::NameAsc),
        filters: ArtistFilters {
            name: None,
            search: None,
        },
    };

    let artists = get_all_artists(&library_db, &request).await?;
    println!("✓ Found {} artists:", artists.len());
    for artist in &artists {
        println!("  - {} (ID: {})", artist.title, artist.id);
    }
    println!();

    // Step 5: Query artists with a search filter
    println!("Step 5: Searching for artists containing 'Rock'...");
    let search_request = ArtistsRequest {
        sources: None,
        sort: Some(ArtistSort::NameAsc),
        filters: ArtistFilters {
            name: None,
            search: Some("rock".to_lowercase()),
        },
    };

    let filtered_artists = get_all_artists(&library_db, &search_request).await?;
    println!("✓ Found {} matching artists:", filtered_artists.len());
    for artist in &filtered_artists {
        println!("  - {} (ID: {})", artist.title, artist.id);
    }
    println!();

    // Step 6: Query artists with name sorting
    println!("Step 6: Querying artists sorted by name (descending)...");
    let sorted_request = ArtistsRequest {
        sources: None,
        sort: Some(ArtistSort::NameDesc),
        filters: ArtistFilters {
            name: None,
            search: None,
        },
    };

    let sorted_artists = get_all_artists(&library_db, &sorted_request).await?;
    println!("✓ Artists sorted (descending):");
    for artist in &sorted_artists {
        println!("  - {} (ID: {})", artist.title, artist.id);
    }
    println!();

    println!("Example completed successfully!");
    println!("\nKey takeaways:");
    println!("- Use ArtistsRequest to configure queries");
    println!("- Apply filters via ArtistFilters (name, search)");
    println!("- Sort results with ArtistSort (NameAsc, NameDesc)");
    println!("- LibraryDatabase provides the database abstraction");

    Ok(())
}

/// Sets up the database schema required for menu operations.
async fn setup_schema(db: &TursoDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Create artists table
    db.exec_raw(
        "CREATE TABLE IF NOT EXISTS artists (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            cover TEXT,
            source TEXT NOT NULL
        )",
    )
    .await?;

    // Create albums table
    db.exec_raw(
        "CREATE TABLE IF NOT EXISTS albums (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            artist_id INTEGER NOT NULL,
            date_released TEXT,
            artwork TEXT,
            source TEXT NOT NULL,
            FOREIGN KEY (artist_id) REFERENCES artists(id)
        )",
    )
    .await?;

    Ok(())
}

/// Inserts sample data for demonstration purposes.
async fn insert_sample_data(db: &TursoDatabase) -> Result<(), Box<dyn std::error::Error>> {
    // Insert sample artists
    db.exec_raw(
        "INSERT INTO artists (id, title, cover, source) VALUES
            (1, 'The Classic Rock Band', NULL, 'LIBRARY'),
            (2, 'Jazz Ensemble', NULL, 'LIBRARY'),
            (3, 'Electronic Pioneers', NULL, 'LIBRARY'),
            (4, 'Indie Rock Collective', NULL, 'LIBRARY')",
    )
    .await?;

    // Insert sample albums
    db.exec_raw(
        "INSERT INTO albums (id, title, artist_id, date_released, artwork, source) VALUES
            (1, 'Greatest Hits', 1, '2020-01-01', NULL, 'LIBRARY'),
            (2, 'Live at the Jazz Club', 2, '2021-06-15', NULL, 'LIBRARY'),
            (3, 'Electronic Dreams', 3, '2019-03-20', NULL, 'LIBRARY'),
            (4, 'Indie Vibes', 4, '2022-11-10', NULL, 'LIBRARY')",
    )
    .await?;

    Ok(())
}
