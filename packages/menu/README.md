# MoosicBox Menu

A menu library providing menu-related functionality and models for browsing artists, albums, and tracks in the MoosicBox ecosystem.

## Features

- **Menu Models**: Re-exports menu data models and structures
- **Library Integration**: Functions for fetching and filtering artists, albums, and tracks from the library
- **Album Management**: Add, remove, and re-favorite albums from various API sources
- **API Endpoints**: Optional REST API endpoints for menu operations (requires `api` feature)
- **OpenAPI Support**: Optional OpenAPI documentation generation (requires `openapi` feature)

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_menu = "0.1.4"

# With specific features
moosicbox_menu = { version = "0.1.4", features = ["api", "openapi"] }
```

## Usage

### Basic Usage

```rust
use moosicbox_menu::models;

fn main() {
    // Access menu models and structures
    // Models are re-exported from moosicbox_menu_models
}
```

### Library Operations

Fetch and filter artists:

```rust
use moosicbox_menu::library::artists::{ArtistsRequest, ArtistFilters, get_all_artists};
use moosicbox_music_models::ArtistSort;

let request = ArtistsRequest {
    sources: None,
    sort: Some(ArtistSort::NameAsc),
    filters: ArtistFilters {
        name: None,
        search: Some("search term".to_string()),
    },
};

let artists = get_all_artists(&db, &request).await?;
```

Get albums from a source:

```rust
use moosicbox_menu::library::albums::get_albums_from_source;
use moosicbox_music_api::models::AlbumsRequest;

let albums = get_albums_from_source(&db, &api, request).await?;
```

Album management:

```rust
use moosicbox_menu::library::albums::{add_album, remove_album, refavorite_album};

// Add an album to the library
let album = add_album(&api, &library_api, &db, &album_id).await?;

// Remove an album from the library
let album = remove_album(&api, &library_api, &db, &album_id).await?;

// Re-favorite an album (remove and re-add with updated information)
let album = refavorite_album(&api, &library_api, &db, &album_id).await?;
```

### API Integration

With the `api` feature enabled, bind the API endpoints to your Actix-web application:

```rust
use actix_web::{App, HttpServer, web};
use moosicbox_menu::api::bind_services;

HttpServer::new(|| {
    App::new().service(
        bind_services(web::scope("/menu"))
    )
})
```

## Modules

- **`models`** - Re-exported menu data models from `moosicbox_menu_models`
- **`library`** - Library-specific menu functionality
    - `library::artists` - Functions for fetching, filtering, and sorting artists
    - `library::albums` - Functions for managing albums (get, add, remove, refavorite)
- **`api`** - Optional REST API endpoints (requires `api` feature)
    - Provides endpoints for artists, albums, tracks, and album management operations

## Available Features

- **`api`** - Enables REST API endpoint functionality using Actix-web
- **`openapi`** - Enables OpenAPI/Swagger documentation generation using utoipa
- **`local`** - Enables local scanning functionality (enabled by default)
- **`default`** - Enables `api`, `local`, and `openapi` features

## Key Dependencies

- `moosicbox_menu_models` - Core menu data models and structures
- `moosicbox_library` - Library database access and caching
- `moosicbox_library_music_api` - Library-specific music API implementation
- `moosicbox_music_api` - Music API abstraction layer
- `moosicbox_music_models` - Music domain models
- `moosicbox_scan` - Music library scanning functionality
- `actix-web` - Web framework (optional, with `api` feature)
- `utoipa` - OpenAPI documentation (optional, with `openapi` feature)
