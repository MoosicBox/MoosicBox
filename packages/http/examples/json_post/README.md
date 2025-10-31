# JSON POST Request Example

Demonstrates making HTTP POST requests with JSON payloads and deserializing JSON responses using the `switchy_http` crate.

## What This Example Demonstrates

- Serializing Rust structs to JSON with serde
- Making POST requests with JSON bodies using `.json()`
- Deserializing JSON responses into Rust structs
- Using the `json` feature of `switchy_http`
- Handling structured API responses
- Working with real-world REST APIs (httpbin.org and JSONPlaceholder)

## Prerequisites

- Understanding of async/await in Rust
- Familiarity with serde for serialization/deserialization
- Basic knowledge of REST APIs and JSON
- Understanding of HTTP POST requests

## Running the Example

```bash
cargo run --manifest-path packages/http/examples/json_post/Cargo.toml
```

## Expected Output

The example will make two POST requests and display their responses:

```
Sending POST request with data: PostData { title: "Learn switchy_http", user_id: 1, completed: false }
Response status: Ok
Response body:
{
  "json": {
    "title": "Learn switchy_http",
    "userId": 1,
    "completed": false
  },
  ...
}

Trying JSONPlaceholder API...
Response status: Created

Deserialized response:
  ID: 201
  Title: Test TODO item
  User ID: 1
  Completed: false
```

## Code Walkthrough

### Defining the request and response structures

```rust
#[derive(Debug, Serialize)]
struct PostData {
    title: String,
    user_id: i32,
    completed: bool,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    id: i32,
    title: String,
    #[serde(rename = "userId")]
    user_id: i32,
    completed: bool,
}
```

We use serde's derive macros to automatically implement JSON serialization for our data structures. The `#[serde(rename = "userId")]` attribute handles field name mapping between Rust's snake_case and JSON's camelCase.

### Creating the request payload

```rust
let post_data = PostData {
    title: "Learn switchy_http".to_string(),
    user_id: 1,
    completed: false,
};
```

We create an instance of our struct with the data we want to send.

### Making a POST request with JSON

```rust
let response = client
    .post("https://httpbin.org/post")
    .json(&post_data)
    .send()
    .await?;
```

The `.json()` method:

1. Serializes the struct to JSON
2. Sets the `Content-Type` header to `application/json`
3. Sets the request body to the JSON data

### Reading the response as text

```rust
let response_text = response.text().await?;
println!("Response body:\n{}", response_text);
```

The `text()` method reads the entire response body as a string, useful for debugging or logging.

### Deserializing the JSON response

```rust
let api_response: ApiResponse = response.json().await?;
```

The `.json()` method on the response:

1. Reads the response body
2. Deserializes it into the specified type
3. Returns a `Result` that we can handle with `?`

## Key Concepts

- **JSON serialization**: Using serde to convert Rust structs to JSON automatically
- **JSON deserialization**: Using serde to parse JSON into strongly-typed Rust structs
- **Content-Type headers**: The `.json()` method automatically sets appropriate headers
- **Type safety**: Compile-time guarantees about the shape of your data
- **Field renaming**: Using `#[serde(rename = "...")]` to map between naming conventions

## Testing the Example

The example uses two different APIs to demonstrate versatility:

1. **httpbin.org**: Echoes back the request data, useful for debugging
2. **JSONPlaceholder**: A fake REST API for testing, returns structured responses

You can modify the example to test with your own APIs by changing the URL and adjusting the struct definitions.

## Troubleshooting

**Problem**: JSON deserialization error
**Solution**: Ensure your struct fields match the API response structure. Use `#[serde(rename = "...")]` for field name mismatches.

**Problem**: Missing fields in response
**Solution**: Use `Option<T>` for optional fields or `#[serde(default)]` for default values.

**Problem**: Network errors
**Solution**: Check your internet connection and verify the API endpoints are accessible.

## Related Examples

- `simple_get` - Basic GET requests without JSON
- `headers_params` - Custom headers and query parameters
