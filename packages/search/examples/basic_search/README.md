# Basic Search Example

A comprehensive example demonstrating the core functionality of the moosicbox_search package, including indexing, searching, updating, and deleting music data.

## Summary

This example shows how to use moosicbox_search to build a searchable music library. It demonstrates creating a search index, populating it with artists, albums, and tracks, performing various types of searches (exact, partial, fuzzy), updating the index with new data, and removing items from the index.

## What This Example Demonstrates

- Creating and populating a Tantivy-based search index with music metadata
- Performing full-text searches across multiple fields (artists, albums, tracks)
- Fuzzy matching that handles typos and partial queries
- Adding new documents to an existing index
- Deleting specific documents from the index
- Pagination for search results
- Using the `simulator` feature for in-memory indexing (no disk I/O required)

## Prerequisites

- Basic understanding of Rust and async/await
- Familiarity with music metadata concepts (artists, albums, tracks)
- Understanding of search concepts (indexing, querying, fuzzy matching)

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/search/examples/basic_search/Cargo.toml
```

Or from the example directory:

```bash
cd packages/search/examples/basic_search
cargo run
```

To see detailed logging output:

```bash
RUST_LOG=debug cargo run --manifest-path packages/search/examples/basic_search/Cargo.toml
```

## Expected Output

The example will display:

```
=== MoosicBox Search - Basic Example ===

Step 1: Preparing sample music data...
  - Created 5 documents

Step 2: Populating the search index...
  - Index populated successfully

Step 3: Performing searches...

Search 1: Looking for 'Pink Floyd'
  - Found 5 results
    * Artist/Album/Track details...

Search 2: Looking for 'Dark Side'
  - Found 2 results
    * Album and track details...

Search 3: Looking for 'Wish You Were'
  - Found 2 results
    * Album and track details...

Search 4: Fuzzy search - 'Pnk Floid' (with typos)
  - Found 5 results (fuzzy matching!)
    * Same results as exact search...

Step 4: Adding new artist to the index...
  - Added Led Zeppelin to index

Search 5: Looking for 'Led Zeppelin'
  - Found 1 results
    * Artist details...

Step 5: Deleting a track from the index...
  - Deleted track with ID 1001

Search 6: Looking for 'Time' (should find fewer results)
  - Found 0 results

Step 7: Pagination - Getting results in batches
  Search for 'Floyd' with limit of 2:
    Page 1: 2 results
    Page 2: 2 results

=== Example Complete ===

Key Takeaways:
  ✓ Created and populated a search index with music data
  ✓ Performed exact, partial, and fuzzy searches
  ✓ Added new data to an existing index
  ✓ Deleted items from the index
  ✓ Demonstrated pagination
```

## Code Walkthrough

### 1. Setting Up the Example

```rust
use moosicbox_search::{
    delete_from_global_search_index, global_search, populate_global_search_index, DataValue,
};
```

The example imports the main search functions: `populate_global_search_index` for indexing, `global_search` for querying, and `delete_from_global_search_index` for removing documents.

### 2. Creating Sample Data

```rust
fn create_sample_music_data() -> Vec<Vec<(&'static str, DataValue)>> {
    vec![
        vec![
            ("document_type", DataValue::String("artists".into())),
            ("artist_title", DataValue::String("Pink Floyd".into())),
            ("artist_id", DataValue::String("100".into())),
            // ... more fields
        ],
        // ... more documents
    ]
}
```

Each document is represented as a vector of field-value pairs. The `DataValue` enum supports strings, booleans, and numbers. Required fields include:

- `document_type`: "artists", "albums", or "tracks"
- `artist_title`, `artist_id`: Artist information
- `album_title`, `album_id`: Album information (for albums/tracks)
- `track_title`, `track_id`: Track information (for tracks only)
- Metadata fields: `cover`, `blur`, dates, version info

### 3. Populating the Index

```rust
// Delete existing data and populate with new data
populate_global_search_index(&music_data, true).await?;

// Append to existing index without deleting
populate_global_search_index(&new_data, false).await?;
```

The second parameter controls whether to clear the index first. Use `true` for initial population or full reindexing, `false` to add documents incrementally.

### 4. Performing Searches

```rust
// Basic search with pagination
let results = global_search("Pink Floyd", Some(0), Some(10))?;

// The function returns structured results
for result in results.results {
    println!("{:?}", result);
}
```

The `global_search` function provides:

- Full-text search across all indexed fields
- Automatic fuzzy matching for typos
- Relevance-based ranking
- Pagination via offset and limit parameters
- Deduplication of results

### 5. Updating the Index

```rust
// Add a new artist
let new_data = vec![vec![
    ("document_type", DataValue::String("artists".into())),
    ("artist_title", DataValue::String("Led Zeppelin".into())),
    // ... other fields
]];

populate_global_search_index(&new_data, false).await?;
```

New documents are added to the index without rebuilding it from scratch.

### 6. Deleting from the Index

```rust
// Delete by track ID (using the _string variant field)
let delete_terms = vec![
    ("track_id_string", DataValue::String("1001".into()))
];

delete_from_global_search_index(&delete_terms)?;
```

Documents are deleted by matching specific field values. Use the `_string` variant fields (e.g., `artist_id_string`, `album_id_string`, `track_id_string`) for exact matching.

## Key Concepts

### Search Index Schema

The search index uses a predefined schema with multiple field variants:

- **Base fields**: Store the actual data (e.g., `artist_title`)
- **Search fields**: Tokenized for full-text search (e.g., `artist_title_search`)
- **String fields**: For exact matching (e.g., `artist_title_string`)

### Fuzzy Matching

The search engine automatically handles:

- Typos and spelling variations
- Partial word matches
- Prefix matching
- Multiple query strategies with different boost factors

### Relevance Ranking

Results are scored and ranked based on:

- Exact phrase matches (highest boost)
- Prefix matches (medium boost)
- Fuzzy matches (base boost)
- Document type specificity (artists, albums, tracks)

### In-Memory Indexing

This example uses the `simulator` feature, which creates the index in RAM rather than on disk. This is useful for:

- Examples and demonstrations
- Testing
- Temporary or disposable indexes
- Environments without persistent storage

For production use, omit the `simulator` feature to use disk-based indexing.

## Testing the Example

Run the example and verify:

1. ✓ All searches complete successfully
2. ✓ Fuzzy search finds results despite typos
3. ✓ Newly added artist is searchable
4. ✓ Deleted track no longer appears in results
5. ✓ Pagination returns correct number of results

## Troubleshooting

### Issue: "Index already exists" error

**Solution**: The example uses the `simulator` feature with in-memory indexes, so this shouldn't occur. If using disk-based indexing, ensure you have write permissions to the index directory.

### Issue: No search results returned

**Solution**: Verify that:

- Data was successfully populated (check for error messages)
- Query string matches indexed content
- Offset and limit parameters are reasonable

### Issue: Compilation errors

**Solution**: Ensure all workspace dependencies are available. Run from the repository root:

```bash
cargo check --manifest-path packages/search/examples/basic_search/Cargo.toml
```

## Related Examples

This is currently the only example for moosicbox_search. Future examples might include:

- Database integration example (using the `db` feature)
- API server example (using the `api` feature)
- Advanced query example (custom boost factors, field-specific searches)
- Reindexing and index management example
