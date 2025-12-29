# HTTP Simple GET Example

Shows how to make an HTTP GET request using switchy_http.

## What it does

- Takes a URL as a command line argument
- Creates an HTTP client with switchy_http::Client::new()
- Makes a GET request to the URL
- Prints the response body as text
- Has custom error handling for missing URL argument

## The code

```rust
let response = switchy_http::Client::new().get(&url).send().await?;
println!("response: {}", response.text().await?);
```

## Running it

```bash
cargo run --package http_simple_get -- "https://example.com"
```

## Dependencies

- switchy_http - HTTP client
- switchy_async - Async runtime (with tokio feature)
- thiserror - Error handling
- log & pretty_env_logger - Logging
