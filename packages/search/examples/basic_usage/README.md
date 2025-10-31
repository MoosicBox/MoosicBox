# Basic Usage Example

This example demonstrates the core functionality of the `moosicbox_search` crate for indexing and searching music library data.

## Summary

This example shows how to create a search index, populate it with music data (artists, albums, tracks), perform full-text searches, and delete documents from the index. It demonstrates both the high-level API for structured results and the low-level API for raw Tantivy documents.

## What This Example Demonstrates

- Creating and populating a Tantivy-based search index with music data
- Structuring data using the `DataValue` enum (String, Bool, Number types)
- Performing full-text searches using `global_search()` (high-level API)
- Performing searches using `search_global_search_index()` (low-level API)
- Deleting specific documents from the index
- Proper error handling with Result types
- Working with search results in different formats (API types vs raw documents)

## Prerequisites

- Basic understanding of Rust async/await syntax
- Familiarity with full-text search concepts (indexing, querying)
- Understanding of music metadata (artists, albums, tracks)

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/search/examples/basic_usage/Cargo.toml
```

Or from the example directory:

```bash
cd packages/search/examples/basic_usage
cargo run
```

## Expected Output

When you run the example, you should see output similar to:

```
=== MoosicBox Search - Basic Usage Example ===

Step 1: Creating sample music data...
  Created 5 documents

Step 2: Populating search index...
  Index populated successfully

Step 3: Performing searches...

  Searching for 'Bohemian':
    1. Track: Queen - Bohemian Rhapsody (from A Night at the Opera)

  Searching for 'Queen':
    1. Artist: Queen
    2. Album: A Night at the Opera by Queen
    3. Track: Queen - Bohemian Rhapsody (from A Night at the Opera)

  Searching for 'opera':
    1. Album: A Night at the Opera by Queen
    2. Track: Queen - Bohemian Rhapsody (from A Night at the Opera)

Step 4: Using low-level search API...
  Searching for 'Pink Floyd':
  Found 2 raw documents
    Document 1: 11 fields
    Document 2: 15 fields

Step 5: Deleting a document from the index...
  Deleted track with ID 789

Step 6: Verifying deletion...
  Searching for 'Bohemian' again:
    No results found

=== Example completed successfully ===
```

## Code Walkthrough

### 1. Creating Sample Data

The `create_sample_data()` function demonstrates how to structure music data for indexing:

```rust
vec![
    ("document_type", DataValue::String("tracks".into())),
    ("artist_title", DataValue::String("Queen".into())),
    ("artist_id", DataValue::String("123".into())),
    ("album_title", DataValue::String("A Night at the Opera".into())),
    ("album_id", DataValue::String("456".into())),
    ("track_title", DataValue::String("Bohemian Rhapsody".into())),
    ("track_id", DataValue::String("789".into())),
    // ... additional fields
]
```

Each document is a vector of field-value tuples. The `DataValue` enum supports three types:

- `String`: Text fields like titles and IDs
- `Bool`: Boolean flags like the blur setting
- `Number`: Numeric values like bit depths and sample rates

### 2. Populating the Index

```rust
populate_global_search_index(&music_data, true).await?;
```

The second parameter (`delete: bool`) controls whether existing index data is cleared:

- `true`: Clears the index before adding new data (full reindex)
- `false`: Adds data to existing index (incremental update)

### 3. High-Level Search API

```rust
let results = global_search("Bohemian", Some(0), Some(10))?;
```

The `global_search()` function returns structured API results:

- First parameter: search query string
- Second parameter: offset for pagination (None uses default: 0)
- Third parameter: limit for results (None uses default: 10)

Results are returned as `ApiGlobalSearchResult` enums that can be:

- `Artist(artist)`: Artist search result
- `Album(album)`: Album search result
- `Track(track)`: Track search result

### 4. Low-Level Search API

```rust
let raw_docs = search_global_search_index("Pink Floyd", 0, 5)?;
```

This returns raw Tantivy `NamedFieldDocument` objects with all indexed fields. Use this when you need:

- Full access to all indexed fields
- Custom result processing
- Integration with Tantivy-specific features

### 5. Deleting Documents

```rust
let delete_terms = vec![
    ("track_id_string", DataValue::String("789".into()))
];
delete_from_global_search_index(&delete_terms)?;
```

Documents are deleted by matching field values. The field name must use the `_string` suffix variant for exact matching. Multiple documents can be deleted if they share the same field value.

## Key Concepts

### Index Schema

The global search index uses a predefined schema with fields for:

- **Document metadata**: `document_type`, IDs
- **Artist fields**: `artist_title`, `artist_id`
- **Album fields**: `album_title`, `album_id`
- **Track fields**: `track_title`, `track_id`
- **Media metadata**: `cover`, `blur`, dates, audio format details

Each field has multiple variants:

- Base field: Stored value (e.g., `artist_title`)
- `_search` suffix: Tokenized for full-text search (e.g., `artist_title_search`)
- `_string` suffix: Exact string matching (e.g., `artist_title_string`)

### Search Algorithm

The search implementation uses multiple query strategies with different boost factors:

1. **Exact match**: Highest boost for exact phrase matches
2. **Prefix match**: Medium boost for prefix matching with fuzzy search
3. **Fuzzy match**: Base boost for fuzzy matching with typo tolerance

This provides intelligent ranking that prioritizes exact matches while still finding results with typos or partial matches.

### Async Operations

The `populate_global_search_index()` and `reindex_global_search_index()` functions are async to avoid blocking the runtime during expensive indexing operations. Search operations are synchronous since they're typically fast.

## Testing the Example

To verify the example works correctly:

1. **Check compilation**: The example should compile without warnings
2. **Run the example**: Should complete without errors
3. **Verify output**: All search operations should return expected results
4. **Check deletion**: After deleting the track, searches should return no results

## Troubleshooting

### Index Directory Permissions

If you see errors about index directory access:

- The index is stored at `{config_dir}/search_indices/global_search_index`
- Ensure the application has write permissions to this directory

### Missing Results

If searches don't return expected results:

- Verify data was indexed correctly (check logs)
- Ensure field names match the schema exactly
- Remember that searches are fuzzy and case-insensitive

### Memory Usage

If you encounter memory issues with large indexes:

- The default memory budget is 50MB
- For production use, consider increasing this via the internal writer configuration
- Use incremental updates (`delete=false`) instead of full reindexing when possible

## Related Examples

This is currently the only example for `moosicbox_search`. For related functionality, see:

- Database integration examples would show the `AsDataValues` and `AsDeleteTerm` traits
- API integration examples would demonstrate the REST endpoints
