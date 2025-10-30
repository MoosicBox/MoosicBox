# Basic Usage Example

A comprehensive example demonstrating how to use the `moosicbox_qobuz` package to interact with the Qobuz music streaming API.

## Summary

This example shows how to build a Qobuz API client, fetch favorite music collections (artists, albums, tracks), and search the Qobuz catalog using the `MusicApi` trait interface.

## What This Example Demonstrates

- Creating and configuring a `QobuzMusicApi` client with database support
- Using the `MusicApi` trait to interact with Qobuz in a consistent way
- Fetching paginated lists of favorite artists and albums
- Searching the Qobuz catalog for artists, albums, and tracks
- Proper error handling for unauthenticated requests
- Iterating through and displaying music metadata

## Prerequisites

Before running this example, you should:

- Have Rust installed (version 1.75 or higher recommended)
- Have a Qobuz account with valid credentials
- Understand basic async Rust concepts (`tokio`, `async`/`await`)
- Be familiar with the `MusicApi` trait pattern used in MoosicBox

**Note:** This example requires Qobuz credentials to be configured in the database. The example will run but show error messages if credentials are not set up.

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/qobuz/examples/basic_usage/Cargo.toml
```

Or from the example directory:

```bash
cd packages/qobuz/examples/basic_usage
cargo run
```

To see detailed logging output:

```bash
RUST_LOG=debug cargo run --manifest-path packages/qobuz/examples/basic_usage/Cargo.toml
```

## Expected Output

When run with valid Qobuz credentials configured, you should see output similar to:

```
MoosicBox Qobuz API - Basic Usage Example
==========================================

Step 1: Creating database connection...
✓ Database connection established

Step 2: Building Qobuz API client...
✓ Qobuz API client ready

Step 3: Fetching favorite artists...
✓ Found 42 total favorite artists
  Displaying first 5 artists:
  1. Miles Davis (ID: 12345)
  2. John Coltrane (ID: 12346)
  3. Herbie Hancock (ID: 12347)
  4. Bill Evans (ID: 12348)
  5. Keith Jarrett (ID: 12349)

Step 4: Fetching favorite albums...
✓ Found 127 total favorite albums
  Displaying first 5 albums:
  1. Kind of Blue by Miles Davis (ID: album_001)
  2. A Love Supreme by John Coltrane (ID: album_002)
  3. Head Hunters by Herbie Hancock (ID: album_003)
  4. Sunday at the Village Vanguard by Bill Evans (ID: album_004)
  5. The Köln Concert by Keith Jarrett (ID: album_005)

Step 5: Searching Qobuz catalog for 'jazz'...
✓ Search completed successfully
  Artists found: 3
    1. Jazz at Lincoln Center Orchestra
    2. The Jazz Messengers
    3. Modern Jazz Quartet
  Albums found: 3
    1. Various Artists - Essential Jazz Classics
    2. Miles Davis - Birth of the Cool
    3. Duke Ellington - Ellington at Newport
  Tracks found: 3
    1. Miles Davis - So What
    2. John Coltrane - Giant Steps
    3. Dave Brubeck - Take Five

==========================================
Example completed!

Next steps:
- Configure Qobuz credentials using the authentication API
- Explore other MusicApi methods (add_favorite, remove_favorite, etc.)
- Fetch album tracks and track streaming URLs
- Try different search queries and pagination options
```

If credentials are not configured, you'll see warning messages explaining this:

```
⚠ Could not fetch artists: No access token available
  This is expected if you haven't configured Qobuz credentials yet.
```

## Code Walkthrough

### 1. Setting Up the Database Connection

```rust
let db = LibraryDatabase::new("qobuz_example.db")?;
```

The `LibraryDatabase` is used to persist Qobuz credentials and configuration. In production, you'd configure this with your actual database path.

### 2. Building the Qobuz API Client

```rust
let qobuz = QobuzMusicApi::builder()
    .with_db(db.clone())
    .build()
    .await?;
```

The builder pattern provides a clean way to configure the API client. The `db` parameter enables credential persistence and automatic token refresh.

### 3. Fetching Favorite Artists

```rust
match qobuz.artists(Some(0), Some(5), None, None).await {
    Ok(artists) => {
        if let Page::WithTotal { items, total, .. } = &artists.page {
            for artist in items {
                println!("{} (ID: {})", artist.title, artist.id);
            }
        }
    }
    Err(e) => println!("Error: {}", e),
}
```

The `artists()` method is part of the `MusicApi` trait. It returns paginated results with offset and limit parameters.

### 4. Fetching Favorite Albums

```rust
let albums_request = AlbumsRequest {
    page: Some(PageRequest {
        offset: 0,
        limit: 5,
    }),
    ..Default::default()
};

let albums = qobuz.albums(&albums_request).await?;
```

The `albums()` method uses an `AlbumsRequest` struct for more flexible filtering options. Here we're only specifying pagination.

### 5. Searching the Catalog

```rust
let results = qobuz.search("jazz", Some(0), Some(3)).await?;

println!("Artists found: {}", results.artists.len());
println!("Albums found: {}", results.albums.len());
println!("Tracks found: {}", results.tracks.len());
```

Search returns a unified response containing artists, albums, and tracks that match the query.

## Key Concepts

### The MusicApi Trait

The `MusicApi` trait provides a consistent interface across different music sources (Qobuz, Tidal, local library, etc.). This means you can write code that works with any music source without knowing the implementation details.

Key methods include:

- `artists()`, `albums()`, `tracks()` - Fetch collections
- `artist()`, `album()`, `track()` - Fetch single items by ID
- `add_artist()`, `add_album()`, `add_track()` - Add to favorites
- `remove_artist()`, `remove_album()`, `remove_track()` - Remove from favorites
- `search()` - Search the catalog
- `track_source()` - Get streaming URL for a track

### Pagination with PagingResponse

Results are returned as `PagingResponse<T>` which contains:

- `page` - Current page data (items, offset, limit, total/has_more)
- `fetch` - Closure for fetching additional pages

You can iterate through pages or fetch all items at once.

### Database Integration

The `db` feature enables:

- Persistent storage of access tokens and app configuration
- Automatic credential refresh when tokens expire
- Caching of app ID and secrets fetched from Qobuz's web interface

Without database support, you must provide access tokens and app configuration manually with each request.

### Error Handling

The example demonstrates graceful error handling. Common errors include:

- `NoAccessTokenAvailable` - No credentials configured
- `Unauthorized` - Invalid or expired credentials
- `HttpRequestFailed` - Network or API errors

## Testing the Example

### Without Credentials

Run the example to see how it handles missing credentials:

```bash
cargo run --manifest-path packages/qobuz/examples/basic_usage/Cargo.toml
```

You should see informative error messages explaining that credentials need to be configured.

### With Credentials

To test with actual Qobuz data, you first need to authenticate. This can be done through:

1. The MoosicBox server's authentication endpoints
2. Direct use of `moosicbox_qobuz::user_login()` function
3. Manual database configuration

Example using the `user_login()` function:

```rust
use moosicbox_qobuz::user_login;

let result = user_login(
    &db,
    "your_username",
    "your_password",
    None,        // app_id (will be auto-fetched)
    Some(true),  // persist credentials
).await?;

println!("Login successful: {}", result);
```

After authentication, re-run the example to see actual data from your Qobuz account.

## Troubleshooting

### "No access token available" Error

**Cause:** Qobuz credentials haven't been configured in the database.

**Solution:** Authenticate using `user_login()` or through the MoosicBox server's authentication API.

### "Unauthorized" Error

**Cause:** Stored credentials are invalid or expired.

**Solution:** Re-authenticate with fresh credentials using `user_login()`.

### Database File Locked

**Cause:** Another instance of the example is running, or the database wasn't closed properly.

**Solution:** Stop other instances or delete the `qobuz_example.db` file to start fresh.

### Compilation Errors

**Cause:** Missing dependencies or workspace not set up correctly.

**Solution:** Ensure you're running from the repository root with a properly configured workspace:

```bash
cd /path/to/MoosicBox
cargo run --manifest-path packages/qobuz/examples/basic_usage/Cargo.toml
```

## Related Examples

- **MoosicBox Tidal Basic Usage** - Similar example for the Tidal music service
- **MoosicBox Music API Examples** - Core music API trait usage patterns
- **MoosicBox Server Examples** - Full server integration with multiple music sources

For more information on the Qobuz package, see the [main README](../../README.md).
