#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for the `moosicbox_qobuz` package.
//!
//! This example demonstrates how to:
//! - Build a `QobuzMusicApi` client with database support
//! - Use the `MusicApi` trait to interact with Qobuz
//! - Fetch favorite artists, albums, and tracks
//! - Search the Qobuz catalog
//!
//! Note: This example requires valid Qobuz credentials to be configured
//! in the database. See the README for setup instructions.

use moosicbox_music_api::{MusicApi, models::AlbumsRequest};
use moosicbox_paging::{Page, PagingRequest};
use moosicbox_qobuz::QobuzMusicApi;

#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("MoosicBox Qobuz API - Basic Usage Example");
    println!("==========================================\n");

    println!("The example code below shows how to use the API once configured:");

    let db = switchy::database_connection::init_sqlite_sqlx(None).await?;

    println!("Step 1: Building Qobuz API client...");
    let qobuz = QobuzMusicApi::builder().with_db(db.into()).build().await?;
    println!("✓ Qobuz API client ready\n");

    // Note: Authentication must be done separately using moosicbox_qobuz::user_login()
    // or through the API's authentication endpoints. This example assumes credentials
    // are already configured in the database.

    // Step 2: Fetch favorite artists
    // The MusicApi trait provides a consistent interface across music sources
    println!("Step 2: Fetching favorite artists...");
    match qobuz.artists(Some(0), Some(5), None, None).await {
        Ok(artists) => {
            if let Page::WithTotal { items, total, .. } = &artists.page {
                println!("✓ Found {total} total favorite artists");
                println!("  Displaying first {} artists:", items.len());
                for (i, artist) in items.iter().enumerate() {
                    println!("  {}. {} (ID: {})", i + 1, artist.title, artist.id);
                }
            }
        }
        Err(e) => {
            println!("⚠ Could not fetch artists: {e}");
            println!("  This is expected if you haven't configured Qobuz credentials yet.");
        }
    }
    println!();

    // Step 3: Fetch favorite albums
    println!("Step 3: Fetching favorite albums...");
    let albums_request = AlbumsRequest {
        page: Some(PagingRequest {
            offset: 0,
            limit: 5,
        }),
        ..Default::default()
    };

    match qobuz.albums(&albums_request).await {
        Ok(albums) => {
            if let Page::WithTotal { items, total, .. } = &albums.page {
                println!("✓ Found {total} total favorite albums");
                println!("  Displaying first {} albums:", items.len());
                for (i, album) in items.iter().enumerate() {
                    println!(
                        "  {}. {} by {} (ID: {})",
                        i + 1,
                        album.title,
                        album.artist,
                        album.id
                    );
                }
            }
        }
        Err(e) => {
            println!("⚠ Could not fetch albums: {e}");
            println!("  This is expected if you haven't configured Qobuz credentials yet.");
        }
    }
    println!();

    // Step 4: Search the Qobuz catalog
    println!("Step 4: Searching Qobuz catalog for 'jazz'...");
    match qobuz.search("jazz", Some(0), Some(10)).await {
        Ok(results) => {
            use moosicbox_music_api::models::search::api::ApiGlobalSearchResult;

            // Separate results by type
            use moosicbox_music_api::models::search::api::{
                ApiGlobalAlbumSearchResult, ApiGlobalArtistSearchResult, ApiGlobalTrackSearchResult,
            };

            type ArtistVec<'a> = Vec<&'a ApiGlobalArtistSearchResult>;
            type AlbumVec<'a> = Vec<&'a ApiGlobalAlbumSearchResult>;
            type TrackVec<'a> = Vec<&'a ApiGlobalTrackSearchResult>;

            println!("✓ Search completed successfully");
            println!("  Total results found: {}", results.results.len());

            let (artists, albums, tracks): (ArtistVec, AlbumVec, TrackVec) =
                results.results.iter().fold(
                    (vec![], vec![], vec![]),
                    |(mut artists, mut albums, mut tracks), result| {
                        match result {
                            ApiGlobalSearchResult::Artist(a) => artists.push(a),
                            ApiGlobalSearchResult::Album(a) => albums.push(a),
                            ApiGlobalSearchResult::Track(t) => tracks.push(t),
                        }
                        (artists, albums, tracks)
                    },
                );

            println!("  Artists: {}", artists.len());
            for (i, artist) in artists.iter().enumerate().take(3) {
                println!("    {}. {} ({})", i + 1, artist.title, artist.api_source);
            }

            println!("  Albums: {}", albums.len());
            for (i, album) in albums.iter().enumerate().take(3) {
                println!(
                    "    {}. {} - {} ({})",
                    i + 1,
                    album.artist,
                    album.title,
                    album.api_source
                );
            }

            println!("  Tracks: {}", tracks.len());
            for (i, track) in tracks.iter().enumerate().take(3) {
                println!(
                    "    {}. {} - {} ({})",
                    i + 1,
                    track.artist,
                    track.title,
                    track.api_source
                );
            }
        }
        Err(e) => {
            println!("⚠ Could not perform search: {e}");
            println!("  This is expected if you haven't configured Qobuz credentials yet.");
        }
    }
    println!();

    // Summary
    println!("==========================================");
    println!("Example completed!");
    println!();
    println!("Next steps:");
    println!("- Configure Qobuz credentials using the authentication API");
    println!("- Explore other MusicApi methods (add_favorite, remove_favorite, etc.)");
    println!("- Fetch album tracks and track streaming URLs");
    println!("- Try different search queries and pagination options");

    Ok(())
}
