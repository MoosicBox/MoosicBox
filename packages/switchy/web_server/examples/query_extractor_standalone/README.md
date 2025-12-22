# Query Extractor Example

This example demonstrates query parameter extraction and parsing with serde deserialization.

## Features Demonstrated

- `Query<T>` extractor for typed query parameter parsing
- Required and optional query parameters
- Type-safe parameter conversion
- Combined Query + RequestData extraction
- Error handling for malformed parameters

## Running the Example

### With Simulator (default)

```bash
cargo run -p switchy_web_server_example_query_extractor_standalone
```

### With Actix

```bash
cargo run -p switchy_web_server_example_query_extractor_standalone --features actix --no-default-features
```

**Note:** The Actix backend demonstrates route registration but does not start an HTTP server in this example. For actual HTTP server usage with Actix, you would need to integrate with `actix-web`'s server builder.

## Routes

- `GET /simple` - Requires name (string) and age (number) parameters
- `GET /optional` - Requires search (string), optional limit, offset, and sort parameters
- `GET /combined` - Combines Query and RequestData extraction
- `GET /error` - Demonstrates error handling and shows raw query string

## Query Parameter Structures

### SimpleQuery (for /simple)

- `name`: string (required)
- `age`: number (required)

### OptionalQuery (for /optional)

- `search`: string (required)
- `limit`: number (optional)
- `offset`: number (optional)
- `sort`: string (optional)

## Example Requests (if integrated with an HTTP server)

**Note:** This example demonstrates query extraction but does not include a running HTTP server. The requests below show how these handlers would be used if integrated into an actual Actix Web or similar HTTP server.

### Simple Query Handler

```bash
curl "http://localhost:8080/simple?name=Alice&age=30"
```

### Optional Query Handler

```bash
# With all parameters
curl "http://localhost:8080/optional?search=rust&limit=10&offset=20&sort=date"

# With only required parameter
curl "http://localhost:8080/optional?search=web+server"

# With some optional parameters
curl "http://localhost:8080/optional?search=example&limit=5"
```

### Combined Handler

```bash
curl "http://localhost:8080/combined?name=Bob&age=25"
```

### Error Handler (displays raw query string)

```bash
curl "http://localhost:8080/error?any=parameters&you=want"
```

## Expected Output

When run with the simulator, it will automatically test all endpoints:

```
🎯 Query Extractor Examples - Query<T> Usage
==============================================

🧪 Running Simulator Backend Query Extractor Examples...
✅ Query extractor routes created successfully:
   GET: /simple GET
   GET: /optional GET
   GET: /combined GET
   GET: /error GET
   Backend: Simulator

📋 Testing Error Demo Handler (RequestData only):
✅ RequestData extracted successfully:
   Query: test=1&debug=true
   Path: /error

📋 Testing Simple Query Handler:
✅ Query extracted successfully:
   Name: Alice
   Age: 30

📋 Testing Optional Query Handler:
✅ Optional query extracted successfully:
   Search: rust
   Limit: Some(10)
   Sort: Some("date")

✅ Query Extractor Examples Complete!
   - Query<T> extractor working with serde deserialization
   - Support for required and optional query parameters
   - Type-safe query parameter parsing
   - Combined Query + RequestData extraction
   - Error handling for malformed query strings
   - Works with both Actix and Simulator backends
   - Real-world query parameter patterns
```

## Key Concepts

- **Query<T>**: Automatically parses query parameters into Rust structs
- **Type Safety**: Automatic conversion from strings to appropriate types (numbers, booleans, etc.)
- **Optional Parameters**: Use `Option<T>` for parameters that may not be present
- **URL Encoding**: Handles URL-encoded query strings (spaces as `+` or `%20`)
- **Error Handling**: Automatic validation and error responses for invalid parameters

## Common Query Parameter Patterns

### Pagination

```rust
#[derive(Deserialize)]
struct PaginationQuery {
    page: Option<u32>,
    per_page: Option<u32>,
}
```

### Filtering

```rust
#[derive(Deserialize)]
struct FilterQuery {
    category: Option<String>,
    min_price: Option<f64>,
    max_price: Option<f64>,
    in_stock: Option<bool>,
}
```

### Sorting

```rust
#[derive(Deserialize)]
struct SortQuery {
    sort_by: Option<String>,
    order: Option<String>, // "asc" or "desc"
}
```

## Use Cases

This example is perfect for:

- REST APIs with query-based filtering
- Search endpoints with parameters
- Pagination and sorting
- Learning query parameter handling
- Building flexible API endpoints
- Understanding URL parameter parsing
