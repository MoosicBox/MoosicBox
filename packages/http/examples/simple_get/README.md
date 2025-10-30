# Simple HTTP GET Request Example

A basic example demonstrating how to make HTTP GET requests using the `switchy_http` crate and process text responses.

## What This Example Demonstrates

- Creating an HTTP client with `switchy_http::Client::new()`
- Making a GET request to a URL
- Reading the response body as text
- Basic error handling for HTTP operations
- Command-line argument processing for URL input

## Prerequisites

- Basic understanding of async Rust (`async`/`await`)
- Familiarity with HTTP GET requests
- Tokio async runtime knowledge

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/http/examples/simple_get/Cargo.toml -- "https://example.com"
```

Or using the package name:

```bash
cargo run --package http_simple_get -- "https://httpbin.org/get"
```

## Expected Output

When run with `https://example.com`, you should see output similar to:

```
INFO  http_simple_get > args="https://example.com"
response: <!doctype html>
<html>
<head>
    <title>Example Domain</title>
    ...
</head>
...
</html>
```

The response body is printed to stdout as plain text.

## Code Walkthrough

### 1. Creating the HTTP Client

```rust
let response = switchy_http::Client::new().get(&url).send().await?;
```

This line demonstrates the builder pattern:

- `Client::new()` - Creates a new HTTP client with default settings
- `.get(&url)` - Initiates a GET request to the specified URL
- `.send()` - Executes the request asynchronously
- `.await?` - Awaits the response and propagates any errors

### 2. Reading the Response

```rust
println!("response: {}", response.text().await?);
```

The `text()` method:

- Consumes the response body
- Converts bytes to a UTF-8 string
- Returns `Result<String, Error>`

### 3. Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Http(#[from] switchy_http::Error),
    #[error("MissingUrlArgument")]
    MissingUrlArgument,
}
```

The example defines custom errors for both HTTP failures and missing arguments.

## Key Concepts

- **HTTP Client Abstraction**: `switchy_http` provides a unified interface that works with multiple backend implementations (reqwest, simulator)
- **Builder Pattern**: Request configuration uses method chaining for a fluent API
- **Async/Await**: All HTTP operations are asynchronous and require an async runtime (Tokio)
- **Error Propagation**: The `?` operator is used throughout to propagate errors up to `main()`

## Testing the Example

Try different URLs to see various responses:

```bash
# JSON API endpoint
cargo run --package http_simple_get -- "https://httpbin.org/get"

# Plain text response
cargo run --package http_simple_get -- "https://example.com"

# Check error handling with an invalid URL
cargo run --package http_simple_get -- "https://invalid-domain-that-does-not-exist-12345.com"
```

## Troubleshooting

**Error: "MissingUrlArgument"**

- Cause: No URL was provided as a command-line argument
- Solution: Add a URL after `--`: `cargo run --package http_simple_get -- "https://example.com"`

**Network/DNS errors**

- Cause: Network connectivity issues or invalid URL
- Solution: Check your internet connection and verify the URL is valid

**SSL/TLS errors**

- Cause: Certificate validation failures
- Solution: Ensure the target URL uses a valid HTTPS certificate

## Related Examples

- `json_request` - Demonstrates JSON serialization/deserialization with POST requests
