# MoosicBox Search

A high-performance full-text search engine for the MoosicBox music ecosystem. Built on Tantivy, this package provides fast, indexed search across music libraries with support for complex queries, faceted search, and real-time indexing.

## Features

- **Full-Text Search**: Fast text search across artists, albums, tracks, and metadata
- **Tantivy Integration**: Built on Rust's high-performance search library
- **Real-Time Indexing**: Automatic index updates as music library changes
- **Faceted Search**: Filter results by genre, year, artist, album, and more
- **Fuzzy Matching**: Find results even with typos or partial matches
- **Ranking & Scoring**: Relevance-based result ordering with customizable scoring
- **Multi-Field Search**: Search across multiple metadata fields simultaneously
- **Async Operations**: Non-blocking search operations with Tokio
- **API Integration**: RESTful API endpoints for web applications
- **Index Management**: Efficient index building, updating, and optimization

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_search = "0.1.1"
```

## Usage

### Basic Search

```rust
use moosicbox_search::{SearchEngine, SearchQuery, SearchResults};

// Initialize search engine
let search_engine = SearchEngine::new("/path/to/index")?;

// Perform simple text search
let query = SearchQuery::new("Pink Floyd");
let results = search_engine.search(&query).await?;

// Process results
for hit in results.hits {
    println!("Found: {} - {} (score: {})",
             hit.artist, hit.title, hit.score);
}
```

### Advanced Search Queries

```rust
use moosicbox_search::{SearchQuery, SearchFilter, SortBy};

// Complex search with filters and sorting
let query = SearchQuery::new("rock")
    .with_filter(SearchFilter::Genre("progressive rock".to_string()))
    .with_filter(SearchFilter::YearRange(1970, 1980))
    .with_sort(SortBy::Relevance)
    .with_limit(50)
    .with_offset(0);

let results = search_engine.search(&query).await?;
```

### Faceted Search

```rust
use moosicbox_search::{FacetQuery, FacetResults};

// Get facet counts for filtering
let facet_query = FacetQuery::new()
    .with_field("genre")
    .with_field("year")
    .with_field("artist");

let facets = search_engine.get_facets(&facet_query).await?;

// Display facet options
for (genre, count) in facets.genres {
    println!("{}: {} tracks", genre, count);
}
```

### Real-Time Indexing

```rust
use moosicbox_search::{IndexWriter, Document};

// Add new documents to index
let mut writer = search_engine.get_writer()?;

let doc = Document::new()
    .with_field("title", "Bohemian Rhapsody")
    .with_field("artist", "Queen")
    .with_field("album", "A Night at the Opera")
    .with_field("year", 1975)
    .with_field("genre", "rock");

writer.add_document(doc)?;
writer.commit()?;
```

### Search with Highlighting

```rust
use moosicbox_search::{SearchQuery, HighlightOptions};

let query = SearchQuery::new("bohemian")
    .with_highlight(HighlightOptions::new()
        .with_pre_tag("<mark>")
        .with_post_tag("</mark>")
        .with_max_fragments(3));

let results = search_engine.search(&query).await?;

for hit in results.hits {
    if let Some(highlights) = hit.highlights {
        println!("Title: {}", highlights.title.unwrap_or_default());
    }
}
```

## Programming Interface

### Core Types

```rust
pub struct SearchEngine {
    index: Index,
    reader: IndexReader,
    schema: Schema,
}

impl SearchEngine {
    pub fn new<P: AsRef<Path>>(index_path: P) -> Result<Self, SearchError>;
    pub async fn search(&self, query: &SearchQuery) -> Result<SearchResults, SearchError>;
    pub async fn get_facets(&self, query: &FacetQuery) -> Result<FacetResults, SearchError>;
    pub fn get_writer(&self) -> Result<IndexWriter, SearchError>;
    pub async fn rebuild_index(&self) -> Result<(), SearchError>;
}

#[derive(Debug, Clone)]
pub struct SearchQuery {
    pub text: String,
    pub filters: Vec<SearchFilter>,
    pub sort: Option<SortBy>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub highlight: Option<HighlightOptions>,
}

#[derive(Debug, Clone)]
pub struct SearchResults {
    pub hits: Vec<SearchHit>,
    pub total_count: usize,
    pub query_time_ms: u64,
    pub facets: Option<FacetResults>,
}
```

### Search Filters

```rust
#[derive(Debug, Clone)]
pub enum SearchFilter {
    Artist(String),
    Album(String),
    Genre(String),
    Year(i32),
    YearRange(i32, i32),
    Duration(std::time::Duration),
    DurationRange(std::time::Duration, std::time::Duration),
    HasLyrics(bool),
    Rating(f32),
    RatingRange(f32, f32),
}

#[derive(Debug, Clone)]
pub enum SortBy {
    Relevance,
    Title,
    Artist,
    Album,
    Year,
    Duration,
    Rating,
    DateAdded,
}
```

### Document Management

```rust
#[derive(Debug, Clone)]
pub struct Document {
    fields: HashMap<String, FieldValue>,
}

impl Document {
    pub fn new() -> Self;
    pub fn with_field<T: Into<FieldValue>>(mut self, name: &str, value: T) -> Self;
    pub fn get_field(&self, name: &str) -> Option<&FieldValue>;
}

#[derive(Debug, Clone)]
pub enum FieldValue {
    Text(String),
    Integer(i64),
    Float(f64),
    Date(DateTime<Utc>),
    Boolean(bool),
}
```

## Configuration

### Environment Variables

- `SEARCH_INDEX_PATH`: Path to search index directory (default: `./search_index`)
- `SEARCH_INDEX_MEMORY_MB`: Memory budget for indexing in MB (default: 128)
- `SEARCH_WRITER_THREADS`: Number of indexing threads (default: 4)
- `SEARCH_COMMIT_INTERVAL_SEC`: Auto-commit interval in seconds (default: 30)

### Index Schema Configuration

```rust
use moosicbox_search::{SchemaBuilder, FieldType};

let schema = SchemaBuilder::new()
    .add_text_field("title", FieldType::Text { stored: true, indexed: true })
    .add_text_field("artist", FieldType::Text { stored: true, indexed: true })
    .add_text_field("album", FieldType::Text { stored: true, indexed: true })
    .add_facet_field("genre", FieldType::Facet)
    .add_integer_field("year", FieldType::Integer { stored: true, indexed: true })
    .add_float_field("duration", FieldType::Float { stored: true, indexed: true })
    .build()?;
```

## Web API Endpoints

When the `api` feature is enabled:

```
GET    /search?q={query}&limit={limit}&offset={offset}
GET    /search/facets?fields={fields}
POST   /search/advanced
GET    /search/suggest?q={query}
POST   /index/rebuild
GET    /index/stats
```

### API Usage Examples

```bash
# Simple search
curl "http://localhost:8000/search?q=pink%20floyd&limit=10"

# Advanced search with filters
curl -X POST http://localhost:8000/search/advanced \
  -H "Content-Type: application/json" \
  -d '{
    "text": "rock",
    "filters": [
      {"Genre": "progressive rock"},
      {"YearRange": [1970, 1980]}
    ],
    "sort": "Relevance",
    "limit": 20
  }'

# Get search facets
curl "http://localhost:8000/search/facets?fields=genre,artist,year"
```

## Performance Optimization

### Index Tuning

```rust
use moosicbox_search::{IndexSettings, CompressionType};

let settings = IndexSettings::new()
    .with_memory_budget_mb(256)
    .with_compression(CompressionType::Lz4)
    .with_merge_policy_max_segments(10)
    .with_commit_interval_sec(60);

let search_engine = SearchEngine::with_settings("/path/to/index", settings)?;
```

### Query Optimization

```rust
// Use specific field searches for better performance
let query = SearchQuery::new("")
    .with_field_query("artist", "Pink Floyd")  // Faster than full-text
    .with_field_query("album", "Dark Side");

// Limit result size for pagination
let query = SearchQuery::new("rock")
    .with_limit(20)  // Don't fetch more than needed
    .with_offset(0);

// Use filters instead of text search when possible
let query = SearchQuery::new("")
    .with_filter(SearchFilter::Genre("rock".to_string()))  // Faster
    .with_filter(SearchFilter::YearRange(1970, 1980));
```

## Index Management

### Building Initial Index

```rust
use moosicbox_search::{IndexBuilder, ProgressCallback};

let builder = IndexBuilder::new("/path/to/index");

// Build from music library
let progress = |indexed: usize, total: usize| {
    println!("Indexed {} of {} tracks", indexed, total);
};

builder.build_from_library("/path/to/music", Some(progress)).await?;
```

### Index Maintenance

```rust
// Optimize index (merge segments)
search_engine.optimize().await?;

// Get index statistics
let stats = search_engine.get_stats().await?;
println!("Index size: {} MB", stats.size_mb);
println!("Document count: {}", stats.document_count);
println!("Segments: {}", stats.segment_count);

// Rebuild index from scratch
search_engine.rebuild_index().await?;
```

## Testing

```bash
# Run all tests
cargo test

# Run with specific features
cargo test --features "api,db"

# Run performance benchmarks
cargo bench

# Test with sample data
cargo test --test integration -- --ignored
```

## Error Handling

```rust
use moosicbox_search::SearchError;

match search_engine.search(&query).await {
    Ok(results) => {
        println!("Found {} results", results.total_count);
    }
    Err(SearchError::IndexNotFound) => {
        eprintln!("Search index not found. Run index rebuild.");
    }
    Err(SearchError::QueryParseError(msg)) => {
        eprintln!("Invalid query: {}", msg);
    }
    Err(SearchError::IndexCorrupted) => {
        eprintln!("Index corrupted. Rebuilding required.");
        search_engine.rebuild_index().await?;
    }
    Err(e) => eprintln!("Search error: {}", e),
}
```

## Troubleshooting

### Common Issues

**Index Not Found**
- Ensure index directory exists and is readable
- Run initial index build if this is first use
- Check file permissions on index directory

**Poor Search Performance**
- Increase memory budget for indexing
- Optimize index to reduce segment count
- Use more specific search queries
- Consider index warming strategies

**Out of Memory During Indexing**
- Reduce memory budget in settings
- Process library in smaller batches
- Increase system swap space
- Use streaming indexing for large libraries

**Index Corruption**
- Enable regular index backups
- Use atomic commits
- Check disk space and file system health
- Rebuild index from source data

## See Also

- [`moosicbox_music_api`](../music_api/README.md) - Music API abstractions
- [`moosicbox_library`](../library/README.md) - Music library management
- [`moosicbox_scan`](../scan/README.md) - Library scanning and indexing
- [`moosicbox_database`](../database/README.md) - Database operations
- [`moosicbox_config`](../config/README.md) - Configuration management
