# MoosicBox Authentication

Basic authentication utilities for client registration and token management in the MoosicBox ecosystem.

## Overview

The MoosicBox Auth package provides:

- **Client Registration**: Register clients and manage access tokens
- **Signature Token Fetching**: Retrieve signature tokens for secure operations
- **Request Authorization**: Basic request authorization middleware
- **Database Integration**: Store and retrieve client credentials
- **API Endpoints**: Optional REST API for magic token management (requires `api` feature)

## Features

### Core Authentication Functions

- **Client ID Generation**: Generate unique client identifiers as UUIDs
- **Token Management**: Store and retrieve client access tokens from database
- **Signature Token Access**: Fetch signature tokens for secure operations

### Request Authorization

- **Non-Tunnel Authorization**: Validate that requests are not from tunnel services
- **Header-Based Auth**: Check user agent headers for authorization

### API Endpoints (requires `api` feature, enabled by default)

- **GET /magic-token**: Retrieve credentials associated with a magic token
- **POST /magic-token**: Create a new magic token for authentication flows
- Magic tokens expire after 1 day and are single-use (deleted after retrieval)

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
moosicbox_auth = "0.1.4"
```

### Available Features

- `api` (default): Enables API endpoints for magic token management
- `openapi` (default): Enables OpenAPI/utoipa documentation support
- `fail-on-warnings`: Enables strict compilation warnings

To use without API endpoints:

```toml
[dependencies]
moosicbox_auth = { version = "0.1.4", default-features = false }
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

### Using API Endpoints

The package provides REST API endpoints when the `api` feature is enabled (default). To use these endpoints in your Actix Web application:

```rust
use actix_web::{App, HttpServer, web};
use moosicbox_auth::api::bind_services;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .service(
                bind_services(web::scope("/auth"))
            )
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
```

This enables:

- `GET /auth/magic-token?magicToken=<token>` - Retrieve credentials for a magic token
- `POST /auth/magic-token?host=<host>` - Create a new magic token

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

## Database Schema

The package uses the following database tables:

### `client_access_tokens`

- `client_id`: String - The unique client identifier
- `token`: String - The access token for the client
- `expires`: Optional timestamp - Token expiration time
- `updated`: Timestamp - Last update time

### `magic_tokens` (requires `api` feature)

- `magic_token`: String - The magic token UUID
- `client_id`: String - Associated client identifier
- `access_token`: String - Associated access token
- `expires`: Timestamp - Expiration time (1 day from creation)

## HTTP Integration

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

- Client IDs are generated as UUIDs for uniqueness
- Magic tokens are UUIDs for temporary authentication (when using API endpoints)
- Magic tokens expire after 1 day and are single-use (deleted upon retrieval)
- Access tokens are managed securely in the database
- Request filtering prevents tunnel service abuse via User-Agent checking
- The `NonTunnelRequestAuthorized` extractor blocks requests with User-Agent "MOOSICBOX_TUNNEL"
