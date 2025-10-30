# JSON API Request Example

A comprehensive example demonstrating how to work with JSON APIs using the `switchy_http` crate, including JSON serialization, deserialization, and various request patterns.

## What This Example Demonstrates

- Creating JSON request bodies with the `.json()` method
- Sending POST requests with JSON payloads
- Deserializing JSON responses with `.json().await`
- Working with nested JSON structures
- Adding query parameters to requests
- Checking response status codes
- Using `serde` for automatic serialization/deserialization

## Prerequisites

- Understanding of async Rust (`async`/`await`)
- Basic knowledge of HTTP methods (GET, POST)
- Familiarity with JSON and REST APIs
- Understanding of `serde` derive macros

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/http/examples/json_request/Cargo.toml
```

Or using the package name:

```bash
cargo run --package switchy_http_json_request_example
```

## Expected Output

When run successfully, you should see output similar to:

```
INFO  switchy_http_json_request_example > Sending POST request with JSON body...
POST Response:
  Name: John Doe
  Job: Software Engineer
  ID: 123
  Created At: 2024-01-15T10:30:45.123Z

INFO  switchy_http_json_request_example > Sending GET request for JSON data...
GET Response:
  User ID: 2
  Email: janet.weaver@reqres.in
  Name: Janet Weaver
  Avatar: https://reqres.in/img/faces/2-image.jpg

INFO  switchy_http_json_request_example > Sending GET request with query parameters...
Response status: Ok
```

## Code Walkthrough

### 1. Defining Request and Response Types

```rust
#[derive(Debug, Serialize)]
struct CreateUserRequest {
    name: String,
    job: String,
}

#[derive(Debug, Deserialize)]
struct CreateUserResponse {
    name: String,
    job: String,
    id: String,
    #[serde(rename = "createdAt")]
    created_at: String,
}
```

- Use `#[derive(Serialize)]` for request bodies that will be sent as JSON
- Use `#[derive(Deserialize)]` for response bodies that will be parsed from JSON
- Use `#[serde(rename = "...")]` to map between Rust naming conventions and API field names

### 2. Sending JSON POST Requests

```rust
let request_body = CreateUserRequest {
    name: "John Doe".to_string(),
    job: "Software Engineer".to_string(),
};

let response = client
    .post("https://reqres.in/api/users")
    .json(&request_body)
    .send()
    .await?;
```

Key points:

- Create a struct instance with your request data
- Use `.json(&request_body)` to automatically serialize the struct to JSON
- The `Content-Type: application/json` header is set automatically
- The `.json()` method requires the `json` feature to be enabled

### 3. Deserializing JSON Responses

```rust
let user_response: CreateUserResponse = response.json().await?;

println!("Name: {}", user_response.name);
println!("Job: {}", user_response.job);
```

- Call `.json().await?` on the response to deserialize into your type
- Specify the type explicitly or use type inference
- Returns `Result<T, Error>` where `T` is your deserialized type

### 4. Working with Nested JSON

```rust
#[derive(Debug, Deserialize)]
struct GetUserResponse {
    data: UserData,
}

#[derive(Debug, Deserialize)]
struct UserData {
    id: u32,
    email: String,
    first_name: String,
    last_name: String,
}
```

`serde` automatically handles nested structures - just define matching struct hierarchies.

### 5. Adding Query Parameters

```rust
let response = client
    .get("https://reqres.in/api/users")
    .query_param("page", "2")
    .send()
    .await?;
```

- Use `.query_param(name, value)` to add individual parameters
- Parameters are automatically URL-encoded
- Multiple calls to `.query_param()` can be chained

## Key Concepts

- **JSON Feature**: Enable with `features = ["json"]` in your `Cargo.toml`
- **Automatic Serialization**: The `.json()` method handles serialization and sets appropriate headers
- **Type Safety**: `serde` provides compile-time guarantees about JSON structure
- **Error Handling**: Deserialization errors are returned as `Result` types
- **Builder Pattern**: Request configuration uses method chaining for clean, readable code

## Testing the Example

The example uses the public [ReqRes API](https://reqres.in) for testing. You can modify the code to test with other JSON APIs:

```rust
// Try different endpoints
let response = client
    .get("https://jsonplaceholder.typicode.com/posts/1")
    .send()
    .await?;
```

For local testing, you can use tools like:

- `httpbin.org` - HTTP testing service
- `mockoon.com` - Local mock API server
- `json-server` - Quick local JSON API

## Troubleshooting

**Deserialization errors**

- Cause: JSON structure doesn't match your struct definition
- Solution: Use `serde_json::Value` to inspect the raw JSON first, then define matching structs

**Missing field errors**

- Cause: Optional fields in the API response
- Solution: Use `Option<T>` for optional fields in your struct:

```rust
#[derive(Deserialize)]
struct User {
    id: u32,
    nickname: Option<String>, // This field is optional
}
```

**Field name mismatches**

- Cause: API uses different naming conventions (camelCase vs snake_case)
- Solution: Use `#[serde(rename = "fieldName")]` attribute

**Feature not enabled error**

- Cause: The `json` feature is not enabled for `switchy_http`
- Solution: Add `features = ["json"]` to your dependency:

```toml
[dependencies]
switchy_http = { workspace = true, features = ["reqwest", "json"] }
```

## Related Examples

- `simple_get` - Basic GET requests with text responses
