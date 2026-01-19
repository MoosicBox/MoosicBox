# MoosicBox Search

A full-text search implementation for the MoosicBox music ecosystem. Built on Tantivy, this package provides indexed search across artists, albums, and tracks in your music library.

## Features

- **Full-Text Search**: Fast text search across artists, albums, tracks, and metadata
- **Tantivy Integration**: Built on Rust's high-performance search library
- **Real-Time Indexing**: Add, update, and delete documents from the index
- **Fuzzy Matching**: Find results even with typos or partial matches
- **Ranking & Scoring**: Relevance-based result ordering with custom boost logic
- **Multi-Field Search**: Search across artist, album, and track fields simultaneously
- **Async Operations**: Non-blocking search operations with Tokio
- **API Integration**: RESTful API endpoints for web applications (with `api` feature)
- **Index Management**: Efficient index building, updating, and optimization

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_search = "0.1.4"
```

## Usage

### Basic Search

```rust,no_run
use moosicbox_search::{global_search, search_global_search_index};

// High-level search (returns structured results)
let results = global_search("Pink Floyd", Some(0), Some(10))?;

// Process results
for result in results.results {
    println!("Found: {:?}", result);
}

// Lower-level search (returns raw Tantivy documents)
let documents = search_global_search_index("Pink Floyd", 0, 10)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Populating the Index

```rust,no_run
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
use moosicbox_search::{populate_global_search_index, DataValue};

// Create data entries for indexing
let data = vec![
    vec![
        ("document_type", DataValue::String("tracks".into())),
        ("artist_title", DataValue::String("Queen".into())),
        ("artist_id", DataValue::String("123".into())),
        ("album_title", DataValue::String("A Night at the Opera".into())),
        ("album_id", DataValue::String("456".into())),
        ("track_title", DataValue::String("Bohemian Rhapsody".into())),
        ("track_id", DataValue::String("789".into())),
        ("cover", DataValue::String("cover.jpg".into())),
        ("blur", DataValue::Bool(false)),
    ],
];

// Populate index (delete=true clears existing data first)
populate_global_search_index(&data, true).await?;
# Ok(())
# }
```

### Deleting from Index

```rust,no_run
use moosicbox_search::{delete_from_global_search_index, DataValue};

// Delete by track ID
let delete_terms = vec![
    ("track_id_string", DataValue::String("789".into()))
];

delete_from_global_search_index(&delete_terms)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Reindexing

```rust,no_run
# async fn example() -> Result<(), Box<dyn std::error::Error>> {
use moosicbox_search::reindex_global_search_index;

// Rebuild the entire index with fresh data
let fresh_data = vec![/* ... */];
reindex_global_search_index(&fresh_data).await?;
# Ok(())
# }
```

### Working with Database Models (requires `db` feature)

```rust,ignore
use moosicbox_search::data::{AsDataValues, AsDeleteTerm, recreate_global_search_index};
use moosicbox_music_models::{Artist, Album, Track};

// Convert database models to search data
let artist = Artist { /* ... */ };
let data_values = artist.as_data_values();

// Get delete term for a model
let delete_term = artist.as_delete_term();

// Recreate index from scratch
recreate_global_search_index().await?;
```

## Programming Interface

### Core Functions

```rust,ignore
// Search the global index
pub fn search_global_search_index(
    search: &str,
    offset: u32,
    limit: u32,
) -> Result<Vec<NamedFieldDocument>, SearchIndexError>

// High-level search with structured results
pub fn global_search(
    query: &str,
    offset: Option<u32>,
    limit: Option<u32>,
) -> Result<ApiSearchResultsResponse, SearchIndexError>

// Populate the index with data
pub async fn populate_global_search_index(
    data: &[Vec<(&str, DataValue)>],
    delete: bool,
) -> Result<(), PopulateIndexError>

// Delete documents from the index
pub fn delete_from_global_search_index(
    data: &[(&str, DataValue)],
) -> Result<(), DeleteFromIndexError>

// Rebuild the entire index
pub async fn reindex_global_search_index(
    data: &[Vec<(&str, DataValue)>],
) -> Result<(), ReindexError>
```

### Data Types

```rust
#[derive(Debug, Clone)]
pub enum DataValue {
    String(String),
    Bool(bool),
    Number(u64),
}
```

### Error Types

```rust,ignore
pub enum SearchIndexError { /* ... */ }
pub enum PopulateIndexError { /* ... */ }
pub enum DeleteFromIndexError { /* ... */ }
pub enum ReindexError { /* ... */ }
```

## Index Schema

The global search index includes the following fields:

- `document_type` - Type of document: "artists", "albums", or "tracks"
- `artist_title`, `artist_id` - Artist information
- `album_title`, `album_id` - Album information
- `track_title`, `track_id` - Track information
- `cover` - Cover art path
- `blur` - Whether cover art should be blurred
- `date_released`, `date_added` - Date fields
- `version_formats`, `version_bit_depths`, `version_sample_rates`, `version_channels`, `version_sources` - Audio format metadata

Each field has variants for different search types (e.g., `artist_title_search` for tokenized search, `artist_title_string` for exact matching).

## Web API Endpoints

When the `api` feature is enabled:

```text
GET /global-search?query={query}&offset={offset}&limit={limit}
GET /raw-global-search?query={query}&offset={offset}&limit={limit}
```

### API Usage Examples

```bash
# Structured search results
curl "http://localhost:8000/global-search?query=pink%20floyd&limit=10"

# Raw Tantivy document results
curl "http://localhost:8000/raw-global-search?query=rock&offset=0&limit=20"
```

## Search Algorithm

The search implementation uses multiple query strategies with different boost factors:

1. **Exact match**: Highest boost for exact phrase matches
2. **Prefix match**: Medium boost for prefix matching with fuzzy search
3. **Fuzzy match**: Base boost for fuzzy matching with typo tolerance

Search queries are sanitized to remove special characters and searches are performed across multiple field combinations optimized for artists, albums, and tracks.

## Features

- `default`: Enables `api`, `db`, and `openapi` features
- `api`: Enables Actix-web REST API endpoints
- `db`: Enables database integration and model conversion traits
- `openapi`: Enables OpenAPI documentation support
- `simulator`: Enables filesystem simulation for testing

## Testing

```bash
# Run all tests
cargo test

# Run with specific features
cargo test --features "api,db"
```

## Error Handling

```rust,no_run
use moosicbox_search::{global_search, SearchIndexError};

match global_search("query", Some(0), Some(10)) {
    Ok(results) => {
        println!("Found {} results", results.results.len());
    }
    Err(SearchIndexError::GetGlobalSearchIndex(e)) => {
        eprintln!("Index error: {}", e);
    }
    Err(SearchIndexError::QueryParser(e)) => {
        eprintln!("Invalid query: {}", e);
    }
    Err(e) => eprintln!("Search error: {}", e),
}
```

## Implementation Notes

- The search index is stored at `{config_dir}/search_indices/global_search_index` by default
- In test mode, uses an in-memory index
- Index writer uses a 50MB memory budget by default
- Search operations use query sanitization to handle special characters
- The implementation includes custom boost logic to prioritize exact matches and specific document types

## See Also

- [`moosicbox_music_api_models`](../music_api/models/README.md) - Search API models
- [`moosicbox_music_models`](../music/models/README.md) - Music data models
- [`moosicbox_library`](../library/README.md) - Music library management
- [`moosicbox_scan`](../scan/README.md) - Library scanning and indexing
