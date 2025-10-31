#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic Usage Example for `moosicbox_search`
//!
//! This example demonstrates the core functionality of the `moosicbox_search` crate:
//! - Creating and populating a search index with music data
//! - Performing full-text searches across artists, albums, and tracks
//! - Deleting documents from the index
//! - Using both low-level and high-level search APIs

use moosicbox_search::{
    DataValue, delete_from_global_search_index, global_search, populate_global_search_index,
    search_global_search_index,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see what's happening
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("=== MoosicBox Search - Basic Usage Example ===\n");

    // Create a runtime for async operations
    let runtime = switchy_async::runtime::Runtime::new();

    // Step 1: Create sample music data to index
    println!("Step 1: Creating sample music data...");
    let music_data = create_sample_data();
    println!("  Created {} documents\n", music_data.len());

    // Step 2: Populate the search index
    println!("Step 2: Populating search index...");
    // The `delete=true` parameter clears any existing data in the index
    runtime.block_on(populate_global_search_index(&music_data, true))?;
    println!("  Index populated successfully\n");

    // Step 3: Perform searches using the high-level API
    println!("Step 3: Performing searches...\n");

    // Search for "Bohemian"
    println!("  Searching for 'Bohemian':");
    let results = global_search("Bohemian", Some(0), Some(10))?;
    print_search_results(&results.results);

    // Search for "Queen"
    println!("\n  Searching for 'Queen':");
    let results = global_search("Queen", Some(0), Some(10))?;
    print_search_results(&results.results);

    // Search for "opera"
    println!("\n  Searching for 'opera':");
    let results = global_search("opera", Some(0), Some(10))?;
    print_search_results(&results.results);

    // Step 4: Use the low-level API to get raw Tantivy documents
    println!("\nStep 4: Using low-level search API...");
    println!("  Searching for 'Pink Floyd':");
    let raw_docs = search_global_search_index("Pink Floyd", 0, 5)?;
    println!("  Found {} raw documents", raw_docs.len());
    for (i, doc) in raw_docs.iter().enumerate() {
        println!("    Document {}: {} fields", i + 1, doc.0.len());
    }

    // Step 5: Delete a document from the index
    println!("\nStep 5: Deleting a document from the index...");
    let delete_terms = vec![("track_id_string", DataValue::String("789".into()))];
    delete_from_global_search_index(&delete_terms)?;
    println!("  Deleted track with ID 789\n");

    // Step 6: Verify deletion
    println!("Step 6: Verifying deletion...");
    println!("  Searching for 'Bohemian' again:");
    let results = global_search("Bohemian", Some(0), Some(10))?;
    print_search_results(&results.results);

    println!("\n=== Example completed successfully ===");

    Ok(())
}

/// Creates sample music data for indexing
fn create_sample_data() -> Vec<Vec<(&'static str, DataValue)>> {
    vec![
        // Artist: Queen
        vec![
            ("document_type", DataValue::String("artists".into())),
            ("artist_title", DataValue::String("Queen".into())),
            ("artist_id", DataValue::String("123".into())),
            ("album_title", DataValue::String(String::new())),
            ("track_title", DataValue::String(String::new())),
            ("cover", DataValue::String("queen.jpg".into())),
            ("blur", DataValue::Bool(false)),
            ("date_released", DataValue::String(String::new())),
            ("date_added", DataValue::String(String::new())),
            ("version_formats", DataValue::String(String::new())),
            ("version_sources", DataValue::String(String::new())),
        ],
        // Album: A Night at the Opera
        vec![
            ("document_type", DataValue::String("albums".into())),
            ("artist_title", DataValue::String("Queen".into())),
            ("artist_id", DataValue::String("123".into())),
            (
                "album_title",
                DataValue::String("A Night at the Opera".into()),
            ),
            ("album_id", DataValue::String("456".into())),
            ("track_title", DataValue::String(String::new())),
            ("cover", DataValue::String("opera_cover.jpg".into())),
            ("blur", DataValue::Bool(false)),
            (
                "date_released",
                DataValue::String("1975-11-21T00:00:00Z".into()),
            ),
            ("date_added", DataValue::String(String::new())),
            ("version_formats", DataValue::String("FLAC".into())),
            ("version_bit_depths", DataValue::Number(24)),
            ("version_sample_rates", DataValue::Number(96000)),
            ("version_channels", DataValue::Number(2)),
            ("version_sources", DataValue::String("LOCAL".into())),
        ],
        // Track: Bohemian Rhapsody
        vec![
            ("document_type", DataValue::String("tracks".into())),
            ("artist_title", DataValue::String("Queen".into())),
            ("artist_id", DataValue::String("123".into())),
            (
                "album_title",
                DataValue::String("A Night at the Opera".into()),
            ),
            ("album_id", DataValue::String("456".into())),
            ("track_title", DataValue::String("Bohemian Rhapsody".into())),
            ("track_id", DataValue::String("789".into())),
            ("cover", DataValue::String("opera_cover.jpg".into())),
            ("blur", DataValue::Bool(false)),
            (
                "date_released",
                DataValue::String("1975-11-21T00:00:00Z".into()),
            ),
            ("date_added", DataValue::String(String::new())),
            ("version_formats", DataValue::String("FLAC".into())),
            ("version_bit_depths", DataValue::Number(24)),
            ("version_sample_rates", DataValue::Number(96000)),
            ("version_channels", DataValue::Number(2)),
            ("version_sources", DataValue::String("LOCAL".into())),
        ],
        // Artist: Pink Floyd
        vec![
            ("document_type", DataValue::String("artists".into())),
            ("artist_title", DataValue::String("Pink Floyd".into())),
            ("artist_id", DataValue::String("124".into())),
            ("album_title", DataValue::String(String::new())),
            ("track_title", DataValue::String(String::new())),
            ("cover", DataValue::String("pink_floyd.jpg".into())),
            ("blur", DataValue::Bool(false)),
            ("date_released", DataValue::String(String::new())),
            ("date_added", DataValue::String(String::new())),
            ("version_formats", DataValue::String(String::new())),
            ("version_sources", DataValue::String(String::new())),
        ],
        // Album: The Dark Side of the Moon
        vec![
            ("document_type", DataValue::String("albums".into())),
            ("artist_title", DataValue::String("Pink Floyd".into())),
            ("artist_id", DataValue::String("124".into())),
            (
                "album_title",
                DataValue::String("The Dark Side of the Moon".into()),
            ),
            ("album_id", DataValue::String("457".into())),
            ("track_title", DataValue::String(String::new())),
            ("cover", DataValue::String("dsotm_cover.jpg".into())),
            ("blur", DataValue::Bool(false)),
            (
                "date_released",
                DataValue::String("1973-03-01T00:00:00Z".into()),
            ),
            ("date_added", DataValue::String(String::new())),
            ("version_formats", DataValue::String("FLAC".into())),
            ("version_bit_depths", DataValue::Number(24)),
            ("version_sample_rates", DataValue::Number(192_000)),
            ("version_channels", DataValue::Number(2)),
            ("version_sources", DataValue::String("LOCAL".into())),
        ],
    ]
}

/// Helper function to print search results in a readable format
fn print_search_results(
    results: &[moosicbox_music_api_models::search::api::ApiGlobalSearchResult],
) {
    if results.is_empty() {
        println!("    No results found");
        return;
    }

    for (i, result) in results.iter().enumerate() {
        match result {
            moosicbox_music_api_models::search::api::ApiGlobalSearchResult::Artist(artist) => {
                println!("    {}. Artist: {}", i + 1, artist.title);
            }
            moosicbox_music_api_models::search::api::ApiGlobalSearchResult::Album(album) => {
                println!("    {}. Album: {} by {}", i + 1, album.title, album.artist);
            }
            moosicbox_music_api_models::search::api::ApiGlobalSearchResult::Track(track) => {
                println!(
                    "    {}. Track: {} - {} (from {})",
                    i + 1,
                    track.artist,
                    track.title,
                    track.album
                );
            }
        }
    }
}
