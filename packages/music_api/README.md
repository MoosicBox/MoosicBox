# MoosicBox Music API

A unified API abstraction layer for music services in the MoosicBox ecosystem. This package provides the `MusicApi` trait and supporting infrastructure for accessing music metadata, search functionality, and authentication across different music streaming services.

## Features

- **MusicApi Trait**: Common async trait defining operations for music services (artists, albums, tracks)
- **Authentication Management**: Flexible authentication system with poll and username/password support
- **Caching Support**: Built-in `CachedMusicApi` wrapper with cascade delete options
- **Profile Integration**: Multi-profile support for different user configurations
- **Pagination Support**: Efficient handling of large result sets using `moosicbox_paging`
- **Async/Await Support**: Non-blocking operations using async/await
- **Error Handling**: Comprehensive error types with detailed context

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_music_api = "0.1.4"
```

## Usage

### Implementing the MusicApi Trait

```rust
use moosicbox_music_api::{MusicApi, Error};
use moosicbox_music_models::{Artist, Album, Track, ApiSource, id::Id};
use moosicbox_paging::{PagingResult, PagingResponse};
use async_trait::async_trait;

struct MyMusicService {
    source: ApiSource,
}

#[async_trait]
impl MusicApi for MyMusicService {
    fn source(&self) -> &ApiSource {
        &self.source
    }

    async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, Error> {
        // Fetch artist by ID
        todo!("Implement artist lookup")
    }

    async fn album(&self, album_id: &Id) -> Result<Option<Album>, Error> {
        // Fetch album by ID
        todo!("Implement album lookup")
    }

    async fn track(&self, track_id: &Id) -> Result<Option<Track>, Error> {
        // Fetch track by ID
        todo!("Implement track lookup")
    }

    // ... implement other required trait methods
}
```

### Using Search (Optional Feature)

```rust
use moosicbox_music_api::MusicApi;
use moosicbox_music_api::models::search::api::ApiGlobalSearchResult;

// Check if API supports search
if api.supports_search() {
    let response = api.search("Pink Floyd", Some(0), Some(20)).await?;

    // Process search results
    for result in response.results {
        match result {
            ApiGlobalSearchResult::Artist(artist) => {
                println!("Artist: {}", artist.title);
            }
            ApiGlobalSearchResult::Album(album) => {
                println!("Album: {}", album.title);
            }
            ApiGlobalSearchResult::Track(track) => {
                println!("Track: {}", track.title);
            }
        }
    }
}
```

### Authentication

```rust
use moosicbox_music_api::auth::{ApiAuth, Auth};

// Using username/password authentication (requires auth-username-password feature)
#[cfg(feature = "auth-username-password")]
{
    use moosicbox_music_api::auth::username_password::UsernamePasswordAuth;

    let username_auth = UsernamePasswordAuth::builder()
        .with_handler(|username, password| async move {
            // Implement your authentication logic
            Ok(true)
        })
        .build()?;

    let api_auth = ApiAuth::builder()
        .with_auth(username_auth)
        .build();

    let logged_in = api_auth.attempt_login(|auth| async move {
        // Perform login with the auth
        Ok(true)
    }).await?;
}

// Using poll-based authentication (requires auth-poll feature)
#[cfg(feature = "auth-poll")]
{
    use moosicbox_music_api::auth::poll::PollAuth;

    let poll_auth = PollAuth::new()
        .with_timeout_secs(120);

    let api_auth = ApiAuth::builder()
        .with_auth(poll_auth)
        .build();
}
```

### Working with Pagination

```rust
use moosicbox_music_api::MusicApi;
use moosicbox_paging::Page;

// Fetch album tracks with pagination
let mut paging_result = music_api
    .album_tracks(&album_id, Some(0), Some(50), None, None)
    .await?;

// Get the first page of results
let tracks = &paging_result[..];

// Fetch more pages using the provided fetch function
let next_page = (paging_result.fetch.lock().await)(50, 50).await?;
```

## Programming Interface

### MusicApi Trait

The core `MusicApi` trait defines the interface all music service implementations must follow:

```rust
#[async_trait]
pub trait MusicApi: Send + Sync {
    fn source(&self) -> &ApiSource;

    // Artist operations
    async fn artists(&self, offset: Option<u32>, limit: Option<u32>,
                    order: Option<ArtistOrder>, order_direction: Option<ArtistOrderDirection>)
                    -> PagingResult<Artist, Error>;
    async fn artist(&self, artist_id: &Id) -> Result<Option<Artist>, Error>;
    async fn add_artist(&self, artist_id: &Id) -> Result<(), Error>;
    async fn remove_artist(&self, artist_id: &Id) -> Result<(), Error>;
    async fn album_artist(&self, album_id: &Id) -> Result<Option<Artist>, Error>;
    async fn artist_cover_source(&self, artist: &Artist, size: ImageCoverSize)
                                 -> Result<Option<ImageCoverSource>, Error>;

    // Album operations
    async fn albums(&self, request: &AlbumsRequest) -> PagingResult<Album, Error>;
    async fn album(&self, album_id: &Id) -> Result<Option<Album>, Error>;
    async fn album_versions(&self, album_id: &Id, offset: Option<u32>, limit: Option<u32>)
                           -> PagingResult<AlbumVersion, Error>;
    async fn artist_albums(&self, artist_id: &Id, album_type: Option<AlbumType>,
                          offset: Option<u32>, limit: Option<u32>,
                          order: Option<AlbumOrder>, order_direction: Option<AlbumOrderDirection>)
                          -> PagingResult<Album, Error>;
    async fn add_album(&self, album_id: &Id) -> Result<(), Error>;
    async fn remove_album(&self, album_id: &Id) -> Result<(), Error>;
    async fn album_cover_source(&self, album: &Album, size: ImageCoverSize)
                                -> Result<Option<ImageCoverSource>, Error>;

    // Track operations
    async fn tracks(&self, track_ids: Option<&[Id]>, offset: Option<u32>, limit: Option<u32>,
                   order: Option<TrackOrder>, order_direction: Option<TrackOrderDirection>)
                   -> PagingResult<Track, Error>;
    async fn track(&self, track_id: &Id) -> Result<Option<Track>, Error>;
    async fn album_tracks(&self, album_id: &Id, offset: Option<u32>, limit: Option<u32>,
                         order: Option<TrackOrder>, order_direction: Option<TrackOrderDirection>)
                         -> PagingResult<Track, Error>;
    async fn add_track(&self, track_id: &Id) -> Result<(), Error>;
    async fn remove_track(&self, track_id: &Id) -> Result<(), Error>;

    // Track source and quality
    async fn track_source(&self, track: TrackOrId, quality: TrackAudioQuality)
                         -> Result<Option<TrackSource>, Error>;
    async fn track_size(&self, track: TrackOrId, source: &TrackSource, quality: PlaybackQuality)
                       -> Result<Option<u64>, Error>;

    // Scan operations
    async fn enable_scan(&self) -> Result<(), Error>;
    async fn scan(&self) -> Result<(), Error>;
    async fn scan_enabled(&self) -> Result<bool, Error>;
    fn supports_scan(&self) -> bool;

    // Search operations
    fn supports_search(&self) -> bool;
    async fn search(&self, query: &str, offset: Option<u32>, limit: Option<u32>)
                   -> Result<ApiSearchResultsResponse, Error>;

    // Other
    fn auth(&self) -> Option<&ApiAuth>;
    fn cached(self) -> impl MusicApi where Self: Sized;
}
```

### Helper Types

```rust
pub enum TrackOrId {
    Track(Box<Track>),
    Id(Id),
}

pub struct MusicApis(Arc<BTreeMap<ApiSource, Arc<Box<dyn MusicApi>>>>);

pub struct CachedMusicApi<T: MusicApi> {
    // Wraps a MusicApi with caching capabilities
}
```

## Configuration

### Feature Flags

- `default`: Includes `all-auth`, `api`, and `openapi` features
- `api`: Enable Actix Web integration for profile-based API request extraction
- `openapi`: Enable OpenAPI documentation generation (delegates to models)
- `all-auth`: Enables both `auth-poll` and `auth-username-password`
- `auth-poll`: Enable polling-based authentication support
- `auth-username-password`: Enable username/password authentication support
- `models-api-search`: Enable search API models (delegates to `moosicbox_music_api_models`)
- `models-search`: Enable search models (delegates to `moosicbox_music_api_models`)
- `fail-on-warnings`: Treat warnings as errors (development feature)

## Error Handling

The package provides a comprehensive `Error` enum for handling various error cases:

```rust
use moosicbox_music_api::Error;

match music_api.artist(&artist_id).await {
    Ok(Some(artist)) => println!("Found artist: {}", artist.title),
    Ok(None) => println!("Artist not found"),
    Err(Error::MusicApiNotFound(source)) => {
        eprintln!("API not found for source: {}", source);
    }
    Err(Error::Unauthorized) => {
        eprintln!("Authentication required");
    }
    Err(Error::UnsupportedAction(action)) => {
        eprintln!("Action not supported: {}", action);
    }
    Err(e) => eprintln!("Unexpected error: {}", e),
}
```

## Using MusicApis Collection

The `MusicApis` type provides a collection of registered music APIs:

```rust
use moosicbox_music_api::{MusicApis, MusicApi};
use moosicbox_music_models::ApiSource;

let mut apis = MusicApis::new();

// Add a music API to the collection
apis.add_source(Arc::new(Box::new(my_api)));

// Retrieve an API by source
if let Some(api) = apis.get(&ApiSource::library()) {
    let artist = api.artist(&artist_id).await?;
}

// Iterate over all registered APIs
for api in &apis {
    println!("API source: {:?}", api.source());
}
```

## Profile Integration

The `profiles` module provides multi-profile support with the `MusicApisProfiles` type:

```rust
use moosicbox_music_api::profiles::PROFILES;

// Add APIs for a profile
PROFILES.add("my-profile".to_string(), music_apis_map);

// Get APIs for a profile
if let Some(apis) = PROFILES.get("my-profile") {
    // Use the APIs
}

// List all profile names
let profile_names = PROFILES.names();
```

When the `api` feature is enabled, `MusicApis` can be extracted from Actix Web requests based on the profile header.

## Caching

The package provides a `CachedMusicApi` wrapper that adds caching to any `MusicApi` implementation:

```rust
use moosicbox_music_api::MusicApi;

let cached_api = my_api.cached();

// Or create with cascade delete enabled
let cached_api = CachedMusicApi::new(my_api)
    .with_cascade_delete(true);

// Clear cache when needed
cached_api.clear_cache().await;
```

When cascade delete is enabled, removing an artist will also remove all associated albums and tracks from the cache.

## Testing

```bash
# Run all tests
cargo test

# Run with specific features
cargo test --features "api,auth-poll"

# Run with all features
cargo test --all-features
```

## Package Dependencies

This package depends on:

- `moosicbox_menu_models` - Menu and album version models
- `moosicbox_music_api_models` - API models for search and other operations
- `moosicbox_music_models` - Core music domain models (Artist, Album, Track)
- `moosicbox_paging` - Pagination utilities
- `moosicbox_profiles` - Profile management
- `async-trait` - Async trait support
- `switchy_async` - Async utilities
- `thiserror` - Error derivation

## See Also

- [`moosicbox_music_models`](../music/models/README.md) - Core music domain models
- [`moosicbox_paging`](../paging/README.md) - Pagination utilities
- [`moosicbox_profiles`](../profiles/README.md) - Profile management
