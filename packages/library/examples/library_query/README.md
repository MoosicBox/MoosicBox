# Library Query Example

A comprehensive example demonstrating how to use the `moosicbox_library` crate to query and manage music library data.

## Summary

This example shows how to interact with the MoosicBox music library to retrieve artists, albums, and tracks, with support for filtering, sorting, and pagination.

## What This Example Demonstrates

- Querying favorite artists with pagination
- Querying favorite albums with filtering and sorting options
- Retrieving specific artists, albums, and tracks by ID
- Getting albums for a specific artist
- Getting tracks for a specific album
- Using pagination to limit result sets
- Working with the `LibraryDatabase` abstraction
- Creating an in-memory test database with sample music data

## Prerequisites

- Basic understanding of Rust async/await programming
- Familiarity with the tokio runtime
- Understanding of database concepts (SQLite)
- Knowledge of music metadata structures (artists, albums, tracks)

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/library/examples/library_query/Cargo.toml
```

Or from the example directory:

```bash
cd packages/library/examples/library_query
cargo run
```

## Expected Output

When you run this example, you should see output similar to:

```
=== MoosicBox Library Query Example ===

Created in-memory database with sample music data

1. Querying favorite artists...
   Found 2 total artists
   - Artist: The Beatles (ID: 1)
   - Artist: Pink Floyd (ID: 2)

2. Querying favorite albums (sorted by name)...
   Found 2 total albums
   - Album: Abbey Road by The Beatles (ID: 1)
   - Album: The Dark Side of the Moon by Pink Floyd (ID: 2)

3. Fetching specific artist (ID: Number(1))...
   Artist: The Beatles
   Cover: None

4. Fetching albums for artist...
   Found 1 albums
   - Abbey Road

5. Fetching tracks for album 'Abbey Road'...
   Found 2 tracks
   - Track 1: Come Together
   - Track 2: Something

6. Querying favorite tracks (paginated)...
   Retrieved 3 of 3 tracks
   - 1: Come Together (259)
   - 2: Something (183)
   - 1: Speak to Me (68)

7. Fetching specific album (ID: Number(1))...
   Album: Abbey Road
   Artist: The Beatles
   Released: Some("1969-09-26")

=== Example completed successfully! ===
```

## Code Walkthrough

### Database Setup

The example first creates an in-memory SQLite database with sample music data:

```rust
let db = create_test_database().await?;
```

This helper function:

- Creates an in-memory SQLite database using `switchy_database`
- Defines schema for artists, albums, and tracks tables
- Inserts sample data (The Beatles and Pink Floyd albums)
- Returns a `LibraryDatabase` instance

### Querying Favorite Artists

To retrieve favorite artists with pagination:

```rust
let artists_response = favorite_artists(&db, None, Some(10), None, None).await?;
```

The parameters are:

- Database reference
- Offset (None = start from 0)
- Limit (Some(10) = max 10 results)
- Order type (None = default ordering)
- Order direction (None = default direction)

### Querying Albums with Filtering and Sorting

To retrieve albums with specific sorting:

```rust
let albums_request = AlbumsRequest {
    sources: None,           // All sources
    sort: Some(AlbumSort::NameAsc),  // Sort by name ascending
    filters: None,           // No filters
    page: Some(PagingRequest {
        offset: 0,
        limit: 10,
    }),
};
let albums_response = favorite_albums(&db, &albums_request).await?;
```

The `AlbumsRequest` structure allows you to:

- Filter by source (local, Tidal, Qobuz, etc.)
- Apply custom sorting (by name, artist, release date, date added)
- Filter by artist, album type, search terms
- Configure pagination

### Retrieving Individual Items

To get a specific artist by ID:

```rust
let artist_id = Id::Number(1);
let artist_info = artist(&db, &artist_id).await?;
```

Similarly for albums and tracks:

```rust
let album_info = album(&db, &album_id).await?;
let track_info = track(&db, &track_id).await?;
```

### Querying Related Items

To get all albums for an artist:

```rust
let artist_albums_response = artist_albums(
    &db,
    &artist_id,
    None,        // offset
    Some(5),     // limit to 5 albums
    None         // album type filter
).await?;
```

To get all tracks for an album:

```rust
let tracks_response = album_tracks(
    &db,
    &album_id,
    None,    // offset
    None     // limit
).await?;
```

## Key Concepts

### LibraryDatabase

The `LibraryDatabase` is a wrapper around a SQLite database that provides type-safe access to music library data. It supports:

- Schema management
- Parameterized queries
- In-memory databases for testing
- File-based persistent storage

### Pagination

Most query functions return a `PagingResponse` that includes:

- Current page items
- Total count
- Offset and limit information
- Ability to fetch additional pages

Access items from a paging response:

```rust
for item in response.page.items() {
    println!("{:?}", item);
}
```

### ID Types

The library uses the `Id` enum to represent entity identifiers:

- `Id::Number(u64)` - Numeric database ID
- `Id::String(String)` - String-based external API ID

### Album Filtering

The `AlbumsRequest` supports rich filtering:

- **By source**: Local files, Tidal, Qobuz, etc.
- **By artist**: Filter to specific artist ID or API ID
- **By album type**: Regular albums, EPs, singles, compilations
- **By name/artist/search**: Text-based filtering
- **Sorting**: By name, artist, release date, or date added (ascending/descending)

### Error Handling

All library functions return `Result` types with specific error enums:

- `LibraryFavoriteArtistsError`
- `LibraryFavoriteAlbumsError`
- `LibraryFavoriteTracksError`
- `LibraryArtistError`, `LibraryAlbumError`, `LibraryTrackError`

Use the `?` operator to propagate errors or match on specific error variants.

## Testing the Example

The example is self-contained with an in-memory database, so it will always produce consistent output. To modify it:

1. **Add more sample data**: Edit the `create_test_database()` function to insert additional artists, albums, or tracks
2. **Try different filters**: Modify the `AlbumsRequest` to test different filter combinations
3. **Test pagination**: Change offset and limit values to see how pagination works
4. **Experiment with sorting**: Try different `AlbumSort` options

Example modifications:

```rust
// Filter albums by artist name
let albums_request = AlbumsRequest {
    filters: Some(AlbumFilters {
        artist: Some("beatles".to_string()),
        ..Default::default()
    }),
    ..Default::default()
};

// Sort albums by release date descending
let albums_request = AlbumsRequest {
    sort: Some(AlbumSort::ReleaseDateDesc),
    ..Default::default()
};
```

## Troubleshooting

### "Database error" when running

- Ensure all workspace dependencies are properly resolved
- Try running `cargo clean` and rebuilding

### "Type not found" errors

- Verify you have the correct version of `moosicbox_library` and related crates
- Check that workspace dependencies are properly defined

### No output or empty results

- The example creates its own in-memory database with sample data
- If you see empty results, check the `create_test_database()` function for any errors

### Pagination seems incorrect

- Remember that `offset` is the starting index (0-based)
- `limit` is the maximum number of items to return
- `total` is the total count of matching items across all pages

## Related Examples

This is currently the only example for `moosicbox_library`. For related functionality, see:

- Search operations: Use the `search()` function for full-text search across the library
- Reindexing: Use `reindex_global_search_index()` to rebuild search indices
- Album versions: Use `album_versions()` to get different quality versions of an album
