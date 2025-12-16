# Combined Extractors Example

This example demonstrates using multiple extractors together in a single handler function, showcasing the power of combining different data sources.

## Features Demonstrated

- Multiple extractors in a single handler (up to 2 parameters currently)
- Query + RequestData combinations
- JSON + RequestData combinations
- RequestData + RequestData combinations
- JSON API response patterns
- Real-world API endpoint patterns

## Running the Example

### With Simulator (default)

```bash
cargo run -p switchy_web_server_example_combined_extractors_standalone
```

### With Actix

```bash
cargo run -p switchy_web_server_example_combined_extractors_standalone --features actix --no-default-features
```

## Routes

- `GET /search` - Combines Query<SearchQuery> + RequestData
- `PUT /update` - Combines Json<UserUpdate> + RequestData
- `GET /json-info` - Uses RequestData with JSON response
- `GET /double` - Uses RequestData + RequestData (for demonstration)

## Data Structures

### SearchQuery (for /search)

```json
{
    "q": "string (required)",
    "limit": "number (optional)",
    "offset": "number (optional)"
}
```

### UserUpdate (for /update)

```json
{
    "name": "string (optional)",
    "email": "string (optional)",
    "bio": "string (optional)"
}
```

### ApiResponse (returned by all handlers)

```json
{
  "success": boolean,
  "message": "string",
  "data": "object (optional)"
}
```

## Example Requests (if using Actix)

### Search Handler (Query + RequestData)

```bash
curl "http://localhost:8080/search?q=rust+web+server&limit=20&offset=10" \
  -H "User-Agent: MyApp/1.0"
```

### Update Handler (JSON + RequestData)

```bash
curl -X PUT http://localhost:8080/update \
  -H "Content-Type: application/json" \
  -H "User-Agent: MyApp/1.0" \
  -d '{"name": "Alice Updated", "bio": "New bio text"}'
```

### JSON Info Handler (RequestData with JSON response)

```bash
curl "http://localhost:8080/json-info?debug=true" \
  -H "User-Agent: MyApp/1.0"
```

### Double Data Handler (RequestData + RequestData)

```bash
curl "http://localhost:8080/double?param1=value1&param2=value2" \
  -H "User-Agent: MyApp/1.0"
```

## Expected Output

When run with the simulator, it will automatically test all combinations:

```
🎯 Combined Extractors Examples - Multiple Extractors Together
==============================================================

🧪 Running Simulator Backend Combined Extractor Examples...
✅ Combined extractor routes created successfully:
   GET: /search GET
   PUT: /update PUT
   GET: /json-info GET
   GET: /double GET
   Backend: Simulator

📋 Testing JSON Info Handler (RequestData only):
✅ RequestData extracted successfully:
   Method: Get
   Path: /json-info
   Query: test=1&debug=true
   Headers: 2

📋 Testing Double Data Handler (RequestData + RequestData):
✅ Double RequestData extracted successfully:
   Data1 Method: Get
   Data2 Method: Get
   Same data: true

📋 Testing Search Handler (Query + RequestData):
✅ Query + RequestData extracted successfully:
   Search term: rust web server
   Limit: Some(20)
   Request method: Get
   User agent: Some("MoosicBox-CombinedTest/1.0")

✅ Combined Extractors Examples Complete!
   - Multiple extractors working together (up to 2 parameters currently)
   - Query + RequestData combinations
   - Json + RequestData combinations
   - RequestData + RequestData combinations
   - RequestData extraction working standalone
   - JSON API response patterns
   - Works with both Actix and Simulator backends
   - Real-world API endpoint patterns
```

## Key Concepts

- **Multiple Extractors**: Handlers can accept multiple extractors as parameters
- **Data Correlation**: Combine query parameters with request metadata
- **Flexible APIs**: Mix different data sources (JSON body, query params, headers)
- **Consistent Responses**: Use structured response types for all endpoints
- **Request Context**: Access request metadata alongside parsed data

## Common Patterns

### Search with Metadata

```rust
async fn search_handler(
    query: Query<SearchQuery>,
    data: RequestData,
) -> Result<HttpResponse, Error> {
    // Use query.q for the search
    // Use data.user_agent for analytics
    // Use data.remote_addr for rate limiting
}
```

### Update with Audit Trail

```rust
async fn update_handler(
    json: Json<UpdateData>,
    data: RequestData,
) -> Result<HttpResponse, Error> {
    // Use json.0 for the update data
    // Use data.remote_addr for audit logging
    // Use data.headers for authentication
}
```

### Pagination with Context

```rust
async fn list_handler(
    query: Query<PaginationQuery>,
    data: RequestData,
) -> Result<HttpResponse, Error> {
    // Use query.page and query.per_page for pagination
    // Use data.path for canonical URLs
    // Use data.query for next/prev links
}
```

## Use Cases

This example is perfect for:

- Building comprehensive REST APIs
- Learning advanced extractor patterns
- Understanding request data correlation
- Implementing audit trails and logging
- Creating flexible, context-aware endpoints
- Real-world web service development

## Limitations

- Currently supports up to 2 extractors per handler
- More complex combinations require additional handler support
- Some extractor combinations may have ordering requirements
