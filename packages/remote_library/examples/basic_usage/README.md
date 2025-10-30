# Basic Usage Example

Demonstrates the core functionality of the MoosicBox Remote Library client for accessing remote music libraries over HTTP.

## Summary

This example shows how to create a remote library API client, fetch artists and albums with pagination, search the library, and handle errors properly when connecting to a remote MoosicBox server.

## What This Example Demonstrates

- Creating a `RemoteLibraryMusicApi` client instance
- Fetching artists with pagination support
- Retrieving detailed information for specific artists
- Querying albums for a specific artist
- Searching the remote library for artists, albums, and tracks
- Fetching album details and track listings
- Proper error handling for network and API failures
- Using environment variables for configuration

## Prerequisites

- A running MoosicBox server (local or remote)
- The server URL and port (default: `http://localhost:8000`)
- A valid profile name (default: `"default"`)
- Network connectivity to the MoosicBox server

## Running the Example

### Using default configuration (localhost)

```bash
cargo run --manifest-path packages/remote_library/examples/basic_usage/Cargo.toml
```

### Connecting to a remote server

```bash
MOOSICBOX_SERVER_URL="http://192.168.1.100:8080" cargo run --manifest-path packages/remote_library/examples/basic_usage/Cargo.toml
```

### Using a custom profile

```bash
MOOSICBOX_PROFILE="my-profile" cargo run --manifest-path packages/remote_library/examples/basic_usage/Cargo.toml
```

### Combining environment variables

```bash
MOOSICBOX_SERVER_URL="http://192.168.1.100:8080" MOOSICBOX_PROFILE="guest" cargo run --manifest-path packages/remote_library/examples/basic_usage/Cargo.toml
```

## Expected Output

When connected to a MoosicBox server with a populated music library, you'll see output similar to:

```
MoosicBox Remote Library - Basic Usage Example

Connecting to MoosicBox server at: http://localhost:8000
Using profile: default

Fetching first 10 artists...
Found 10 artists (showing 10)

  1. Pink Floyd (ID: 123)
  2. The Beatles (ID: 124)
  3. Led Zeppelin (ID: 125)
  ...

Getting details for artist: Pink Floyd
  Title: Pink Floyd
  ID: 123
  Source: Library

  Fetching albums for this artist...
  Found 5 albums (showing up to 5):
    - The Dark Side of the Moon (1973)
    - Wish You Were Here (1975)
    - The Wall (1979)
    ...

Searching for 'rock'...
Found 5 total results (showing up to 5)

  [Artist] Pink Floyd
  [Album]  Dark Side of the Moon
  [Track]  The Beatles - Hey Jude
  ...

Demonstrating album operations...
Album found:
  Title: The Dark Side of the Moon
  Artist: Pink Floyd
  Released: 1973

  Fetching tracks...
  Found 10 tracks (showing up to 10):
    1. Speak to Me
    2. Breathe
    3. On the Run
    ...

Example completed successfully!
```

## Code Walkthrough

### Step 1: Creating the Remote Library Client

```rust
let api = RemoteLibraryMusicApi::new(
    server_url,           // e.g., "http://localhost:8000"
    ApiSource::library(), // API source identifier
    profile,              // Profile name for authentication
);
```

The `RemoteLibraryMusicApi` is the main client for accessing remote MoosicBox servers. It implements the `MusicApi` trait, providing a consistent interface for music library operations.

### Step 2: Fetching Artists with Pagination

```rust
match api.artists(Some(0), Some(10), None, None).await {
    Ok(artists_page) => {
        let items = artists_page.items();
        for artist in items {
            println!("{} (ID: {})", artist.title, artist.id);
        }
    }
    Err(e) => println!("Error: {}", e),
}
```

The `artists()` method supports:

- **offset**: Starting position (for pagination)
- **limit**: Maximum number of results
- **order**: Sort order (optional)
- **order_direction**: Sort direction (optional)

Results are returned as a `PagingResponse`, which provides access to items and pagination metadata.

### Step 3: Getting Specific Artist Details

```rust
match api.artist(&artist_id).await {
    Ok(Some(artist)) => {
        println!("Artist: {}", artist.title);
    }
    Ok(None) => println!("Artist not found"),
    Err(e) => println!("Error: {}", e),
}
```

Returns `Option<Artist>` - `None` if the artist doesn't exist, `Some(artist)` if found.

### Step 4: Searching the Library

```rust
match api.search("rock", Some(0), Some(5)).await {
    Ok(search_results) => {
        for result in &search_results.results {
            match result {
                ApiGlobalSearchResult::Artist(artist) => { /* ... */ }
                ApiGlobalSearchResult::Album(album) => { /* ... */ }
                ApiGlobalSearchResult::Track(track) => { /* ... */ }
            }
        }
    }
    Err(e) => println!("Error: {}", e),
}
```

Search returns mixed results (artists, albums, tracks) that match the query string.

### Step 5: Fetching Albums and Tracks

```rust
// Get albums for an artist
api.artist_albums(&artist_id, None, Some(0), Some(5), None, None).await?;

// Get a specific album
api.album(&album_id).await?;

// Get tracks in an album
api.album_tracks(&album_id, Some(0), Some(10), None, None).await?;
```

All operations support pagination and return structured data types (`Artist`, `Album`, `Track`).

## Key Concepts

### Remote Library Client

The `RemoteLibraryMusicApi` acts as an HTTP client that proxies all `MusicApi` operations to a remote MoosicBox server. This allows applications to access remote music libraries transparently, using the same interface as local libraries.

### Pagination

All list operations (`artists`, `albums`, `tracks`) return `PagingResponse` objects that support:

- **Offset-based pagination**: Navigate through large result sets
- **Configurable limits**: Control how many items to fetch
- **Total counts**: Know how many total items exist
- **Lazy fetching**: Fetch additional pages on demand

### Error Handling

The library uses `Result` types throughout:

- **`Ok(Some(...))`**: Resource found
- **`Ok(None)`**: Resource not found (404)
- **`Err(MusicApiError::Other(...))`**: Network or HTTP errors
- **`Err(MusicApiError::UnsupportedAction(...))`**: Operation not supported remotely

### Profile Support

The `profile` parameter enables multi-user support or different configurations on the same server. The server uses this header to determine authentication/authorization context.

### API Source

The `ApiSource` identifies which backend API the client is accessing (e.g., local library, Tidal, Qobuz). For remote libraries, use `ApiSource::library()`.

## Testing the Example

### With a Local Server

1. Start your MoosicBox server locally on port 8000
2. Ensure it has some music in the library
3. Run the example with default settings
4. You should see your local library's artists, albums, and tracks

### With a Remote Server

1. Note your remote server's IP address and port
2. Run with: `MOOSICBOX_SERVER_URL="http://192.168.1.100:8080" cargo run --manifest-path packages/remote_library/examples/basic_usage/Cargo.toml`
3. The example will connect to the remote server and display its library

### Without a Server

If no server is available, you'll see error messages like:

```
Error fetching artists: Other(Request(Reqwest(reqwest::Error { ... })))

Note: Make sure your MoosicBox server is running and accessible.
```

This is expected behavior - the example demonstrates proper error handling.

## Troubleshooting

### Connection Refused

**Problem**: `Error fetching artists: ... connection refused`

**Solution**: Make sure the MoosicBox server is running and the URL is correct.

### Not Found (404)

**Problem**: `Artist not found` or empty result sets

**Solution**: Check that your music library is populated on the server.

### Network Timeout

**Problem**: Requests hang or timeout

**Solution**: Verify network connectivity and firewall settings. The server must be reachable from where you run the example.

### Invalid Profile

**Problem**: `Unsuccessful: Status 401` or `403`

**Solution**: Ensure the profile name matches a valid profile configured on the server.

## Related Examples

This is currently the only example for `moosicbox_remote_library`. For related networking examples, see:

- `packages/web_server/examples/simple_get/` - HTTP server example
- `packages/async_service/examples/basic/` - Async service patterns

For music API usage with other backends, refer to the main MoosicBox documentation.
