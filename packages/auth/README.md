# MoosicBox Authentication

Basic authentication utilities for client registration and token management in the MoosicBox ecosystem.

## Overview

The MoosicBox Auth package provides:

- **Client Registration**: Register clients and manage access tokens
- **Magic Token Support**: Create and validate magic tokens for authentication flows
- **Signature Token Fetching**: Retrieve signature tokens for secure operations
- **Request Authorization**: Basic request authorization middleware
- **Database Integration**: Store and retrieve client credentials

## Features

### Core Authentication Functions
- **Client ID Generation**: Generate unique client identifiers
- **Token Management**: Store and retrieve client access tokens
- **Magic Token Workflow**: Create and validate temporary authentication tokens
- **Signature Token Access**: Fetch signature tokens for secure operations

### Request Authorization
- **Non-Tunnel Authorization**: Validate that requests are not from tunnel services
- **Header-Based Auth**: Check user agent headers for authorization

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_auth = "0.1.1"
```

## Usage

### Client Registration and Token Management

```rust
use moosicbox_auth::{get_client_id_and_access_token, AuthError};
use switchy_database::config::ConfigDatabase;

#[tokio::main]
async fn main() -> Result<(), AuthError> {
    let db = ConfigDatabase::new().await?;
    let host = "https://api.example.com";

    // Get or create client credentials
    let (client_id, access_token) = get_client_id_and_access_token(&db, host).await?;

    println!("Client ID: {}", client_id);
    println!("Access Token: {}", access_token);

    Ok(())
}
```

### Magic Token Authentication

```rust
use moosicbox_auth::create_magic_token;
use switchy_database::config::ConfigDatabase;

async fn magic_token_example() -> Result<(), Box<dyn std::error::Error>> {
    let db = ConfigDatabase::new().await?;
    let tunnel_host = Some("https://tunnel.example.com".to_string());

    // Create a magic token for authentication flow
    let magic_token = create_magic_token(&db, tunnel_host).await?;

    println!("Magic token: {}", magic_token);

    Ok(())
}
```

### Signature Token Retrieval

```rust
use moosicbox_auth::fetch_signature_token;

async fn get_signature_token() -> Result<(), Box<dyn std::error::Error>> {
    let host = "https://api.example.com";
    let client_id = "your-client-id";
    let access_token = "your-access-token";

    // Fetch signature token for secure operations
    match fetch_signature_token(host, client_id, access_token).await? {
        Some(signature_token) => {
            println!("Signature token: {}", signature_token);
        }
        None => {
            println!("No signature token available");
        }
    }

    Ok(())
}
```

### Request Authorization Middleware

```rust
use moosicbox_auth::NonTunnelRequestAuthorized;
use actix_web::{web, HttpResponse, Result};

// Handler that requires non-tunnel authorization
async fn protected_handler(_auth: NonTunnelRequestAuthorized) -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json("Access granted"))
}

// The middleware automatically checks the User-Agent header
// and rejects requests from "MOOSICBOX_TUNNEL"
```

## API Features

### Database Operations
- Store and retrieve client access tokens
- Manage magic token lifecycle
- Handle credential persistence

### HTTP Integration
- Actix Web request extractor for authorization
- User-Agent based request filtering
- Header validation and parsing

## Error Handling

The package provides comprehensive error handling through the `AuthError` enum:

- `DatabaseFetch`: Database operation errors
- `Parse`: JSON parsing errors
- `Http`: HTTP request errors
- `RegisterClient`: Client registration failures
- `Unauthorized`: Authorization failures

## Environment Variables

- `TUNNEL_ACCESS_TOKEN`: Required for client registration with tunnel services

## Security Considerations

- Magic tokens are UUIDs for temporary authentication
- Client IDs are generated as UUIDs for uniqueness
- Access tokens are managed securely in the database
- Request filtering prevents tunnel service abuse
