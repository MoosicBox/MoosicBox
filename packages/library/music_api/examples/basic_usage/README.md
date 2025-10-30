# Basic Usage Example

This example demonstrates the fundamental usage patterns of the `LibraryMusicApi` implementation.

## Summary

Learn how to create and use a `LibraryMusicApi` instance to access local music library content through the `MusicApi` trait interface.

## What This Example Demonstrates

- Creating a `LibraryMusicApi` instance with a library database
- Querying favorite artists with pagination and ordering
- Retrieving favorite albums with filtering options
- Performing full-text search across library content
- Checking scan support and scan status
- Managing favorites (adding and removing artists, albums, tracks)
- Understanding the `MusicApi` trait implementation patterns

## Prerequisites

- Basic understanding of async/await in Rust
- Familiarity with the `MusicApi` trait from `moosicbox_music_api`
- Understanding of pagination concepts

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/library/music_api/examples/basic_usage/Cargo.toml
```

Or from the example directory:

```bash
cd packages/library/music_api/examples/basic_usage
cargo run
```

## Expected Output

The example will output:

```
LibraryMusicApi Basic Usage Example
====================================

Step 1: Setting up in-memory library database...
✓ Database created

Step 2: Creating LibraryMusicApi instance...
✓ API instance created
  API Source: Library

Step 3: Fetching favorite artists...
✓ Artists retrieved
  Total artists: 0
  Artists in this page: 0

Step 4: Fetching favorite albums...
✓ Albums retrieved
  Total albums: 0
  Albums in this page: 0

Step 5: Testing search functionality...
✓ Search completed for query: 'example'
  Artists found: 0
  Albums found: 0
  Tracks found: 0

Step 6: Checking library scan support...
✓ Library scanning is supported
  Scan enabled: false

Step 7: Managing favorites...
  Attempting to add artist to favorites (ID: 999999)...
  Note: [error message]
  Attempting to remove artist from favorites...
  Note: [error message]

Example completed successfully!
```

Note: The in-memory database starts empty, so queries return no results. In a real application with a populated library database, you would see actual artists, albums, and tracks.

## Code Walkthrough

### Step 1: Database Setup

```rust
let db = Database::new_in_memory()
    .await
    .map_err(|e| format!("Failed to create database: {e}"))?;
let library_db = LibraryDatabase::from(db);
```

Create an in-memory database for demonstration. In production, you would connect to an existing library database with populated music data.

### Step 2: API Initialization

```rust
let api = LibraryMusicApi::new(library_db);
```

Create a `LibraryMusicApi` instance by providing a `LibraryDatabase`. The API automatically implements the `MusicApi` trait, providing a unified interface for music operations.

### Step 3: Querying Artists

```rust
let artists_result = api
    .artists(
        Some(0),  // offset
        Some(10), // limit
        None,     // order
        None,     // order_direction
    )
    .await?;
```

Retrieve paginated favorite artists. The API returns a `PagingResult` with total count and items, enabling efficient pagination through large collections.

### Step 4: Querying Albums

```rust
let albums_request = AlbumsRequest {
    sources: None,
    sort: None,
    name: None,
    artist: None,
    search: None,
    album_type: None,
    offset: Some(0),
    limit: Some(10),
};

let albums_result = api.albums(&albums_request).await?;
```

Retrieve albums using an `AlbumsRequest` structure that supports various filtering options including source filtering, sorting, name/artist filtering, and album type filtering.

### Step 5: Search Functionality

```rust
if api.supports_search() {
    let search_results = api.search("example", Some(0), Some(5)).await?;
}
```

The API supports full-text search across artists, albums, and tracks. Always check `supports_search()` before calling search functionality.

### Step 6: Scan Support

```rust
if api.supports_scan() {
    let enabled = api.scan_enabled().await?;
}
```

LibraryMusicApi supports library scanning for indexing local music files. Check scan support and status before initiating scans.

### Step 7: Managing Favorites

```rust
api.add_artist(&artist_id).await?;
api.remove_artist(&artist_id).await?;
```

Add or remove artists, albums, and tracks from favorites. Similar methods exist for albums (`add_album`, `remove_album`) and tracks (`add_track`, `remove_track`).

## Key Concepts

### MusicApi Trait

The `LibraryMusicApi` implements the `MusicApi` trait, which provides a unified interface for accessing music content regardless of the source (local library, streaming service, etc.). This abstraction allows applications to work with multiple music sources using the same API.

### Pagination

All list operations return `PagingResult` objects that support:

- **Offset and limit**: Control which page of results to retrieve
- **Total count**: Know the total number of items available
- **Fetch callback**: Automatically fetch additional pages as needed

### Local Library Source

The library API operates on local music files indexed in a database. It provides:

- Direct file system access for playback
- Full-text search across metadata
- Album version support (different quality levels)
- Integration with library scanning

### Favorites Management

The API maintains separate favorite collections for:

- **Artists**: Favorite artists you want to track
- **Albums**: Favorite albums for quick access
- **Tracks**: Individual favorite tracks

## Testing the Example

Since this example uses an empty in-memory database, all queries return empty results. To see the API in action with real data:

1. **Use a populated database**: Replace the in-memory database with a connection to your actual library database
2. **Run a scan**: Enable scanning and run `api.scan().await?` to index local music files
3. **Add test data**: Manually insert test data into the database for demonstration
4. **Query specific items**: Use known IDs from your library to test individual retrieval methods like `api.artist(&id)`, `api.album(&id)`, `api.track(&id)`

### Example with test data queries:

```rust
// Query a specific artist by ID
if let Some(artist) = api.artist(&artist_id).await? {
    println!("Artist: {} ({})", artist.name, artist.id);
}

// Get albums for an artist
let artist_albums = api.artist_albums(
    &artist_id,
    None,    // album_type
    Some(0), // offset
    Some(20),// limit
    None,    // order
    None,    // order_direction
).await?;

// Get tracks from an album
let album_tracks = api.album_tracks(
    &album_id,
    Some(0), // offset
    Some(50),// limit
    None,    // order
    None,    // order_direction
).await?;
```

## Troubleshooting

### "Database connection error"

Ensure the database path is valid and the database is accessible. For in-memory databases, this shouldn't occur unless there's a system resource issue.

### "Search not supported"

Always check `api.supports_search()` before calling search. The LibraryMusicApi does support search, but check the feature flags to ensure search functionality is compiled.

### "Empty results"

The in-memory database starts empty. To see real results:

- Connect to an existing populated library database
- Run a library scan to index your local music files
- Insert test data for demonstration

### "Scan not enabled"

Call `api.enable_scan().await?` to enable scanning before attempting to scan the library.

## Related Examples

This is currently the only example for `moosicbox_library_music_api`. For related concepts, see:

- Database examples in `packages/database/examples/` for database interaction patterns
- Web server examples in `packages/web_server/examples/` for API endpoint patterns
- Music API trait definition in `packages/music_api/src/lib.rs`
