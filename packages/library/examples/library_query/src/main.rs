//! Example demonstrating music library query operations.
//!
//! This example shows how to use the `moosicbox_library` crate to:
//! - Query favorite artists, albums, and tracks
//! - Retrieve specific items by ID
//! - Get albums for an artist
//! - Get tracks for an album
//! - Filter and sort albums
//! - Work with pagination

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_library::{
    album, album_tracks, artist, artist_albums, favorite_albums, favorite_artists, favorite_tracks,
};
use moosicbox_music_api_models::AlbumsRequest;
use moosicbox_music_models::{AlbumSort, id::Id};
use moosicbox_paging::PagingRequest;
use switchy_database::{DatabaseValue, profiles::LibraryDatabase, simulator::SimulationDatabase};

/// Main entry point for the library query example.
///
/// # Errors
///
/// Returns an error if database operations fail or if required test data cannot be created.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see trace output
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("=== MoosicBox Library Query Example ===\n");

    // Create an in-memory database for demonstration
    let db = create_test_database().await?;

    // 1. Query favorite artists
    println!("1. Querying favorite artists...");
    let artists_response = favorite_artists(&db, None, Some(10), None, None).await?;
    println!(
        "   Found {} total artists",
        artists_response.page.total().unwrap_or(0)
    );

    for artist in artists_response.page.items() {
        println!("   - Artist: {} (ID: {})", artist.title, artist.id);
    }
    println!();

    // 2. Query favorite albums with filtering and sorting
    println!("2. Querying favorite albums (sorted by name)...");
    let albums_request = AlbumsRequest {
        sources: None,
        sort: Some(AlbumSort::NameAsc),
        filters: None,
        page: Some(PagingRequest {
            offset: 0,
            limit: 10,
        }),
    };
    let albums_response = favorite_albums(&db, &albums_request).await?;
    println!(
        "   Found {} total albums",
        albums_response.page.total().unwrap_or(0)
    );

    for album in albums_response.page.items() {
        println!(
            "   - Album: {} by {} (ID: {})",
            album.title, album.artist, album.id
        );
    }
    println!();

    // 3. Get a specific artist by ID
    if let Some(first_artist) = artists_response.page.items().first() {
        let artist_id = Id::Number(first_artist.id);
        println!("3. Fetching specific artist (ID: {artist_id})...");

        match artist(&db, &artist_id).await {
            Ok(artist_info) => {
                println!("   Artist: {}", artist_info.title);
                println!("   Cover: {:?}", artist_info.cover);
            }
            Err(e) => println!("   Error: {e}"),
        }
        println!();

        // 4. Get albums for this artist
        println!("4. Fetching albums for artist...");
        let artist_albums_response = artist_albums(&db, &artist_id, None, Some(5), None).await?;
        println!(
            "   Found {} albums",
            artist_albums_response.page.total().unwrap_or(0)
        );

        for album in artist_albums_response.page.items() {
            println!("   - {}", album.title);
        }
        println!();
    }

    // 5. Get tracks for a specific album
    if let Some(first_album) = albums_response.page.items().first() {
        let album_id = Id::Number(first_album.id);
        println!("5. Fetching tracks for album '{}'...", first_album.title);

        let tracks_response = album_tracks(&db, &album_id, None, None).await?;
        println!(
            "   Found {} tracks",
            tracks_response.page.total().unwrap_or(0)
        );

        for track in tracks_response.page.items() {
            println!("   - Track {}: {}", track.number, track.title);
        }
        println!();
    }

    // 6. Query favorite tracks with pagination
    println!("6. Querying favorite tracks (paginated)...");
    let tracks_response = favorite_tracks(&db, None, Some(0), Some(5), None, None).await?;
    println!(
        "   Retrieved {} of {} tracks",
        tracks_response.page.items().len(),
        tracks_response.page.total().unwrap_or(0)
    );

    for track in tracks_response.page.items() {
        println!(
            "   - {}: {} ({})",
            track.number, track.title, track.duration
        );
    }
    println!();

    // 7. Get a specific album by ID
    if let Some(first_album) = albums_response.page.items().first() {
        let album_id = Id::Number(first_album.id);
        println!("7. Fetching specific album (ID: {album_id})...");

        match album(&db, &album_id).await? {
            Some(album_info) => {
                println!("   Album: {}", album_info.title);
                println!("   Artist: {}", album_info.artist);
                println!("   Released: {:?}", album_info.date_released);
            }
            None => println!("   Album not found"),
        }
        println!();
    }

    println!("=== Example completed successfully! ===");

    Ok(())
}

/// Creates an in-memory test database with sample music data.
///
/// # Errors
///
/// Returns an error if database operations fail.
#[allow(clippy::too_many_lines)]
async fn create_test_database() -> Result<LibraryDatabase, Box<dyn std::error::Error>> {
    // Create in-memory SQLite database using the simulation database
    // (None = in-memory database, not persisted to disk)
    let db = SimulationDatabase::new_for_path(None)?;
    let library_db = LibraryDatabase::from(std::sync::Arc::new(
        Box::new(db) as Box<dyn switchy_database::Database>
    ));

    // Create schema
    library_db
        .exec_raw(
            "CREATE TABLE IF NOT EXISTS artists (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            cover TEXT,
            date_added INTEGER NOT NULL
        )",
        )
        .await?;

    library_db
        .exec_raw(
            "CREATE TABLE IF NOT EXISTS albums (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            artist TEXT NOT NULL,
            artist_id INTEGER NOT NULL,
            date_released TEXT,
            date_added INTEGER NOT NULL,
            artwork TEXT,
            directory TEXT,
            blur INTEGER DEFAULT 0,
            album_type TEXT DEFAULT 'ALBUM'
        )",
        )
        .await?;

    library_db
        .exec_raw(
            "CREATE TABLE IF NOT EXISTS tracks (
            id INTEGER PRIMARY KEY,
            number INTEGER NOT NULL,
            title TEXT NOT NULL,
            duration REAL NOT NULL,
            album TEXT NOT NULL,
            album_id INTEGER NOT NULL,
            date_released TEXT,
            artist TEXT NOT NULL,
            artist_id INTEGER NOT NULL,
            file TEXT,
            artwork TEXT,
            blur INTEGER DEFAULT 0,
            bytes INTEGER,
            format TEXT,
            bit_depth INTEGER,
            audio_bitrate INTEGER,
            overall_bitrate INTEGER,
            sample_rate INTEGER,
            channels INTEGER
        )",
        )
        .await?;

    // Insert sample data - Artists
    library_db
        .exec_raw_params(
            "INSERT INTO artists (id, title, cover, date_added) VALUES (?, ?, ?, ?)",
            &[
                DatabaseValue::Int64(1),
                DatabaseValue::String("The Beatles".into()),
                DatabaseValue::Null,
                DatabaseValue::Int64(1_234_567_890),
            ],
        )
        .await?;

    library_db
        .exec_raw_params(
            "INSERT INTO artists (id, title, cover, date_added) VALUES (?, ?, ?, ?)",
            &[
                DatabaseValue::Int64(2),
                DatabaseValue::String("Pink Floyd".into()),
                DatabaseValue::Null,
                DatabaseValue::Int64(1_234_567_891),
            ],
        )
        .await?;

    // Insert sample data - Albums
    library_db.exec_raw_params(
        "INSERT INTO albums (id, title, artist, artist_id, date_released, date_added, artwork, directory, album_type)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &[
            DatabaseValue::Int64(1),
            DatabaseValue::String("Abbey Road".into()),
            DatabaseValue::String("The Beatles".into()),
            DatabaseValue::Int64(1),
            DatabaseValue::String("1969-09-26".into()),
            DatabaseValue::Int64(1_234_567_890),
            DatabaseValue::Null,
            DatabaseValue::String("/music/beatles/abbey-road".into()),
            DatabaseValue::String("ALBUM".into()),
        ]
    ).await?;

    library_db.exec_raw_params(
        "INSERT INTO albums (id, title, artist, artist_id, date_released, date_added, artwork, directory, album_type)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &[
            DatabaseValue::Int64(2),
            DatabaseValue::String("The Dark Side of the Moon".into()),
            DatabaseValue::String("Pink Floyd".into()),
            DatabaseValue::Int64(2),
            DatabaseValue::String("1973-03-01".into()),
            DatabaseValue::Int64(1_234_567_891),
            DatabaseValue::Null,
            DatabaseValue::String("/music/pink-floyd/dark-side".into()),
            DatabaseValue::String("ALBUM".into()),
        ]
    ).await?;

    // Insert sample data - Tracks
    library_db.exec_raw_params(
        "INSERT INTO tracks (id, number, title, duration, album, album_id, artist, artist_id, file, format)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &[
            DatabaseValue::Int64(1),
            DatabaseValue::Int64(1),
            DatabaseValue::String("Come Together".into()),
            DatabaseValue::Real64(259.0),
            DatabaseValue::String("Abbey Road".into()),
            DatabaseValue::Int64(1),
            DatabaseValue::String("The Beatles".into()),
            DatabaseValue::Int64(1),
            DatabaseValue::String("/music/beatles/abbey-road/01-come-together.flac".into()),
            DatabaseValue::String("FLAC".into()),
        ]
    ).await?;

    library_db.exec_raw_params(
        "INSERT INTO tracks (id, number, title, duration, album, album_id, artist, artist_id, file, format)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &[
            DatabaseValue::Int64(2),
            DatabaseValue::Int64(2),
            DatabaseValue::String("Something".into()),
            DatabaseValue::Real64(183.0),
            DatabaseValue::String("Abbey Road".into()),
            DatabaseValue::Int64(1),
            DatabaseValue::String("The Beatles".into()),
            DatabaseValue::Int64(1),
            DatabaseValue::String("/music/beatles/abbey-road/02-something.flac".into()),
            DatabaseValue::String("FLAC".into()),
        ]
    ).await?;

    library_db.exec_raw_params(
        "INSERT INTO tracks (id, number, title, duration, album, album_id, artist, artist_id, file, format)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &[
            DatabaseValue::Int64(3),
            DatabaseValue::Int64(1),
            DatabaseValue::String("Speak to Me".into()),
            DatabaseValue::Real64(68.0),
            DatabaseValue::String("The Dark Side of the Moon".into()),
            DatabaseValue::Int64(2),
            DatabaseValue::String("Pink Floyd".into()),
            DatabaseValue::Int64(2),
            DatabaseValue::String("/music/pink-floyd/dark-side/01-speak-to-me.flac".into()),
            DatabaseValue::String("FLAC".into()),
        ]
    ).await?;

    println!("Created in-memory database with sample music data");
    println!();

    Ok(library_db)
}
