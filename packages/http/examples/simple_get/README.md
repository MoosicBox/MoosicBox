# Simple GET Request Example

A basic example demonstrating how to make HTTP GET requests using the `switchy_http` crate.

## What This Example Demonstrates

- Creating an HTTP client with `switchy_http::Client::new()`
- Making a GET request to a URL
- Reading the response body as text
- Custom error handling for missing command-line arguments
- Using the `reqwest` backend for real network requests

## Prerequisites

- Basic understanding of async/await in Rust
- Familiarity with the Tokio runtime
- Understanding of HTTP GET requests

## Running the Example

```bash
cargo run --manifest-path packages/http/examples/simple_get/Cargo.toml -- "https://example.com"
```

You can try different URLs:

```bash
# Fetch a simple HTML page
cargo run --manifest-path packages/http/examples/simple_get/Cargo.toml -- "https://example.com"

# Fetch JSON data
cargo run --manifest-path packages/http/examples/simple_get/Cargo.toml -- "https://httpbin.org/get"
```

## Expected Output

When run successfully, the example will print the response body to the console. For example, with `https://example.com`, you'll see the HTML content of the page.

If you forget to provide a URL, you'll see an error:

```
Error: MissingUrlArgument
```

## Code Walkthrough

### Setting up the async runtime

```rust
#[tokio::main]
async fn main() -> Result<(), Error> {
    pretty_env_logger::init();
```

The example uses Tokio's `#[tokio::main]` macro to set up an async runtime for making the HTTP request.

### Getting the URL from command-line arguments

```rust
let Some(url) = std::env::args().nth(1) else {
    return Err(Error::MissingUrlArgument);
};
```

The URL is expected as the first command-line argument. If not provided, the example returns a custom error.

### Making the GET request

```rust
let response = switchy_http::Client::new().get(&url).send().await?;
```

This demonstrates the core functionality:

1. `Client::new()` - Creates a new HTTP client instance
2. `.get(&url)` - Creates a GET request builder for the specified URL
3. `.send().await?` - Sends the request and awaits the response

### Reading the response

```rust
println!("response: {}", response.text().await?);
```

The `text()` method reads the entire response body and converts it to a UTF-8 string.

## Key Concepts

- **Client creation**: `Client::new()` creates a reusable HTTP client
- **Request builder pattern**: Methods like `get()` return a builder that can be configured before sending
- **Async/await**: All HTTP operations are asynchronous and require `.await`
- **Error propagation**: The `?` operator propagates errors up the call stack

## Testing the Example

Try these different scenarios:

1. **Valid URL**: Verify you can fetch content from various websites
2. **Missing URL**: Run without arguments to test error handling
3. **Invalid URL**: Try malformed URLs to see how errors are handled
4. **Different content types**: Test with HTML, JSON, XML endpoints

## Troubleshooting

**Problem**: "MissingUrlArgument" error
**Solution**: Make sure to provide a URL as a command-line argument

**Problem**: SSL/TLS errors
**Solution**: Ensure the URL uses `https://` and the certificate is valid

**Problem**: Connection timeout
**Solution**: Check your network connection and verify the URL is accessible

## Related Examples

- `json_post` - Demonstrates POST requests with JSON payloads
- `headers_params` - Shows how to add custom headers and query parameters
