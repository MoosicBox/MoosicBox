# Query Extractor Example

This example demonstrates query parameter extraction and parsing with the `Query<T>` extractor using serde deserialization. It shows how to handle both required and optional query parameters in a type-safe way.

## What This Example Demonstrates

- **Query<T> Extractor**: Automatic query parameter parsing into Rust structs
- **Required Parameters**: Type-safe handling of mandatory query parameters
- **Optional Parameters**: Using `Option<T>` for optional query parameters
- **Type Conversion**: Automatic conversion from URL strings to Rust types (numbers, booleans, etc.)
- **Combined Extractors**: Using `Query<T>` together with `RequestData`
- **Error Handling**: Graceful handling of malformed or missing parameters
- **URL Encoding**: Proper handling of URL-encoded query strings

## Prerequisites

- Rust toolchain (see root README)
- Understanding of async Rust
- Basic knowledge of HTTP query parameters
- Familiarity with serde deserialization

## Running the Example

### With Simulator Backend (Default)

```bash
# From repository root
cargo run -p query_extractor_standalone_example

# Or with explicit feature
cargo run -p query_extractor_standalone_example --features simulator

# From example directory
cd packages/web_server/examples/query_extractor_standalone
cargo run
```

### With Actix Backend

```bash
# From repository root
cargo run -p query_extractor_standalone_example --features actix --no-default-features

# From example directory
cd packages/web_server/examples/query_extractor_standalone
cargo run --features actix --no-default-features
```

**Note**: This example demonstrates route registration and query extraction mechanics but does not start an actual HTTP server. It uses the simulator backend to test the extraction logic directly.

## Expected Output

When run with the simulator backend (default), the example automatically tests all query extraction patterns:

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

## Code Walkthrough

### Query Parameter Structures

**Simple Query with Required Fields**

```rust
#[derive(Debug, Deserialize)]
struct SimpleQuery {
    name: String,   // Required string parameter
    age: u32,       // Required numeric parameter
}
```

This structure requires both `name` and `age` parameters to be present in the query string. Missing or invalid parameters will result in an error.

**Query with Optional Fields**

```rust
#[derive(Debug, Deserialize)]
struct OptionalQuery {
    search: String,          // Required
    limit: Option<u32>,      // Optional
    offset: Option<u32>,     // Optional
    sort: Option<String>,    // Optional
}
```

Optional fields use `Option<T>`, allowing requests with only the required parameters to succeed.

### Handler Implementations

**Simple Query Extraction**

```rust
async fn simple_query_handler(query: Query<SimpleQuery>) -> Result<HttpResponse, Error> {
    // Query<T> automatically parses the query string
    // Access the inner struct with query.0
    let response = format!(
        "Simple Query Extraction:\n  Name: {}\n  Age: {}",
        query.0.name, query.0.age
    );
    Ok(HttpResponse::ok().with_body(response))
}
```

**Combined Extractors**

```rust
async fn combined_handler(
    query: Query<SimpleQuery>,
    data: RequestData,
) -> Result<HttpResponse, Error> {
    // Use multiple extractors in the same handler
    let response = format!(
        "Query: {} ({})\nPath: {}\nMethod: {:?}",
        query.0.name, query.0.age, data.path, data.method
    );
    Ok(HttpResponse::ok().with_body(response))
}
```

### Route Registration

```rust
moosicbox_web_server::Route::with_handler1(
    moosicbox_web_server::Method::Get,
    "/simple",
    simple_query_handler,
)
```

The `with_handler1` method registers a handler with one extractor parameter.

### Simulator Testing

```rust
let request = SimulationRequest::new(Method::Get, "/simple")
    .with_query_string("name=Alice&age=30");

let stub = SimulationStub::new(request);
let http_request = HttpRequest::Stub(Stub::Simulator(stub));

// Extract query parameters
let query = Query::<SimpleQuery>::from_request_sync(&http_request)?;
```

The simulator backend allows testing query extraction without an HTTP server.

## Key Concepts

### Automatic Type Conversion

The `Query<T>` extractor automatically converts URL query parameters from strings to the appropriate Rust types:

- **Strings**: Direct extraction (`name=John` → `String`)
- **Numbers**: Parsed from strings (`age=30` → `u32`)
- **Booleans**: Parsed from strings (`active=true` → `bool`)
- **Collections**: Arrays via repeated parameters (`id=1&id=2&id=3` → `Vec<u32>`)

### Optional vs Required Parameters

- **Required**: Field without `Option<T>` - request fails if missing
- **Optional**: Field with `Option<T>` - becomes `None` if missing

### URL Encoding

Query parameters are automatically URL-decoded:

- `name=John+Doe` → `"John Doe"`
- `search=rust%20web` → `"rust web"`
- `text=hello%20world%21` → `"hello world!"`

### Error Handling

The extractor returns appropriate errors for:

- **Missing required parameters**: HTTP 400 Bad Request
- **Type conversion failures**: HTTP 400 Bad Request
- **Malformed query strings**: HTTP 400 Bad Request

## Testing the Example

### Manual Testing (If Integrated with HTTP Server)

The example demonstrates query extraction mechanics. If you integrate these handlers into an actual HTTP server, you can test with curl:

**Simple Query Handler**

```bash
# Valid request
curl "http://localhost:8080/simple?name=Alice&age=30"
# Expected: Success with formatted output

# Missing parameter
curl "http://localhost:8080/simple?name=Alice"
# Expected: 400 Bad Request - missing 'age'

# Invalid type
curl "http://localhost:8080/simple?name=Alice&age=invalid"
# Expected: 400 Bad Request - 'age' must be a number
```

**Optional Query Handler**

```bash
# All parameters
curl "http://localhost:8080/optional?search=rust&limit=10&offset=20&sort=date"
# Expected: Success with all fields populated

# Only required parameter
curl "http://localhost:8080/optional?search=web+server"
# Expected: Success with optional fields as None

# With some optional parameters
curl "http://localhost:8080/optional?search=example&limit=5"
# Expected: Success with limit=Some(5), others=None
```

**Combined Handler**

```bash
curl "http://localhost:8080/combined?name=Bob&age=25"
# Expected: Success showing both query data and request info
```

### Running the Simulator Tests

```bash
cargo run -p query_extractor_standalone_example
```

The simulator automatically tests all extraction patterns and displays the results.

## Common Query Parameter Patterns

### Pagination

```rust
#[derive(Deserialize)]
struct PaginationQuery {
    page: Option<u32>,      // Default to page 1 if missing
    per_page: Option<u32>,  // Default to 10 if missing
}

// Usage: ?page=2&per_page=50
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

// Usage: ?category=electronics&min_price=10&max_price=100&in_stock=true
```

### Sorting and Ordering

```rust
#[derive(Deserialize)]
struct SortQuery {
    sort_by: Option<String>,  // Field to sort by
    order: Option<String>,    // "asc" or "desc"
}

// Usage: ?sort_by=price&order=desc
```

### Search with Filters

```rust
#[derive(Deserialize)]
struct SearchQuery {
    q: String,                // Search term (required)
    category: Option<String>,
    limit: Option<u32>,
    offset: Option<u32>,
}

// Usage: ?q=laptop&category=computers&limit=20
```

## Troubleshooting

### Feature Flag Issues

**Problem**: "trait bound not satisfied" or "Query not found"

**Solution**: Ensure the `serde` feature is enabled in your dependencies. This example requires serde for deserialization:

```toml
moosicbox_web_server = { workspace = true, features = ["serde"] }
```

### Missing Required Parameters

**Problem**: Query extraction fails with "missing field" error

**Solution**: Ensure all required fields (those without `Option<T>`) are provided in the query string. Make optional fields use `Option<T>`:

```rust
// Before (fails if limit is missing)
struct Query {
    search: String,
    limit: u32,  // Required!
}

// After (succeeds even if limit is missing)
struct Query {
    search: String,
    limit: Option<u32>,  // Optional
}
```

### Type Conversion Errors

**Problem**: "invalid type" or "failed to parse" errors

**Solution**: Ensure query parameter values match the expected types:

- Numbers: `age=30` not `age=thirty`
- Booleans: `active=true` not `active=yes`
- Check for typos in parameter names

### URL Encoding Issues

**Problem**: Spaces or special characters not handled correctly

**Solution**: Use proper URL encoding in your queries:

- Spaces: Use `+` or `%20` (e.g., `name=John+Doe`)
- Special chars: Use percent encoding (e.g., `search=c%2B%2B`)
- Most HTTP clients handle this automatically

### No Output from Example

**Problem**: Example runs but shows no route testing

**Solution**: Ensure you're running with a valid feature flag:

```bash
cargo run -p query_extractor_standalone_example --features simulator
```

## Related Examples

- **basic_handler_standalone**: Foundation for request handling without query parsing
- **json_extractor_standalone**: Demonstrates JSON body extraction with `Json<T>`
- **combined_extractors_standalone**: Shows multiple extractors working together
- **simple_get**: Basic GET endpoint with manual query string access
- **nested_get**: Route organization patterns applicable to query-based APIs

## Use Cases

This query extraction pattern is essential for:

- **REST APIs**: Filtering, sorting, and pagination
- **Search Endpoints**: Query-based search with multiple filters
- **API Versioning**: Query parameters for version selection
- **Feature Flags**: Enabling features via query parameters
- **Analytics**: Tracking parameters in API requests
- **Data Export**: Customizing export formats and fields

This example provides a solid foundation for building flexible, type-safe query parameter handling in web APIs using the MoosicBox web server framework.
