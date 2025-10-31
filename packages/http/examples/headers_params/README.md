# Headers and Query Parameters Example

Demonstrates how to add custom HTTP headers and query parameters to requests using the `switchy_http` crate.

## What This Example Demonstrates

- Adding custom HTTP headers to requests
- Adding query parameters to URLs
- Using optional query parameters with `query_param_opt()`
- Adding multiple query parameters at once with `query_params()`
- Using the `Header` enum for common headers
- Combining headers and query parameters in a single request
- Inspecting request headers and query params with httpbin.org

## Prerequisites

- Understanding of async/await in Rust
- Knowledge of HTTP headers and query parameters
- Familiarity with REST API conventions
- Basic understanding of serde for JSON deserialization

## Running the Example

```bash
cargo run --manifest-path packages/http/examples/headers_params/Cargo.toml
```

## Expected Output

The example runs six different scenarios and prints the results:

```
=== Example 1: Custom Headers ===
Response with custom headers:
{
  "headers": {
    "User-Agent": "switchy_http/1.0",
    "X-Custom-Header": "my-custom-value",
    "Authorization": "Bearer fake-token-for-demo"
  }
}

=== Example 2: Query Parameters ===
Query parameters received by server:
{
  "page": "1",
  "limit": "10",
  "sort": "name"
}

...
```

## Code Walkthrough

### Adding custom headers

```rust
let response = client
    .get("https://httpbin.org/headers")
    .header("User-Agent", "switchy_http/1.0")
    .header("X-Custom-Header", "my-custom-value")
    .header("Authorization", "Bearer fake-token-for-demo")
    .send()
    .await?;
```

The `.header()` method adds a custom header to the request. You can chain multiple `.header()` calls to add multiple headers. Common use cases include:

- Authentication: `Authorization` header with API tokens
- User agents: `User-Agent` to identify your client
- Content negotiation: `Accept` to specify response format
- Custom metadata: Any custom headers your API requires

### Adding query parameters

```rust
let response = client
    .get("https://httpbin.org/get")
    .query_param("page", "1")
    .query_param("limit", "10")
    .query_param("sort", "name")
    .send()
    .await?;
```

The `.query_param()` method adds query parameters to the URL. These are appended as `?page=1&limit=10&sort=name`. Common use cases include:

- Pagination: `page`, `limit`, `offset`
- Filtering: `status=active`, `type=premium`
- Sorting: `sort=name`, `order=desc`
- API versioning: `api_version=v2`

### Optional query parameters

```rust
let user_filter: Option<&str> = Some("john");
let category_filter: Option<&str> = None;

let response = client
    .get("https://httpbin.org/get")
    .query_param("status", "active")
    .query_param_opt("user", user_filter)
    .query_param_opt("category", category_filter)
    .send()
    .await?;
```

The `.query_param_opt()` method only adds the parameter if the `Option` is `Some`. This is useful when parameters are conditionally applied based on user input or configuration.

### Bulk query parameters

```rust
let params = [
    ("filter[status]", "active"),
    ("filter[type]", "premium"),
    ("sort", "-created_at"),
    ("page", "2"),
];

let response = client
    .get("https://httpbin.org/get")
    .query_params(&params)
    .send()
    .await?;
```

The `.query_params()` method accepts a slice of tuples, making it easy to add multiple parameters at once. This is particularly useful for complex filtering scenarios.

### Using the Header enum

```rust
let response = client
    .get("https://httpbin.org/headers")
    .header(switchy_http::Header::UserAgent.as_ref(), "CustomBot/1.0")
    .header(switchy_http::Header::Authorization.as_ref(), "Bearer token")
    .send()
    .await?;
```

The `Header` enum provides type-safe constants for common HTTP headers:

- `Header::Authorization`
- `Header::UserAgent`
- `Header::Range`
- `Header::ContentLength`

Using the enum helps prevent typos in header names.

## Key Concepts

- **Header chaining**: Multiple `.header()` calls can be chained together
- **Query parameter encoding**: Special characters in parameters are automatically URL-encoded
- **Builder pattern**: Headers and query params are configured before calling `.send()`
- **Type safety**: The `Header` enum provides compile-time safety for common headers
- **Conditional parameters**: Use `query_param_opt()` for optional filtering

## Testing the Example

The example uses httpbin.org, which echoes back the headers and query parameters it receives. This makes it easy to verify that your headers and parameters are being sent correctly.

You can modify the example to test with your own APIs by:

1. Changing the URL to your API endpoint
2. Adjusting the headers to match your API's authentication requirements
3. Modifying the query parameters to match your API's filtering/pagination scheme

## Troubleshooting

**Problem**: Headers not appearing in the request
**Solution**: Verify the header name is spelled correctly. Use the `Header` enum for common headers to avoid typos.

**Problem**: Query parameters with special characters not working
**Solution**: The library automatically URL-encodes parameters, so just pass the raw values.

**Problem**: API authentication failing
**Solution**: Check that your `Authorization` header format matches what the API expects (e.g., "Bearer token" vs "token" vs "Basic base64").

## Related Examples

- `simple_get` - Basic GET requests without headers or parameters
- `json_post` - POST requests with JSON payloads
