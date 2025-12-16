# MoosicBox JSON Utils

Simple JSON utility library for the MoosicBox ecosystem, providing type-safe JSON value conversion traits and basic parsing utilities for database and search engine integration.

## Features

- **Type-Safe Conversion**: Convert JSON values to specific Rust types with validation
- **Database Integration**: JSON utilities for database value conversion
- **Tantivy Support**: JSON processing helpers for search engine indexing
- **SQLite Integration**: JSON handling utilities for SQLite operations
- **Serde JSON Extensions**: Additional conversion traits for serde_json values
- **Error Handling**: Basic error handling for JSON parsing operations
- **Nested Value Access**: Helper functions for accessing nested JSON properties

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_json_utils = "0.1.4"
```

## Usage

### Basic Type Conversion

```rust
use moosicbox_json_utils::{ToValueType, ParseError};

fn main() -> Result<(), ParseError> {
    // Convert JSON number to i32
    let json_value = serde_json::json!(42);
    let number: i32 = (&json_value).to_value_type()?;
    assert_eq!(number, 42);

    // Convert JSON boolean
    let json_bool = serde_json::json!(true);
    let boolean: bool = (&json_bool).to_value_type()?;
    assert_eq!(boolean, true);

    // Convert JSON array to vector
    let json_array = serde_json::json!([1, 2, 3, 4, 5]);
    let numbers: Vec<i32> = (&json_array).to_value_type()?;
    assert_eq!(numbers, vec![1, 2, 3, 4, 5]);

    // Convert JSON string
    let json_string = serde_json::json!("hello");
    let text: String = (&json_string).to_value_type()?;
    assert_eq!(text, "hello");

    Ok(())
}
```

### Nested Value Access

```rust
use moosicbox_json_utils::serde_json::ToNestedValue;

let metadata = serde_json::json!({
    "track": {
        "title": "Bohemian Rhapsody",
        "artist": "Queen",
        "details": {
            "duration": 355,
            "year": 1975
        }
    }
});

// Access nested values
let title: String = metadata.to_nested_value(&["track", "title"])?;
let duration: u32 = metadata.to_nested_value(&["track", "details", "duration"])?;

println!("Title: {}, Duration: {}s", title, duration);
```

### Optional Value Handling

```rust
use moosicbox_json_utils::serde_json::ToValue;

let data = serde_json::json!({
    "name": "John",
    "age": null
});

// Handle optional values gracefully
let name: String = data.to_value("name")?;
let age: Option<u32> = data.to_value("age")?;

println!("Name: {}, Age: {:?}", name, age); // Age will be None
```

## Programming Interface

### Core Traits

```rust
pub trait ToValueType<T> {
    fn to_value_type(self) -> Result<T, ParseError>;
    fn missing_value(&self, error: ParseError) -> Result<T, ParseError> {
        Err(error)
    }
}

pub trait MissingValue<Type> {
    fn missing_value(&self, error: ParseError) -> Result<Type, ParseError> {
        Err(error)
    }
}

```

### Error Types

```rust
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ParseError {
    #[error("Failed to parse property: {0:?}")]
    Parse(String),

    #[error("Failed to convert to type: {0:?}")]
    ConvertType(String),

    #[error("Missing required value: {0:?}")]
    MissingValue(String),
}
```

### Database Integration

```rust
#[cfg(feature = "database")]
pub mod database {
    use switchy_database::DatabaseValue;

    // DatabaseValue implements ToValueType for various types
    impl ToValueType<String> for &DatabaseValue { /* ... */ }
    impl ToValueType<bool> for &DatabaseValue { /* ... */ }
    impl ToValueType<u64> for &DatabaseValue { /* ... */ }
    // ... and many other numeric types

    // Provides a ToValue trait for Row types
    pub trait ToValue<Type> {
        fn to_value<T>(self, index: &str) -> Result<T, ParseError>
        where
            Type: ToValueType<T>;
    }
}
```

### Tantivy Integration

```rust
#[cfg(feature = "tantivy")]
pub mod tantivy {
    use tantivy::schema::OwnedValue;

    // OwnedValue implements ToValueType for various types
    impl ToValueType<String> for &OwnedValue { /* ... */ }
    impl ToValueType<bool> for &OwnedValue { /* ... */ }
    impl ToValueType<u64> for &OwnedValue { /* ... */ }
    // ... and many other numeric types

    // Provides a ToValue trait for NamedFieldDocument
    pub trait ToValue<Type> {
        fn to_value<'a, T>(&'a self, index: &str) -> Result<T, ParseError>
        where
            Type: 'a,
            &'a Type: ToValueType<T>;
    }
}
```

### SQLite Integration

```rust
#[cfg(feature = "rusqlite")]
pub mod rusqlite {
    use rusqlite::types::Value;

    // Value implements ToValueType for various types
    impl ToValueType<String> for &Value { /* ... */ }
    impl ToValueType<bool> for &Value { /* ... */ }
    impl ToValueType<u64> for &Value { /* ... */ }
    // ... and many other numeric types

    // Provides a ToValue trait for rusqlite Row types
    pub trait ToValue<Type> {
        fn to_value<T>(self, index: &str) -> Result<T, ParseError>
        where
            Type: ToValueType<T>;
    }
}
```

## Configuration

### Feature Flags

- `database`: Enable database integration utilities (requires `switchy_database`)
- `rusqlite`: Enable SQLite-specific value conversion functions
- `tantivy`: Enable Tantivy search engine value conversion
- `serde_json`: Enable serde_json value conversion utilities
- `decimal`: Enable decimal type support (requires `database` feature)
- `uuid`: Enable UUID type support (requires `database` feature)

## Integration Examples

### Music Library JSON Processing

```rust
use moosicbox_json_utils::serde_json::ToValue;

fn process_album_metadata() -> Result<(), Box<dyn std::error::Error>> {
    // Process album metadata
    let album_json = serde_json::json!({
        "title": "The Dark Side of the Moon",
        "artist": "Pink Floyd",
        "release_year": 1973,
        "tracks": [
            {
                "title": "Speak to Me",
                "duration": 90,
                "track_number": 1
            },
            {
                "title": "Breathe (In the Air)",
                "duration": 163,
                "track_number": 2
            }
        ],
        "genres": ["progressive rock", "psychedelic rock"],
        "total_duration": 2532
    });

    // Extract values using ToValue trait
    let title: String = album_json.to_value("title")?;
    let artist: String = album_json.to_value("artist")?;
    let release_year: u32 = album_json.to_value("release_year")?;
    let tracks: Vec<&serde_json::Value> = album_json.to_value("tracks")?;
    let genres: Vec<String> = album_json.to_value("genres")?;

    println!("Album: {} by {} ({})", title, artist, release_year);
    println!("Tracks: {}", tracks.len());
    println!("Genres: {:?}", genres);

    // Process individual tracks
    for track in tracks {
        let track_title: String = track.to_value("title")?;
        let duration: u32 = track.to_value("duration")?;
        let track_number: u8 = track.to_value("track_number")?;

        println!("  {}. {} ({}s)", track_number, track_title, duration);
    }

    Ok(())
}
```

### Database Value Conversion

```rust
use moosicbox_json_utils::database::ToValue;
use switchy_database::{Database, DatabaseValue, Row};

async fn query_albums(db: &dyn Database) -> Result<(), Box<dyn std::error::Error>> {
    // Query album data from database
    let rows = db.query("SELECT title, artist, year FROM albums", &[]).await?;

    for row in rows {
        // Extract values from database row using ToValue trait
        let title: String = row.to_value("title")?;
        let artist: String = row.to_value("artist")?;
        let year: Option<u32> = row.to_value("year")?; // Optional values supported

        println!("Album: {} by {}", title, artist);
        if let Some(y) = year {
            println!("  Released: {}", y);
        }
    }

    Ok(())
}
```

## Error Handling

```rust
use moosicbox_json_utils::ParseError;

match json_value.to_value_type::<String>() {
    Ok(string_value) => {
        println!("Parsed string: {}", string_value);
    }
    Err(ParseError::Parse(msg)) => {
        eprintln!("Parse error: {}", msg);
    }
    Err(ParseError::ConvertType(msg)) => {
        eprintln!("Type conversion error: {}", msg);
    }
    Err(ParseError::MissingValue(field)) => {
        eprintln!("Missing required field: {}", field);
    }
}
```

## Testing

```bash
# Run all tests (with default features)
cargo test

# Test with specific feature combinations
cargo test --no-default-features --features serde_json
cargo test --no-default-features --features database
cargo test --no-default-features --features rusqlite
cargo test --no-default-features --features tantivy

# Test with all features
cargo test --all-features
```

## See Also

- [`switchy_database`](../database/README.md) - Database abstraction layer
- [`moosicbox_search`](../search/README.md) - Search engine functionality
