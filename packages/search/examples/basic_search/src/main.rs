#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic Search Example
//!
//! This example demonstrates the core functionality of the `moosicbox_search` package:
//! 1. Creating and populating a search index with music data
//! 2. Performing searches with different query patterns
//! 3. Updating the index with new data
//! 4. Deleting items from the index
//!
//! The example uses an in-memory index (via the simulator feature) for simplicity
//! and portability.

use moosicbox_search::{
    DataValue, delete_from_global_search_index, global_search, populate_global_search_index,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see what's happening
    env_logger::init();

    println!("=== MoosicBox Search - Basic Example ===\n");

    // Step 1: Create sample music data for indexing
    println!("Step 1: Preparing sample music data...");
    let music_data = create_sample_music_data();
    println!("  - Created {} documents\n", music_data.len());

    // Step 2: Populate the search index
    println!("Step 2: Populating the search index...");
    // The 'true' parameter means we'll delete any existing data first
    populate_global_search_index(&music_data, true).await?;
    println!("  - Index populated successfully\n");

    // Step 3: Perform various searches
    println!("Step 3: Performing searches...\n");

    // Search for an artist
    println!("Search 1: Looking for 'Pink Floyd'");
    let results = global_search("Pink Floyd", Some(0), Some(10))?;
    println!("  - Found {} results", results.results.len());
    for result in &results.results {
        println!("    * {result:?}");
    }
    println!();

    // Search for an album
    println!("Search 2: Looking for 'Dark Side'");
    let results = global_search("Dark Side", Some(0), Some(10))?;
    println!("  - Found {} results", results.results.len());
    for result in &results.results {
        println!("    * {result:?}");
    }
    println!();

    // Search for a track
    println!("Search 3: Looking for 'Wish You Were'");
    let results = global_search("Wish You Were", Some(0), Some(10))?;
    println!("  - Found {} results", results.results.len());
    for result in &results.results {
        println!("    * {result:?}");
    }
    println!();

    // Fuzzy search (with typo)
    println!("Search 4: Fuzzy search - 'Pnk Floid' (with typos)");
    let results = global_search("Pnk Floid", Some(0), Some(10))?;
    println!(
        "  - Found {} results (fuzzy matching!)",
        results.results.len()
    );
    for result in &results.results {
        println!("    * {result:?}");
    }
    println!();

    // Step 4: Add more data to the index
    println!("Step 4: Adding new artist to the index...");
    let new_data = vec![vec![
        ("document_type", DataValue::String("artists".into())),
        ("artist_title", DataValue::String("Led Zeppelin".into())),
        ("artist_id", DataValue::String("999".into())),
        ("album_title", DataValue::String(String::new())),
        ("track_title", DataValue::String(String::new())),
        ("cover", DataValue::String(String::new())),
        ("blur", DataValue::Bool(false)),
        ("date_released", DataValue::String(String::new())),
        ("date_added", DataValue::String(String::new())),
        ("version_formats", DataValue::String(String::new())),
        ("version_sources", DataValue::String(String::new())),
    ]];

    // The 'false' parameter means we append without deleting existing data
    populate_global_search_index(&new_data, false).await?;
    println!("  - Added Led Zeppelin to index\n");

    // Search for the new artist
    println!("Search 5: Looking for 'Led Zeppelin'");
    let results = global_search("Led Zeppelin", Some(0), Some(10))?;
    println!("  - Found {} results", results.results.len());
    for result in &results.results {
        println!("    * {result:?}");
    }
    println!();

    // Step 5: Delete an item from the index
    println!("Step 5: Deleting a track from the index...");
    let delete_terms = vec![(
        "track_id_string",
        DataValue::String("1001".into()), // Time track ID
    )];
    delete_from_global_search_index(&delete_terms)?;
    println!("  - Deleted track with ID 1001\n");

    // Verify deletion
    println!("Search 6: Looking for 'Time' (should find fewer results)");
    let results = global_search("Time", Some(0), Some(10))?;
    println!("  - Found {} results", results.results.len());
    for result in &results.results {
        println!("    * {result:?}");
    }
    println!();

    // Step 6: Pagination example
    println!("Step 7: Pagination - Getting results in batches");
    println!("  Search for 'Floyd' with limit of 2:");

    let page1 = global_search("Floyd", Some(0), Some(2))?;
    println!("    Page 1: {} results", page1.results.len());

    let page2 = global_search("Floyd", Some(2), Some(2))?;
    println!("    Page 2: {} results", page2.results.len());
    println!();

    println!("=== Example Complete ===");
    println!("\nKey Takeaways:");
    println!("  ✓ Created and populated a search index with music data");
    println!("  ✓ Performed exact, partial, and fuzzy searches");
    println!("  ✓ Added new data to an existing index");
    println!("  ✓ Deleted items from the index");
    println!("  ✓ Demonstrated pagination");

    Ok(())
}

/// Creates sample music data for demonstration purposes.
///
/// Returns a vector of documents, where each document is a vector of
/// field-value pairs ready to be indexed.
#[allow(clippy::too_many_lines)]
fn create_sample_music_data() -> Vec<Vec<(&'static str, DataValue)>> {
    vec![
        // Artist: Pink Floyd
        vec![
            ("document_type", DataValue::String("artists".into())),
            ("artist_title", DataValue::String("Pink Floyd".into())),
            ("artist_id", DataValue::String("100".into())),
            ("album_title", DataValue::String(String::new())),
            ("track_title", DataValue::String(String::new())),
            ("cover", DataValue::String("pink-floyd-cover.jpg".into())),
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
            ("artist_id", DataValue::String("100".into())),
            (
                "album_title",
                DataValue::String("The Dark Side of the Moon".into()),
            ),
            ("album_id", DataValue::String("200".into())),
            ("track_title", DataValue::String(String::new())),
            ("cover", DataValue::String("dsotm-cover.jpg".into())),
            ("blur", DataValue::Bool(false)),
            (
                "date_released",
                DataValue::String("1973-03-01T00:00:00Z".into()),
            ),
            (
                "date_added",
                DataValue::String("2024-01-15T00:00:00Z".into()),
            ),
            ("version_formats", DataValue::String("FLAC".into())),
            ("version_sources", DataValue::String("LOCAL".into())),
        ],
        // Track: Time
        vec![
            ("document_type", DataValue::String("tracks".into())),
            ("artist_title", DataValue::String("Pink Floyd".into())),
            ("artist_id", DataValue::String("100".into())),
            (
                "album_title",
                DataValue::String("The Dark Side of the Moon".into()),
            ),
            ("album_id", DataValue::String("200".into())),
            ("track_title", DataValue::String("Time".into())),
            ("track_id", DataValue::String("1001".into())),
            ("cover", DataValue::String("dsotm-cover.jpg".into())),
            ("blur", DataValue::Bool(false)),
            (
                "date_released",
                DataValue::String("1973-03-01T00:00:00Z".into()),
            ),
            (
                "date_added",
                DataValue::String("2024-01-15T00:00:00Z".into()),
            ),
            ("version_formats", DataValue::String("FLAC".into())),
            ("version_sources", DataValue::String("LOCAL".into())),
        ],
        // Album: Wish You Were Here
        vec![
            ("document_type", DataValue::String("albums".into())),
            ("artist_title", DataValue::String("Pink Floyd".into())),
            ("artist_id", DataValue::String("100".into())),
            (
                "album_title",
                DataValue::String("Wish You Were Here".into()),
            ),
            ("album_id", DataValue::String("201".into())),
            ("track_title", DataValue::String(String::new())),
            ("cover", DataValue::String("wywh-cover.jpg".into())),
            ("blur", DataValue::Bool(false)),
            (
                "date_released",
                DataValue::String("1975-09-12T00:00:00Z".into()),
            ),
            (
                "date_added",
                DataValue::String("2024-02-20T00:00:00Z".into()),
            ),
            ("version_formats", DataValue::String("FLAC".into())),
            ("version_sources", DataValue::String("LOCAL".into())),
        ],
        // Track: Shine On You Crazy Diamond
        vec![
            ("document_type", DataValue::String("tracks".into())),
            ("artist_title", DataValue::String("Pink Floyd".into())),
            ("artist_id", DataValue::String("100".into())),
            (
                "album_title",
                DataValue::String("Wish You Were Here".into()),
            ),
            ("album_id", DataValue::String("201".into())),
            (
                "track_title",
                DataValue::String("Shine On You Crazy Diamond".into()),
            ),
            ("track_id", DataValue::String("1002".into())),
            ("cover", DataValue::String("wywh-cover.jpg".into())),
            ("blur", DataValue::Bool(false)),
            (
                "date_released",
                DataValue::String("1975-09-12T00:00:00Z".into()),
            ),
            (
                "date_added",
                DataValue::String("2024-02-20T00:00:00Z".into()),
            ),
            ("version_formats", DataValue::String("FLAC".into())),
            ("version_sources", DataValue::String("LOCAL".into())),
        ],
    ]
}
