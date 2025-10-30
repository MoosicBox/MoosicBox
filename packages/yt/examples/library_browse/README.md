# YouTube Music Library Browse Example

This example demonstrates how to browse and interact with a YouTube Music library using the `moosicbox_yt` package.

## Summary

A comprehensive example showing how to set up the YouTube Music API client, browse favorite artists and albums, search for content, and access track information.

## What This Example Demonstrates

- Setting up the YouTube Music API client with database integration
- Browsing favorite artists with pagination and sorting options
- Browsing favorite albums with pagination and sorting
- Accessing album tracks and track metadata
- Searching for content (artists, albums, tracks) on YouTube Music
- Converting search results to formatted structures for easier processing
- Handling asynchronous operations with `tokio`
- Error handling for YouTube Music API operations

## Prerequisites

- Basic understanding of Rust and async/await patterns
- Familiarity with music library concepts (artists, albums, tracks)
- `tokio` runtime knowledge is helpful but not required
- A MoosicBox database connection (created automatically in the example)
- **Note**: This example demonstrates the API framework. Actual YouTube Music API integration requires implementation and authentication

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/yt/examples/library_browse/Cargo.toml
```

## Expected Output

```
=== YouTube Music Library Browse Example ===

Initializing database connection...
Database connection established.

Creating YouTube Music API client...
YouTube Music API client created.

=== Browsing Favorite Artists ===
Fetching favorite artists (sorted by date added, descending)...
Found 3 favorite artists.

1. Pink Floyd
   ID: artist123
   Source: Yt
   Description: Progressive rock band from England
   Cover URL: https://example.com/cover.jpg

2. The Beatles
   ID: artist456
   Source: Yt
   Description: Legendary British rock band

...

=== Browsing Favorite Albums ===
Fetching favorite albums (sorted by date, descending)...
Found 5 favorite albums.

1. The Dark Side of the Moon
   Artist: Pink Floyd
   ID: album789
   Released: 1973-03-01
   Artwork URL: https://example.com/artwork.jpg
   Fetching tracks for this album...
   First 5 tracks:
      1. Speak to Me
         Duration: 1:13
      2. Breathe (In the Air)
         Duration: 2:43
      ...

=== Searching Content ===
Searching YouTube Music for: 'Pink Floyd'...

Artists found:
  1. Pink Floyd (ID: artist123)
     Progressive rock band from England

Albums found:
  1. Pink Floyd - The Dark Side of the Moon
     Released: 1973

Tracks found:
  1. Pink Floyd - Comfortably Numb
     Album: The Wall
     Duration: 6:23

=== Example Complete ===
```

**Note**: Since the YouTube Music API endpoints are currently stubbed, the actual output will depend on what data exists in your local database. The example may show empty results or require authentication implementation to fetch real data.

## Code Walkthrough

### 1. Setting Up the API Client

```rust
// Initialize database connection
let db = LibraryDatabase::new().await?;

// Create YouTube Music API client
let yt_api = YtMusicApi::builder()
    .with_db(db.clone())
    .build()
    .await?;
```

The example starts by establishing a database connection using `LibraryDatabase`, which is required for storing YouTube Music credentials and configuration. The `YtMusicApi` is then built using the builder pattern with database integration enabled.

### 2. Browsing Favorite Artists

```rust
let artists_result = favorite_artists(
    db,
    Some(0),     // offset - pagination start
    Some(20),    // limit - max results
    Some(YtArtistOrder::Date),
    Some(YtArtistOrderDirection::Desc),
    None,        // country_code
    None,        // locale
    None,        // device_type
    None,        // access_token (fetched from db)
    None,        // user_id
).await?;
```

The `favorite_artists` function retrieves the user's favorite artists with:

- **Pagination**: Control result set size with offset and limit
- **Sorting**: Order by date added (most recent first)
- **Flexible parameters**: Optional country code, locale, and device type
- **Automatic authentication**: Access token fetched from database if available

### 3. Browsing Favorite Albums

```rust
let albums_result = favorite_albums(
    db,
    Some(0),
    Some(10),
    Some(YtAlbumOrder::Date),
    Some(YtAlbumOrderDirection::Desc),
    // ... other parameters
).await?;
```

Similar to artist browsing, but focused on albums. The example demonstrates:

- Fetching album metadata (title, artist, release date, artwork)
- Accessing tracks within an album using `album_tracks`
- Displaying track information (number, title, duration)

### 4. Accessing Album Tracks

```rust
let album_id = Id::String(album.id.clone());
let tracks_result = album_tracks(
    db,
    &album_id,
    Some(0),     // offset
    Some(5),     // limit - get first 5 tracks
    None,        // other parameters
).await?;
```

For each album, you can retrieve its tracks with pagination support. The example shows how to:

- Convert album ID to the appropriate `Id` type
- Fetch a subset of tracks (first 5)
- Display track metadata including duration formatting

### 5. Searching Content

```rust
let results = search(query, Some(0), Some(20)).await?;

// Convert to formatted structure
use moosicbox_yt::models::YtSearchResultsFormatted;
let formatted: YtSearchResultsFormatted = results.into();
```

The search function provides:

- **Universal search**: Searches artists, albums, and tracks simultaneously
- **Formatted results**: Converts complex search response to structured data
- **Result categorization**: Separate collections for artists, albums, and tracks

## Key Concepts

### YouTube Music API Client (`YtMusicApi`)

The `YtMusicApi` struct is the main entry point for YouTube Music operations. It requires database integration (`db` feature) to store and retrieve authentication tokens.

### Database Integration

The `db` feature enables persistent storage of:

- OAuth2 access tokens and refresh tokens
- User preferences and configuration
- Cached library data

Without database integration, most YouTube Music operations won't work as authentication state cannot be maintained.

### Pagination and Sorting

All browsing functions support:

- **Offset/Limit**: Standard pagination for controlling result sets
- **Order options**: Sort by date, name, or other criteria
- **Order direction**: Ascending or descending

### Error Handling

YouTube Music operations can fail due to:

- Missing authentication (no access token)
- Network errors during API calls
- Invalid parameters or IDs
- Database connection issues

The example uses `Result<(), Box<dyn std::error::Error>>` for comprehensive error handling with the `?` operator.

### Search Result Processing

YouTube Music search returns a complex nested structure (`YtSearchResults`). The example demonstrates converting to `YtSearchResultsFormatted` for easier access to:

- Separate artist, album, and track collections
- Flattened metadata fields
- Type-safe result iteration

## Testing the Example

Since this is a demonstration of the YouTube Music API framework:

1. **Run the example**: Execute the command above to see the structure and flow
2. **Check output**: Observe how the API is initialized and how data would be accessed
3. **Empty results**: If you see "No favorite artists/albums found", this is expected without authentication
4. **Modify search query**: Change `"Pink Floyd"` to search for different artists
5. **Adjust pagination**: Modify offset/limit values to test different result set sizes

## Troubleshooting

### "No favorite artists/albums found"

- **Cause**: The local database has no cached YouTube Music data
- **Solution**: This is expected behavior for the stub API. Actual implementation requires authentication and API integration.

### Authentication errors

- **Cause**: No valid YouTube Music access token in the database
- **Solution**: The authentication flow is stubbed. To use real authentication, implement the OAuth2 device flow as described in the main package README.

### Database connection errors

- **Cause**: Cannot create or connect to the local database
- **Solution**: Ensure write permissions in the working directory and that `switchy_database` dependencies are properly configured.

### Compilation errors

- **Cause**: Missing dependencies or feature flags
- **Solution**: Ensure the `db` feature is enabled for `moosicbox_yt` (already configured in the example's `Cargo.toml`).

## Related Examples

This is currently the only example for `moosicbox_yt`. Additional examples that could be helpful:

- **Authentication flow**: Demonstrating OAuth2 device authorization
- **Audio streaming**: Showing how to get track URLs and streaming information
- **Library management**: Adding/removing favorites

For more comprehensive usage examples, refer to the inline documentation in the main package README at `packages/yt/README.md`.
