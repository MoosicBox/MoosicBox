# Basic Handler Example

This example demonstrates basic request handling using only `RequestData` extraction, without any JSON or serde dependencies.

## Features Demonstrated

- Simple `RequestData` extraction
- Multiple extractors in a single handler
- No external dependencies beyond the web server itself
- Works with both Actix and Simulator backends

## Running the Example

### With Simulator (default)

```bash
cargo run -p basic_handler_standalone_example
```

### With Actix

```bash
cargo run -p basic_handler_standalone_example --features actix --no-default-features
```

## What It Does

The example sets up several routes:

- `/basic-info` - Extracts and displays request information
- `/double` - Demonstrates using two `RequestData` extractors
- `/error` - Demonstrates basic handler with tips about RequestData usage

## Expected Output

When run with the simulator, it will:

1. Create the routes
2. Run test requests against them
3. Display the extracted request data

Example output:

```
🎯 Basic Handler Examples - RequestData Only
============================================

🧪 Running Simulator Backend Basic Handler Examples...
✅ Basic routes created:
   GET: /basic-info GET
   GET: /double GET
   GET: /error GET
   Backend: Simulator

📋 Testing Basic Info Handler (RequestData only):
✅ RequestData extracted successfully:
   Method: Get
   Path: /basic-info
   Query: test=1&debug=true
   Headers: 2

📋 Testing Double Data Handler (RequestData + RequestData):
✅ Double RequestData extracted successfully:
   Data1 Method: Get
   Data2 Method: Get
   Same data: true

✅ Basic Handler Examples Complete!
   - RequestData extraction working standalone
   - Multiple RequestData extractors in one handler
   - No serde or JSON dependencies required
   - Works with both Actix and Simulator backends
   - Clean, minimal web server functionality
```

## Key Concepts

- **RequestData**: Contains parsed request information (method, path, query, headers, etc.)
- **Multiple Extractors**: You can use the same extractor multiple times in one handler
- **No Dependencies**: This example works without any JSON parsing or serialization libraries
- **Backend Agnostic**: The same handler code works with both Actix Web and the test simulator

## Use Cases

This example is perfect for:

- Simple web services that don't need JSON
- Learning the basics of the web server framework
- Building lightweight APIs
- Understanding request extraction fundamentals

## Code Walkthrough

### Handler Definition

```rust
async fn basic_info_handler(data: RequestData) -> Result<HttpResponse, Error> {
    // Access request information
    let method = data.method;
    let path = data.path;
    let query = data.query;

    // Build response
    Ok(HttpResponse::ok().with_body(format!(
        "Method: {:?}, Path: {}, Query: {}",
        method, path, query
    )))
}
```

### Route Registration

```rust
let route = Route::with_handler1(Method::Get, "/basic-info", basic_info_handler);
```

### Multiple Extractors

```rust
async fn double_handler(
    data1: RequestData,
    data2: RequestData,
) -> Result<HttpResponse, Error> {
    // Both parameters contain the same request data
    // Useful for demonstrating extractor mechanics
    Ok(HttpResponse::ok().with_body("..."))
}
```

## Troubleshooting

### Feature Flag Issues

**Problem**: "trait bound not satisfied" errors
**Solution**: Ensure correct backend feature is enabled:

```bash
--no-default-features --features actix        # for Actix backend
# or use default features for Simulator
```

### Handler Registration Errors

**Problem**: Cannot convert handler to Route
**Solution**: Ensure you're using the correct method for the number of parameters:

- `Route::with_handler()` for 0 parameters
- `Route::with_handler1()` for 1 parameter (like RequestData)
- `Route::with_handler2()` for 2 parameters

### Backend-Specific Behavior

**Problem**: Different behavior between Actix and Simulator
**Solution**: This is expected - the Simulator backend is for testing. Use Actix backend for production HTTP servers.

## Related Examples

- **basic_handler**: Handler registration without running a server
- **simple_get**: Basic routing with CORS and server startup
- **query_extractor_standalone**: Add query parameter extraction
- **json_extractor_standalone**: Add JSON body parsing
- **combined_extractors_standalone**: Combine multiple extractors together
