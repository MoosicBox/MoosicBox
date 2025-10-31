#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Example demonstrating basic JSON value conversion using `moosicbox_json_utils`.
//!
//! This example shows how to:
//! - Convert JSON values to Rust types using `ToValueType`
//! - Extract values from JSON objects using `ToValue`
//! - Navigate nested JSON structures using `ToNestedValue`
//! - Handle optional values and error cases

use moosicbox_json_utils::serde_json::{ToNestedValue, ToValue};
use moosicbox_json_utils::{ParseError, ToValueType};
use serde_json::{Value, json};

fn main() -> Result<(), ParseError> {
    println!("=== MoosicBox JSON Utils: Basic Conversion Example ===\n");

    // Example 1: Basic type conversions
    println!("1. Basic Type Conversions:");
    basic_type_conversions()?;

    // Example 2: Extracting values from JSON objects
    println!("\n2. Extracting Values from Objects:");
    extract_from_objects()?;

    // Example 3: Navigating nested structures
    println!("\n3. Navigating Nested Structures:");
    nested_navigation()?;

    // Example 4: Working with arrays
    println!("\n4. Working with Arrays:");
    array_handling()?;

    // Example 5: Handling optional values
    println!("\n5. Handling Optional Values:");
    optional_values()?;

    // Example 6: Error handling
    println!("\n6. Error Handling:");
    error_handling();

    println!("\n=== All examples completed successfully! ===");
    Ok(())
}

/// Demonstrates basic type conversions from JSON values to Rust types.
fn basic_type_conversions() -> Result<(), ParseError> {
    // Convert JSON number to i32
    let json_number = json!(42);
    let number: i32 = (&json_number).to_value_type()?;
    println!("  JSON number to i32: {json_number} -> {number}");

    // Convert JSON boolean
    let json_bool = json!(true);
    let boolean: bool = (&json_bool).to_value_type()?;
    println!("  JSON boolean: {json_bool} -> {boolean}");

    // Convert JSON string
    let json_string = json!("Hello, MoosicBox!");
    let text: String = (&json_string).to_value_type()?;
    println!("  JSON string: {json_string} -> '{text}'");

    // Convert JSON float
    let json_float = json!(123.456);
    let float_val: f64 = (&json_float).to_value_type()?;
    println!("  JSON float: {json_float} -> {float_val}");

    Ok(())
}

/// Demonstrates extracting values from JSON objects by key.
fn extract_from_objects() -> Result<(), ParseError> {
    // Create a JSON object representing a music track
    let track = json!({
        "title": "Bohemian Rhapsody",
        "artist": "Queen",
        "duration": 355,
        "year": 1975,
        "is_favorite": true
    });

    // Extract values by key using ToValue trait
    let title: String = track.to_value("title")?;
    let artist: String = track.to_value("artist")?;
    let duration: u32 = track.to_value("duration")?;
    let year: u16 = track.to_value("year")?;
    let is_favorite: bool = track.to_value("is_favorite")?;

    println!("  Track: '{title}' by {artist}");
    println!("  Duration: {duration}s, Year: {year}, Favorite: {is_favorite}");

    Ok(())
}

/// Demonstrates navigating nested JSON structures.
fn nested_navigation() -> Result<(), ParseError> {
    // Create a nested JSON structure
    let album = json!({
        "title": "The Dark Side of the Moon",
        "metadata": {
            "artist": "Pink Floyd",
            "release": {
                "year": 1973,
                "label": "Harvest Records"
            },
            "stats": {
                "tracks": 10,
                "duration": 2532
            }
        }
    });

    // Navigate to nested values using paths
    let title: String = album.to_nested_value(&["title"])?;
    let artist: String = album.to_nested_value(&["metadata", "artist"])?;
    let year: u16 = album.to_nested_value(&["metadata", "release", "year"])?;
    let label: String = album.to_nested_value(&["metadata", "release", "label"])?;
    let track_count: u8 = album.to_nested_value(&["metadata", "stats", "tracks"])?;
    let duration: u32 = album.to_nested_value(&["metadata", "stats", "duration"])?;

    println!("  Album: '{title}'");
    println!("  Artist: {artist}");
    println!("  Released: {year} on {label}");
    println!("  {track_count} tracks, {duration} seconds total");

    Ok(())
}

/// Demonstrates working with JSON arrays.
fn array_handling() -> Result<(), ParseError> {
    // Create a JSON array
    let json_array = json!([1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);

    // Convert entire array to Vec<i32>
    let numbers: Vec<i32> = (&json_array).to_value_type()?;
    println!("  Array of numbers: {numbers:?}");
    let sum = numbers.iter().sum::<i32>();
    println!("  Sum: {sum}");

    // Work with arrays of strings
    let genres = json!(["rock", "jazz", "classical", "electronic"]);
    let genre_list: Vec<String> = (&genres).to_value_type()?;
    let genres_str = genre_list.join(", ");
    println!("  Genres: {genres_str}");

    // Extract array from object and process items
    let playlist = json!({
        "tracks": [
            {"title": "Song A", "duration": 180},
            {"title": "Song B", "duration": 210},
            {"title": "Song C", "duration": 195}
        ]
    });

    // Get array of track objects
    let tracks: Vec<&Value> = playlist.to_value("tracks")?;
    let track_count = tracks.len();
    println!("  Playlist with {track_count} tracks:");

    let mut total_duration = 0;
    for track in &tracks {
        let title: String = track.to_value("title")?;
        let duration: u32 = track.to_value("duration")?;
        total_duration += duration;
        println!("    - '{title}' ({duration}s)");
    }
    println!("  Total duration: {total_duration}s");

    Ok(())
}

/// Demonstrates handling optional values and null.
fn optional_values() -> Result<(), ParseError> {
    let data = json!({
        "name": "John Doe",
        "email": "john@example.com",
        "phone": null,
        "age": 30
    });

    // Required values
    let name: String = data.to_value("name")?;
    let email: String = data.to_value("email")?;
    println!("  Name: {name}");
    println!("  Email: {email}");

    // Optional value (explicitly null)
    let phone: Option<String> = data.to_value("phone")?;
    println!("  Phone: {phone:?}"); // None

    // Optional value (missing field)
    let address: Option<String> = data.to_value("address")?;
    println!("  Address: {address:?}"); // None

    // Optional value (present)
    let age: Option<u32> = data.to_value("age")?;
    println!("  Age: {age:?}"); // Some(30)

    // Using match to handle optional values
    match data.to_value::<Option<String>>("phone")? {
        Some(p) => println!("  Contact via phone: {p}"),
        None => println!("  No phone number on file"),
    }

    Ok(())
}

/// Demonstrates error handling patterns.
fn error_handling() {
    let data = json!({
        "count": 42,
        "text": "hello"
    });

    // Type mismatch error
    println!("  Attempting invalid conversion (string as number):");
    match data.to_value::<u32>("text") {
        Ok(val) => println!("    Unexpected success: {val}"),
        Err(ParseError::ConvertType(msg)) => {
            println!("    ConvertType error (expected): {msg}");
        }
        Err(e) => println!("    Other error: {e:?}"),
    }

    // Missing value error
    println!("  Attempting to access non-existent field:");
    match data.to_value::<String>("missing") {
        Ok(val) => println!("    Unexpected success: {val}"),
        Err(ParseError::Parse(msg)) => {
            println!("    Parse error (expected): {msg}");
        }
        Err(e) => println!("    Other error: {e:?}"),
    }

    // Successful conversion using ? operator would propagate errors
    println!("  Valid conversion:");
    if let Ok(count) = data.to_value::<u32>("count") {
        println!("    Successfully extracted count: {count}");
    }
}
