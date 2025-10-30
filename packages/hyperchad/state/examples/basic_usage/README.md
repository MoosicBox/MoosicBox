# Basic Usage Example

A comprehensive example demonstrating the core features of the `hyperchad_state` state management library.

## Summary

This example shows how to use `hyperchad_state` to create a persistent state store with SQLite backend, store and retrieve typed data, and manage application state with both in-memory and file-based persistence.

## What This Example Demonstrates

- Creating an in-memory SQLite persistence backend
- Creating a file-based SQLite persistence backend
- Storing typed data with automatic serialization
- Retrieving stored values with type safety
- Using `take()` to atomically remove and retrieve values
- Clearing all stored state
- Handling non-existent keys gracefully
- Working with multiple different data types
- Understanding the in-memory caching behavior
- Error handling patterns with `Result`

## Prerequisites

- Basic understanding of Rust async/await
- Familiarity with `serde` serialization
- Understanding of key-value storage concepts

## Running the Example

From the repository root:

```bash
cargo run --manifest-path packages/hyperchad/state/examples/basic_usage/Cargo.toml
```

Or from the example directory:

```bash
cd packages/hyperchad/state/examples/basic_usage
cargo run
```

## Expected Output

```
=== HyperChad State - Basic Usage Example ===

1. Creating in-memory state store...
   ✓ In-memory store created

2. Storing application configuration...
   ✓ Stored: AppConfig { theme: "dark", notifications_enabled: true, volume: 0.75 }

3. Retrieving configuration...
   ✓ Retrieved: Some(AppConfig { theme: "dark", notifications_enabled: true, volume: 0.75 })
   ✓ Values match!

4. Storing user preferences...
   ✓ Stored: UserPreferences { username: "alice", language: "en-US", timezone: "America/New_York" }

5. Attempting to retrieve non-existent key...
   ✓ Result: None (expected None)

6. Using take() to remove and retrieve user preferences...
   ✓ Taken: Some(UserPreferences { username: "alice", language: "en-US", timezone: "America/New_York" })
   ✓ After take, key is gone: None

7. Demonstrating cache behavior...
   ✓ First get (loads from persistence)...
   ✓ Second get (served from cache)...
   ✓ Cache working correctly

8. Clearing all stored values...
   ✓ After clear: None (expected None)

9. Creating file-based state store...
   ✓ File-based store created (example_state.db)

10. Storing data to file-based store...
   ✓ Stored to file: AppConfig { theme: "light", notifications_enabled: false, volume: 0.5 }
   ℹ This data persists across application restarts

   ✓ Retrieved from file: Some(AppConfig { theme: "light", notifications_enabled: false, volume: 0.5 })

11. Cleaning up...
   ✓ Removed example database file

=== Example completed successfully! ===
```

## Code Walkthrough

### Setting Up the Store

The example begins by creating an in-memory SQLite persistence backend:

```rust
let persistence = SqlitePersistence::new_in_memory().await?;
let store = StateStore::new(persistence);
```

This creates a state store that uses SQLite for persistence but stores the database in memory rather than on disk.

### Storing Typed Data

The `StateStore` works with any type that implements `Serialize` and `Deserialize`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct AppConfig {
    theme: String,
    notifications_enabled: bool,
    volume: f32,
}

let config = AppConfig {
    theme: "dark".to_string(),
    notifications_enabled: true,
    volume: 0.75,
};
store.set("app_config", &config).await?;
```

The data is automatically serialized to JSON and stored in the SQLite database.

### Retrieving Data

Data retrieval is type-safe and returns an `Option`:

```rust
let loaded_config: Option<AppConfig> = store.get("app_config").await?;
```

If the key exists, you get `Some(value)`. If it doesn't exist, you get `None`.

### Using take() for Atomic Remove-and-Retrieve

The `take()` method atomically removes a value from the store and returns it:

```rust
let taken_prefs: Option<UserPreferences> = store.take("user_prefs").await?;
// The key "user_prefs" is now gone from the store
```

This is useful when you want to consume a value and ensure it's not processed twice.

### Caching Behavior

The `StateStore` maintains an in-memory cache backed by a `BTreeMap` with `RwLock`:

- First `get()` loads from SQLite and populates the cache
- Subsequent `get()` calls for the same key are served from the cache
- `set()` operations update both cache and persistence
- `remove()`, `take()`, and `clear()` operations update both cache and persistence

### File-Based Persistence

For data that should survive application restarts, use a file-based database:

```rust
let file_persistence = SqlitePersistence::new("example_state.db").await?;
let file_store = StateStore::new(file_persistence);

file_store.set("config", &persistent_config).await?;
// This data persists to disk and survives restarts
```

## Key Concepts

### Type Safety

The `StateStore` API uses Rust's type system to ensure you always get the correct type back:

- `set<T>()` accepts any serializable type
- `get<T>()` returns `Option<T>`, deserializing to the requested type
- Attempting to deserialize to the wrong type will return an error

### Error Handling

The example uses Rust's `?` operator for clean error propagation. The main errors you might encounter are:

- `Error::Database` - SQLite operation failed
- `Error::InitDb` - Failed to initialize the database
- `Error::Serde` - Serialization/deserialization failed
- `Error::InvalidDbConfiguration` - Database schema is incorrect

### In-Memory vs File-Based

Choose the right persistence mode for your use case:

- **In-memory** (`new_in_memory()`): Fast, good for temporary state, data lost when process exits
- **File-based** (`new("path.db")`): Persistent across restarts, good for user preferences and app configuration

### Async Operations

All operations are async and require an async runtime (this example uses `tokio`):

```rust
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // All store operations use .await
    store.set("key", &value).await?;
    let value = store.get("key").await?;
}
```

## Testing the Example

You can modify the example to experiment with different scenarios:

1. **Test persistence**: Comment out the cleanup code and run the example twice to see file-based persistence work
2. **Test different types**: Add your own structs with different field types
3. **Test error handling**: Try deserializing to the wrong type to see error handling
4. **Test concurrency**: Clone the `StateStore` and use it from multiple tasks

## Troubleshooting

### "Database is locked" errors

If you see database lock errors, ensure you're not creating multiple `SqlitePersistence` instances pointing to the same file without proper synchronization.

### Serialization errors

Make sure all types you store implement `Serialize` and `Deserialize` from `serde`, and that you've added `#[derive(Serialize, Deserialize)]` to your structs.

### Type mismatch on retrieval

If you stored data as one type and try to retrieve it as another, you'll get a deserialization error. Use the same type for both operations, or use `serde_json::Value` for dynamic typing.

## Related Examples

This is currently the only example for `hyperchad_state`. For related state management and persistence concepts, see:

- The `switchy` package documentation for database abstractions
- The `hyperchad` framework documentation for web component state management
