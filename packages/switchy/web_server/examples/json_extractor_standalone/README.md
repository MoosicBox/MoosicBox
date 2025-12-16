# JSON Extractor Example

This example demonstrates JSON request/response handling with serde deserialization and serialization.

## Features Demonstrated

- `Json<T>` extractor for typed JSON parsing
- Optional field handling with partial updates
- JSON response generation
- Combined JSON + RequestData extraction
- Error handling for malformed JSON
- Content-Type validation

## Running the Example

### With Simulator (default)

```bash
cargo run -p switchy_web_server_example_json_extractor_standalone
```

### With Actix

```bash
cargo run -p switchy_web_server_example_json_extractor_standalone --features actix --no-default-features
```

## Routes

- `POST /simple` - Expects a User JSON with name, email, and age
- `PATCH /optional` - Handles partial updates with optional fields
- `POST /combined` - Combines JSON and RequestData extraction
- `POST /echo` - Returns modified JSON response
- `POST /error` - Demonstrates error handling

## Data Structures

### User (for /simple and /echo)

```json
{
  "name": "string",
  "email": "string",
  "age": number
}
```

### UpdateUser (for /optional)

```json
{
  "name": "string (optional)",
  "email": "string (optional)",
  "age": number (optional),
  "bio": "string (optional)"
}
```

## Example Requests (if using Actix)

### Simple JSON Handler

```bash
curl -X POST http://localhost:8080/simple \
  -H "Content-Type: application/json" \
  -d '{"name": "Alice", "email": "alice@example.com", "age": 30}'
```

### Optional Fields Handler

```bash
curl -X PATCH http://localhost:8080/optional \
  -H "Content-Type: application/json" \
  -d '{"name": "Bob Updated", "bio": "New bio text"}'
```

### Combined Handler

```bash
curl -X POST http://localhost:8080/combined \
  -H "Content-Type: application/json" \
  -d '{"name": "Charlie", "email": "charlie@example.com", "age": 35}'
```

### Echo Handler (returns modified JSON)

```bash
curl -X POST http://localhost:8080/echo \
  -H "Content-Type: application/json" \
  -d '{"name": "Dave", "email": "dave@example.com", "age": 40}'
```

## Expected Output

When run with the simulator, it will automatically test all endpoints:

```
🎯 JSON Extractor Examples - Json<T> Usage
===========================================

🧪 Running Simulator Backend JSON Extractor Examples...
✅ JSON extractor routes created successfully:
   POST: /simple POST
   PATCH: /optional PATCH
   POST: /combined POST
   POST: /echo POST
   POST: /error POST
   Backend: Simulator

📋 Testing Simple JSON Handler:
✅ JSON extracted successfully:
   Name: Alice
   Email: alice@example.com
   Age: 30

📋 Testing Optional JSON Handler:
✅ Optional JSON extracted successfully:
   Name: Some("Bob Updated")
   Email: None
   Age: None
   Bio: Some("New bio text")

📋 Testing JSON Response Handler:
✅ JSON for response extracted successfully:
   Original Name: Charlie
   (Response would modify name to 'Hello, Charlie!')
   Note: HttpResponse doesn't support headers yet

✅ JSON Extractor Examples Complete!
   - Json<T> extractor working with serde deserialization
   - Support for simple JSON structures
   - Optional field handling with partial updates
   - JSON response generation with serde_json
   - Combined JSON + RequestData extraction
   - Error handling and content-type validation
   - Works with both Actix and Simulator backends
   - Real-world JSON API patterns
```

## Key Concepts

- **Json<T>**: Automatically deserializes JSON request bodies into Rust structs
- **Serde Integration**: Uses serde for type-safe JSON parsing
- **Optional Fields**: Handle partial updates with `Option<T>` fields
- **Error Handling**: Automatic validation and error responses for malformed JSON
- **Content-Type**: Validates that requests have proper `application/json` content type

## Use Cases

This example is perfect for:

- REST APIs that accept JSON payloads
- CRUD operations with structured data
- Learning JSON handling in web services
- Understanding serde integration
- Building type-safe APIs
