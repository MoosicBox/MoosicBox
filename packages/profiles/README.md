# MoosicBox Profiles

A simple profile name management system for the MoosicBox ecosystem, providing basic profile name storage and validation for request handling.

## Features

- **Profile Name Storage**: Add, remove, and retrieve profile names
- **Profile Validation**: Verify profile names exist before processing requests
- **Thread-Safe Operations**: Safe concurrent access to profile data
- **API Integration**: Extract profile names from HTTP headers and query parameters
- **Event System**: Subscribe to profile addition and removal events (requires `events` feature)

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_profiles = "0.1.4"
```

### Features

- `api` (default): Enables Actix Web request extractors for profile name handling
- `events` (default): Enables event subscription system for profile updates
- `fail-on-warnings`: Treats warnings as errors during compilation

## Usage

### Basic Profile Management

```rust
use moosicbox_profiles::PROFILES;

fn main() {
    // Add a profile
    PROFILES.add("user123".to_string());

    // Check if profile exists
    if let Some(profile) = PROFILES.get("user123") {
        println!("Profile found: {}", profile);
    }

    // Get all profile names
    let all_profiles = PROFILES.names();
    println!("All profiles: {:?}", all_profiles);

    // Add and fetch a profile in one operation
    let profile = PROFILES.add_fetch("user456");
    println!("Added profile: {}", profile);

    // Remove a profile
    PROFILES.remove("user123");
}
```

### API Integration

```rust
use moosicbox_profiles::api::{ProfileName, ProfileNameUnverified};
use actix_web::{web, HttpResponse, Result};

// Extract verified profile name from request
async fn handler(profile: ProfileName) -> Result<HttpResponse> {
    let profile_name: String = profile.into();
    Ok(HttpResponse::Ok().json(format!("Hello, {}", profile_name)))
}

// Extract unverified profile name from request
async fn handler_unverified(profile: ProfileNameUnverified) -> Result<HttpResponse> {
    let profile_name: String = profile.into();
    // Profile name exists in request but may not be registered
    Ok(HttpResponse::Ok().json(format!("Profile: {}", profile_name)))
}
```

### Event Subscription (with `events` feature)

```rust
use moosicbox_profiles::events::{on_profiles_updated_event, trigger_profiles_updated_event};

#[tokio::main]
async fn main() {
    // Subscribe to profile update events
    on_profiles_updated_event(|added, removed| {
        let added = added.to_vec();
        let removed = removed.to_vec();
        async move {
            println!("Profiles added: {:?}", added);
            println!("Profiles removed: {:?}", removed);
            Ok(())
        }
    }).await;

    // Trigger an event manually
    trigger_profiles_updated_event(
        vec!["new_profile".to_string()],
        vec!["old_profile".to_string()]
    ).await.unwrap();
}
```

## API Features

The package provides request extractors for Actix Web:

- **ProfileName**: Extracts and validates profile names from requests
- **ProfileNameUnverified**: Extracts profile names without validation
- **Header Support**: Reads from `moosicbox-profile` header
- **Query Parameter Support**: Reads from `moosicboxProfile` query parameter

Profile names are extracted in this order of precedence:

1. Query parameter `moosicboxProfile`
2. HTTP header `moosicbox-profile`

## Error Handling

- Missing profile information returns `400 Bad Request`
- Non-existent profiles return `400 Bad Request`
- Invalid header values return `400 Bad Request`

## Thread Safety

All operations are thread-safe:

- Profile storage uses `std::sync::RwLock` for concurrent access
- Event listeners (with `events` feature) use `switchy_async::sync::RwLock` for async-safe access
