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
moosicbox_json_utils = "0.1.1"
```

## Usage

### Basic Type Conversion

```rust
use moosicbox_json_utils::{ToValueType, ParseError};
use serde_json::Value;

fn main() -> Result<(), ParseError> {
    // Convert JSON value to specific type
    let json_value = serde_json::json!("42");
    let number: i32 = json_value.to_value_type()?;
    assert_eq!(number, 42);

    // Convert JSON string to boolean
    let json_bool = serde_json::json!("true");
    let boolean: bool = json_bool.to_value_type()?;
    assert_eq!(boolean, true);

    // Convert JSON array to vector
    let json_array = serde_json::json!([1, 2, 3, 4, 5]);
    let numbers: Vec<i32> = json_array.to_value_type()?;
    assert_eq!(numbers, vec![1, 2, 3, 4, 5]);

    Ok(())
}
```

### Nested Value Access

```rust
use moosicbox_json_utils::serde_json::{ToNestedValue, get_nested_value};

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
use moosicbox_json_utils::ToValueType;

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

pub trait JsonValidator {
    fn validate(&self, value: &serde_json::Value) -> Result<(), ParseError>;
}

pub trait JsonSanitizer {
    fn sanitize(&self, value: serde_json::Value) -> serde_json::Value;
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
    use switchy_database::{DatabaseValue, DatabaseError};

    pub trait DatabaseJsonExt {
        fn from_json(value: &serde_json::Value) -> Result<DatabaseValue, DatabaseError>;
        fn to_json(&self) -> Result<serde_json::Value, DatabaseError>;
    }

    impl DatabaseJsonExt for DatabaseValue {
        fn from_json(value: &serde_json::Value) -> Result<DatabaseValue, DatabaseError>;
        fn to_json(&self) -> Result<serde_json::Value, DatabaseError>;
    }
}
```

### Tantivy Integration

```rust
#[cfg(feature = "tantivy")]
pub mod tantivy {
    use tantivy::{Document, schema::Field};

    pub trait TantivyJsonExt {
        fn add_json_object(&mut self, field: Field, value: serde_json::Value);
        fn get_json_object(&self, field: Field) -> Option<&serde_json::Value>;
    }

    impl TantivyJsonExt for Document {
        fn add_json_object(&mut self, field: Field, value: serde_json::Value);
        fn get_json_object(&self, field: Field) -> Option<&serde_json::Value>;
    }
}
```

### SQLite Integration

```rust
#[cfg(feature = "rusqlite")]
pub mod rusqlite {
    use rusqlite::{types::Value, Result};

    pub trait SqliteJsonExt {
        fn from_json_value(value: &serde_json::Value) -> Value;
        fn to_json_value(&self) -> Result<serde_json::Value>;
    }

    impl SqliteJsonExt for Value {
        fn from_json_value(value: &serde_json::Value) -> Value;
        fn to_json_value(&self) -> Result<serde_json::Value>;
    }
}
```

## Configuration

### Feature Flags

- `database`: Enable database integration utilities
- `rusqlite`: Enable SQLite-specific JSON functions
- `tantivy`: Enable Tantivy search engine integration
- `serde_json`: Enable enhanced serde_json functionality

### Environment Variables

- `JSON_UTILS_VALIDATION_STRICT`: Enable strict JSON validation (default: false)
- `JSON_UTILS_SANITIZE_INPUT`: Automatically sanitize JSON input (default: true)
- `JSON_UTILS_MAX_DEPTH`: Maximum JSON nesting depth (default: 64)

## Integration Examples

### Music Library JSON Processing

```rust
use moosicbox_json_utils::{ToValueType, database::DatabaseJsonExt};
use switchy_database::Database;

async fn process_music_library() -> Result<(), Box<dyn std::error::Error>> {
    let db = get_database_connection().await?;

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

    // Store in database
    let db_value = DatabaseValue::from_json(&album_json)?;
    db.execute(
        "INSERT INTO albums (metadata) VALUES (?)",
        &[db_value],
    ).await?;

    // Query and process results
    let rows = db.query(
        "SELECT metadata FROM albums WHERE json_extract(metadata, '$.release_year') > 1970",
        &[],
    ).await?;

    for row in rows {
        let metadata_value = &row[0];
        let album_metadata: serde_json::Value = metadata_value.to_json()?;

        let title: String = album_metadata["title"].to_value_type()?;
        let artist: String = album_metadata["artist"].to_value_type()?;
        let tracks: Vec<serde_json::Value> = album_metadata["tracks"].to_value_type()?;

        println!("Album: {} by {}", title, artist);
        println!("Tracks: {}", tracks.len());
    }

    Ok(())
}
```

### Search Index JSON Processing

```rust
use moosicbox_json_utils::tantivy::TantivyJsonExt;
use tantivy::{Index, IndexWriter};

fn build_music_search_index() -> Result<(), Box<dyn std::error::Error>> {
    // Create schema
    let mut schema_builder = tantivy::schema::Schema::builder();
    let metadata_field = schema_builder.add_json_field("metadata", tantivy::schema::STORED);
    let schema = schema_builder.build();

    let index = Index::create_in_ram(schema.clone());
    let mut index_writer = index.writer(50_000_000)?;

    // Index music metadata
    let music_items = load_music_metadata()?;

    for item in music_items {
        let mut doc = tantivy::Document::new();
        doc.add_json_object(metadata_field, item);
        index_writer.add_document(doc)?;
    }

    index_writer.commit()?;

    Ok(())
}

fn load_music_metadata() -> Result<Vec<serde_json::Value>, Box<dyn std::error::Error>> {
    // Load music metadata from various sources
    Ok(vec![
        serde_json::json!({
            "type": "track",
            "title": "Bohemian Rhapsody",
            "artist": "Queen"
        }),
        serde_json::json!({
            "type": "album",
            "title": "Abbey Road",
            "artist": "The Beatles"
        }),
    ])
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
# Run all tests
cargo test

# Test with database features
cargo test --features database

# Test with Tantivy features
cargo test --features tantivy

# Test SQLite integration
cargo test --features rusqlite
```

## See Also

- [`switchy_database`](../database/README.md) - Database abstraction layer
- [`moosicbox_search`](../search/README.md) - Search engine functionality
- [`moosicbox_music_models`](../music_models/README.md) - Music data models
