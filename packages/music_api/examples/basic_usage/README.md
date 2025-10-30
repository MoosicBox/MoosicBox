# Basic Usage Example

This example demonstrates how to implement and use the `MusicApi` trait from `moosicbox_music_api`, including caching functionality and collection management.

## Summary

A comprehensive example showing how to create a custom music API implementation, use the caching wrapper, and manage multiple APIs through the `MusicApis` collection type. This example provides a foundation for building music service integrations in MoosicBox.

## What This Example Demonstrates

- Implementing the `MusicApi` trait with a simple in-memory backend
- Creating sample data (artists, albums, tracks)
- Using `CachedMusicApi` to add caching to any API implementation
- Working with pagination using `PagingResult` and `PagingResponse`
- Managing multiple APIs with the `MusicApis` collection
- Proper error handling patterns
- Registering custom API sources with `ApiSource::register`

## Prerequisites

- Basic understanding of Rust async/await syntax
- Familiarity with trait implementation
- Understanding of basic music metadata concepts (artists, albums, tracks)

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/music_api/examples/basic_usage/Cargo.toml
```

Or from the example directory:

```bash
cd packages/music_api/examples/basic_usage
cargo run
```

## Expected Output

The example will output:

```
=== MoosicBox Music API - Basic Usage Example ===

Step 1: Creating a simple music API implementation...
API source: ApiSource { ... }

Step 2: Fetching all artists...
Found 2 artists:
  - The Beatles (ID: Number(1))
  - Pink Floyd (ID: Number(2))

Step 3: Fetching artist with ID 1...
Found artist: The Beatles
Cover: Some("https://example.com/beatles.jpg")

Step 4: Fetching albums for artist ID 1...
Found 1 albums:
  - Abbey Road by The Beatles

Step 5: Fetching tracks for album ID 1...
Found 1 tracks:
  - Come Together

Step 6: Creating a cached version of the API...
Cached API created with cascade delete enabled

First fetch of artist ID 1 (calls underlying API)...
Artist: Some("The Beatles")

Second fetch of artist ID 1 (uses cache)...
Artist: Some("The Beatles")

Step 7: Creating a MusicApis collection...
Added SimpleMusicApi to the collection

Iterating over all APIs in the collection:
  - API source: ApiSource { ... }

Step 8: Demonstrating error handling...
Artist with ID 999 not found (expected)

=== Example completed successfully ===
```

## Code Walkthrough

### 1. Defining the API Implementation

The example creates a `SimpleMusicApi` struct that holds in-memory data:

```rust
struct SimpleMusicApi {
    source: ApiSource,
    artists: Vec<Artist>,
    albums: Vec<Album>,
    tracks: Vec<Track>,
}
```

### 2. Implementing the MusicApi Trait

All required trait methods are implemented to provide full functionality:

```rust
#[async_trait]
impl MusicApi for SimpleMusicApi {
    fn source(&self) -> &ApiSource {
        &self.source
    }

    async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, Error> {
        Ok(self.artists.iter().find(|a| &a.id == artist_id).cloned())
    }

    // ... other methods
}
```

Key points:

- All methods are async and return proper Result types
- Pagination is handled using `PagingResponse` with offset/limit
- The implementation returns cloned data (in a real API, this would fetch from a service)

### 3. Creating Sample Data

The `new()` method initializes sample data:

```rust
fn new() -> Self {
    let source = ApiSource::register("simple", "Simple Music API");

    let artists = vec![
        Artist {
            id: Id::Number(1),
            title: "The Beatles".to_string(),
            // ...
        },
    ];
    // ...
}
```

### 4. Using the Cached API

The example demonstrates wrapping an API with caching:

```rust
let cached_api = CachedMusicApi::new(SimpleMusicApi::new())
    .with_cascade_delete(true);

// First fetch hits the underlying API
let artist1 = cached_api.artist(&Id::Number(1)).await?;

// Second fetch uses the cache
let artist2 = cached_api.artist(&Id::Number(1)).await?;
```

With `cascade_delete` enabled, removing an artist will also remove all associated albums and tracks from the cache.

### 5. Managing API Collections

The `MusicApis` type provides a registry for multiple APIs:

```rust
let mut apis = MusicApis::new();
apis.add_source(Arc::new(Box::new(SimpleMusicApi::new())));

// Iterate over all registered APIs
for api in &apis {
    println!("API source: {:?}", api.source());
}
```

## Key Concepts

### The MusicApi Trait

`MusicApi` is the core abstraction in `moosicbox_music_api`. It defines a unified interface for accessing music metadata from any source (local files, streaming services, etc.). Key responsibilities:

- **Artist operations**: Fetch, add, remove artists
- **Album operations**: Fetch albums, get album versions, manage library
- **Track operations**: Fetch tracks, get track sources and sizes
- **Pagination**: All list operations return `PagingResult` for efficient handling of large datasets
- **Authentication**: Optional `auth()` method for APIs requiring authentication
- **Search**: Optional search functionality via `supports_search()` and `search()`

### Caching Strategy

`CachedMusicApi` provides a transparent caching layer that:

- Caches individual artists, albums, and tracks by ID
- Supports cascade deletion (removing an artist removes its albums/tracks)
- Can be cleared with `clear_cache()`
- Works with any `MusicApi` implementation

### Error Handling

The package provides structured error types:

- `Error::MusicApiNotFound`: The API for a source wasn't registered
- `Error::Unauthorized`: Authentication required or failed
- `Error::UnsupportedAction`: The API doesn't support the requested operation
- `Error::Other`: Generic error wrapper

### API Source Registration

Use `ApiSource::register()` to create unique API sources:

```rust
let source = ApiSource::register("my-service", "My Music Service");
```

This allows the system to distinguish between different music sources (e.g., local library, Tidal, Qobuz).

## Testing the Example

1. Run the example: The program will execute all steps automatically
2. Observe the output: Each step prints what operation it's performing
3. Verify caching: Notice that the second artist fetch mentions using cache
4. Check error handling: The final step demonstrates handling missing data gracefully

## Troubleshooting

### Compilation Errors

If you encounter compilation errors:

- Ensure all workspace dependencies are up to date
- Check that you're using the correct Rust edition (see Cargo.toml)
- Verify the example is added to the workspace members in the root Cargo.toml

### Runtime Errors

This example uses only in-memory data, so runtime errors are unlikely. However, if implementing a real API:

- Handle network errors gracefully
- Implement proper retry logic for transient failures
- Use timeouts to prevent hanging requests
- Validate API responses before returning data

## Related Examples

This is currently the only example for `moosicbox_music_api`. For related concepts, see:

- `moosicbox_paging` package for more pagination examples
- Music service implementations in the MoosicBox codebase (Tidal, Qobuz APIs)
