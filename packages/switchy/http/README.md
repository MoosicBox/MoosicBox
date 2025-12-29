# Switchy HTTP

A generic HTTP client abstraction library providing unified interfaces for HTTP operations with pluggable backend implementations.

## Features

- **Generic Client Interface**: Unified traits for HTTP clients, request builders, and responses
- **Multiple Backend Support**: Abstraction over different HTTP implementations (reqwest, simulator)
- **Request Building**: Fluent API for building HTTP requests with headers, query parameters, and body
- **Response Handling**: Unified response interface for status, headers, text, bytes, and streaming
- **JSON Support**: Built-in JSON serialization/deserialization support
- **Streaming Support**: Byte stream responses for large data handling
- **Method Support**: GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS methods with dedicated functions; CONNECT and TRACE via `request()` method
- **Error Handling**: Unified error types across different backends

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
switchy_http = "0.1.4"

# Choose your backend (default includes all features)
switchy_http = { version = "0.1.4", features = ["reqwest"] }
# or
switchy_http = { version = "0.1.4", features = ["simulator"] }
```

## Usage

### Basic HTTP Requests

```rust
use switchy_http::{Client, Error};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let client = Client::new();

    // GET request
    let mut response = client
        .get("https://api.example.com/users")
        .header("Authorization", "Bearer token123")
        .query_param("page", "1")
        .query_param("limit", "10")
        .send()
        .await?;

    println!("Status: {:?}", response.status());
    let text = response.text().await?;
    println!("Response: {}", text);

    Ok(())
}
```

### POST with JSON Body

```rust
use switchy_http::Client;
use serde_json::json;

async fn create_user() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();

    let user_data = json!({
        "name": "John Doe",
        "email": "john@example.com"
    });

    let response = client
        .post("https://api.example.com/users")
        .header("Content-Type", "application/json")
        .json(&user_data)
        .send()
        .await?;

    if response.status().is_success() {
        println!("User created successfully");
    }

    Ok(())
}
```

### Handling Different Response Types

```rust
use switchy_http::Client;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct User {
    id: u32,
    name: String,
    email: String,
}

async fn fetch_user_data() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();

    // JSON response
    let user: User = client
        .get("https://api.example.com/users/1")
        .send()
        .await?
        .json()
        .await?;

    println!("User: {} ({})", user.name, user.email);

    // Raw bytes
    let image_data = client
        .get("https://api.example.com/users/1/avatar")
        .send()
        .await?
        .bytes()
        .await?;

    println!("Downloaded {} bytes", image_data.len());

    // Text response
    let readme = client
        .get("https://raw.githubusercontent.com/example/repo/README.md")
        .send()
        .await?
        .text()
        .await?;

    println!("README content: {}", readme);

    Ok(())
}
```

### Streaming Large Responses

```rust
use switchy_http::Client;
use futures::StreamExt;

async fn download_large_file() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();

    let response = client
        .get("https://example.com/large-file.zip")
        .send()
        .await?;

    let mut stream = response.bytes_stream();
    let mut total_bytes = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        total_bytes += chunk.len();
        println!("Downloaded {} bytes so far", total_bytes);

        // Process chunk (e.g., write to file)
    }

    println!("Download complete: {} total bytes", total_bytes);
    Ok(())
}
```

### Custom Headers and Query Parameters

```rust
use switchy_http::{Client, Header};

async fn api_request_with_auth() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();

    let response = client
        .get("https://api.example.com/protected")
        .header(Header::Authorization.as_ref(), "Bearer secret-token")
        .header(Header::UserAgent.as_ref(), "MoosicBox/1.0")
        .query_param("format", "json")
        .query_param_opt("filter", Some("active"))
        .query_params(&[("sort", "name"), ("order", "asc")])
        .send()
        .await?;

    println!("Response status: {:?}", response.status());
    Ok(())
}
```

### Error Handling

```rust
use switchy_http::{Client, Error};

async fn handle_errors() {
    let client = Client::new();

    match client.get("https://invalid-url").send().await {
        Ok(response) => {
            println!("Request succeeded: {:?}", response.status());
        }
        Err(Error::Reqwest(e)) => {
            println!("Network error: {}", e);
        }
        Err(Error::Deserialize(e)) => {
            println!("JSON parsing error: {}", e);
        }
        Err(Error::Decode) => {
            println!("Response decoding error");
        }
    }
}
```

## Architecture

### Core Traits

- **`GenericClient<RB>`**: Main HTTP client interface with method shortcuts
- **`GenericRequestBuilder<R>`**: Request builder interface for headers, params, and body
- **`GenericResponse`**: Response interface for status, headers, and body access
- **`GenericClientBuilder<RB, C>`**: Client builder interface

### Wrapper Types

- **`ClientWrapper`**: Wraps backend-specific clients with unified interface
- **`RequestBuilderWrapper`**: Wraps backend-specific request builders
- **`ResponseWrapper`**: Wraps backend-specific responses

### Header Enumeration

Common HTTP headers are available as enum variants:

- `Authorization`
- `UserAgent`
- `Range`
- `ContentLength`

## Backend Features

### `reqwest` Feature

Enables integration with the popular `reqwest` HTTP client library.

### `simulator` Feature

Enables a simulated HTTP backend for testing and development.

### `json` Feature

Adds JSON serialization/deserialization support using `serde_json`.

### `stream` Feature

Enables streaming response support for handling large responses.

### Compression Features

Enable automatic decompression of HTTP responses (requires `reqwest` feature):

- `brotli` - Brotli decompression
- `deflate` - Deflate decompression
- `gzip` - Gzip decompression
- `zstd` - Zstandard decompression

## Error Types

- `Error::Decode` - Response decoding failures
- `Error::Deserialize` - JSON deserialization errors (with `json` feature)
- `Error::Reqwest` - Reqwest-specific errors (with `reqwest` feature)

## Thread Safety

All client types implement `Send + Sync` for safe usage across async tasks and threads.
