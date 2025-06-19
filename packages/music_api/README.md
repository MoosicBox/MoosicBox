# MoosicBox Music API

A unified API abstraction layer for music services in the MoosicBox ecosystem. This package provides standardized interfaces and implementations for accessing music metadata, search functionality, and authentication across different music streaming services.

## Features

- **Unified API Interface**: Common abstractions for music services (Tidal, Qobuz, local library)
- **Search Functionality**: Standardized search across artists, albums, tracks, and playlists
- **Authentication Management**: Flexible authentication system supporting multiple auth methods
- **Pagination Support**: Efficient handling of large result sets with cursor-based pagination
- **Profile Integration**: Multi-profile support for different user configurations
- **OpenAPI Documentation**: Auto-generated API documentation and client SDKs
- **Async/Await Support**: Non-blocking operations with Tokio async runtime
- **Error Handling**: Comprehensive error types with detailed context

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_music_api = "0.1.1"
```

## Usage

### Basic API Implementation

```rust
use moosicbox_music_api::{MusicApi, SearchQuery, SearchResults};
use async_trait::async_trait;

#[async_trait]
impl MusicApi for MyMusicService {
    async fn search(&self, query: &SearchQuery) -> Result<SearchResults, ApiError> {
        // Implement search functionality
        let results = self.perform_search(query).await?;
        Ok(SearchResults::from(results))
    }

    async fn get_artist(&self, id: &str) -> Result<Artist, ApiError> {
        // Fetch artist by ID
        self.fetch_artist(id).await
    }

    async fn get_album(&self, id: &str) -> Result<Album, ApiError> {
        // Fetch album by ID
        self.fetch_album(id).await
    }
}
```

### Search Operations

```rust
use moosicbox_music_api::{SearchQuery, SearchType, SearchResults};

// Create search query
let query = SearchQuery::new("Pink Floyd")
    .with_types(vec![SearchType::Artist, SearchType::Album])
    .with_limit(20)
    .with_offset(0);

// Execute search
let results = music_api.search(&query).await?;

// Process results
for artist in results.artists {
    println!("Artist: {} ({})", artist.name, artist.id);
}

for album in results.albums {
    println!("Album: {} by {} ({})", album.title, album.artist, album.id);
}
```

### Authentication

```rust
use moosicbox_music_api::auth::{AuthMethod, AuthResult};

// Username/password authentication
let auth = AuthMethod::UsernamePassword {
    username: "user@example.com".to_string(),
    password: "password".to_string(),
};

let result = music_api.authenticate(auth).await?;
match result {
    AuthResult::Success(token) => {
        println!("Authenticated with token: {}", token);
    }
    AuthResult::RequiresPolling(poll_info) => {
        // Handle polling-based auth (e.g., OAuth device flow)
        let token = music_api.poll_for_token(poll_info).await?;
    }
    AuthResult::Failed(error) => {
        eprintln!("Authentication failed: {}", error);
    }
}
```

### Pagination

```rust
use moosicbox_music_api::{PagingRequest, PagingResponse};

let mut page_request = PagingRequest::new().with_limit(50);
let mut all_tracks = Vec::new();

loop {
    let response: PagingResponse<Track> = music_api
        .get_album_tracks("album_id", &page_request)
        .await?;

    all_tracks.extend(response.items);

    if let Some(next_cursor) = response.next_cursor {
        page_request = page_request.with_cursor(next_cursor);
    } else {
        break;
    }
}
```

## Programming Interface

### Core Traits

```rust
#[async_trait]
pub trait MusicApi: Send + Sync {
    async fn search(&self, query: &SearchQuery) -> Result<SearchResults, ApiError>;
    async fn get_artist(&self, id: &str) -> Result<Artist, ApiError>;
    async fn get_album(&self, id: &str) -> Result<Album, ApiError>;
    async fn get_track(&self, id: &str) -> Result<Track, ApiError>;
    async fn get_playlist(&self, id: &str) -> Result<Playlist, ApiError>;
    async fn authenticate(&self, method: AuthMethod) -> Result<AuthResult, ApiError>;
}

#[async_trait]
pub trait SearchableApi: MusicApi {
    async fn search_artists(&self, query: &str, limit: Option<u32>) -> Result<Vec<Artist>, ApiError>;
    async fn search_albums(&self, query: &str, limit: Option<u32>) -> Result<Vec<Album>, ApiError>;
    async fn search_tracks(&self, query: &str, limit: Option<u32>) -> Result<Vec<Track>, ApiError>;
}
```

### Data Models

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub types: Vec<SearchType>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResults {
    pub artists: Vec<Artist>,
    pub albums: Vec<Album>,
    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub total_count: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    UsernamePassword { username: String, password: String },
    Token(String),
    OAuth { client_id: String, redirect_uri: String },
}
```

## Configuration

### Environment Variables

- `MUSIC_API_TIMEOUT`: Request timeout in seconds (default: 30)
- `MUSIC_API_RETRY_COUNT`: Number of retry attempts (default: 3)
- `MUSIC_API_CACHE_TTL`: Cache time-to-live in seconds (default: 300)

### Feature Flags

- `api`: Enable Actix Web API endpoints
- `auth-poll`: Enable polling-based authentication
- `auth-username-password`: Enable username/password authentication
- `models-api-search`: Enable search API models
- `openapi`: Enable OpenAPI documentation generation

## Web API Endpoints

When the `api` feature is enabled, the following endpoints are available:

```
GET    /search?q={query}&types={types}&limit={limit}
GET    /artists/{id}
GET    /albums/{id}
GET    /tracks/{id}
GET    /playlists/{id}
POST   /auth
GET    /auth/poll/{poll_id}
```

## Error Handling

```rust
use moosicbox_music_api::ApiError;

match music_api.get_artist("invalid_id").await {
    Ok(artist) => println!("Found artist: {}", artist.name),
    Err(ApiError::NotFound) => println!("Artist not found"),
    Err(ApiError::Unauthorized) => println!("Authentication required"),
    Err(ApiError::RateLimited) => println!("Rate limit exceeded"),
    Err(ApiError::ServiceUnavailable) => println!("Service temporarily unavailable"),
    Err(e) => eprintln!("Unexpected error: {}", e),
}
```

## Integration Examples

### With Tidal API

```rust
use moosicbox_music_api::MusicApi;
use moosicbox_tidal::TidalApi;

let tidal = TidalApi::new("client_id", "client_secret")?;
let results = tidal.search(&SearchQuery::new("Daft Punk")).await?;
```

### With Local Library

```rust
use moosicbox_music_api::MusicApi;
use moosicbox_library::LocalLibraryApi;

let library = LocalLibraryApi::new("/path/to/music")?;
let results = library.search(&SearchQuery::new("Beatles")).await?;
```

## Testing

```bash
# Run all tests
cargo test

# Run with specific features
cargo test --features "api,auth-poll"

# Run integration tests
cargo test --test integration
```

## Troubleshooting

### Common Issues

**Authentication Failures**
- Verify credentials are correct
- Check if service requires specific authentication flow
- Ensure network connectivity to authentication servers

**Search Returns No Results**
- Verify search query format
- Check if service requires authentication for search
- Try different search terms or types

**Rate Limiting**
- Implement exponential backoff
- Cache results to reduce API calls
- Consider using multiple API keys if supported

**Performance Issues**
- Enable response caching
- Use pagination for large result sets
- Implement connection pooling for HTTP clients

## See Also

- [`moosicbox_tidal`](../tidal/README.md) - Tidal streaming service integration
- [`moosicbox_qobuz`](../qobuz/README.md) - Qobuz Hi-Res streaming integration
- [`moosicbox_library`](../library/README.md) - Local music library management
- [`moosicbox_search`](../search/README.md) - Full-text search functionality
- [`moosicbox_auth`](../auth/README.md) - Authentication and authorization
- [`moosicbox_paging`](../paging/README.md) - Pagination utilities
