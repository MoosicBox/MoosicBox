# Basic Database Connection Usage Example

Demonstrates basic database connection initialization patterns using switchy_database_connection.

## Summary

This example shows how to initialize database connections with various backends (SQLite, PostgreSQL), parse credentials from URLs, and handle common errors. It provides a practical introduction to the database connection API.

## What This Example Demonstrates

- Initializing in-memory SQLite databases for testing
- Creating file-based SQLite databases for persistence
- Parsing database credentials from connection URLs
- Creating credentials manually with the builder API
- Handling initialization errors gracefully
- Different database backend configurations

## Prerequisites

- Basic understanding of async Rust and Tokio
- Familiarity with database concepts (connections, credentials)
- No running database server required (uses SQLite)

## Running the Example

```bash
cargo run --manifest-path packages/database_connection/examples/basic_usage/Cargo.toml
```

## Expected Output

```
=== Database Connection Examples ===

1. Initializing in-memory SQLite database...
   ✓ In-memory database initialized successfully
   This database exists only in memory and will be lost when the program exits.

2. Initializing file-based SQLite database...
   ✓ File-based database initialized successfully
   Database file created at: ./example_database.db
   This database persists on disk.

3. Parsing database credentials from URL...
   ✓ Credentials parsed successfully:
     - Host: localhost
     - Database: mydb
     - User: myuser
     - Password: ***
   Note: This example uses SQLite, so PostgreSQL credentials won't be used.

4. Creating credentials manually...
   ✓ Credentials created:
     - Host: database.example.com
     - Database: production_db
     - User: app_user
     - Has password: true

5. Demonstrating error handling...
   Testing invalid URL format...
   ✓ Caught expected error: Invalid URL format

=== All examples completed successfully ===

Next steps:
  - To use PostgreSQL, enable postgres-sqlx or postgres-raw features
  - To use TLS, add postgres-native-tls or postgres-openssl features
  - To use AWS SSM for credentials, enable the 'creds' feature
  - See the README.md for comprehensive documentation

Cleaned up example database file.
```

## Code Walkthrough

### 1. In-Memory Database Initialization

The simplest form of database initialization creates an in-memory SQLite database:

```rust
match init(None, None).await {
    Ok(_db) => {
        println!("✓ In-memory database initialized successfully");
    }
    Err(e) => {
        eprintln!("✗ Failed: {e}");
    }
}
```

**Key points:**

- `init(None, None)` creates an in-memory database when no path is provided
- Perfect for testing or temporary data
- Data is lost when the program exits
- No filesystem permissions required

### 2. File-Based Database Initialization

For persistent storage, provide a file path:

```rust
let db_path = Path::new("./example_database.db");
match init(Some(db_path), None).await {
    Ok(_db) => {
        println!("✓ File-based database initialized");
    }
    Err(e) => {
        eprintln!("✗ Failed: {e}");
    }
}
```

**Key points:**

- `init(Some(path), None)` creates a file-based SQLite database
- File is created automatically if it doesn't exist
- Data persists between program runs
- Requires filesystem write permissions

### 3. Parsing Connection URLs

The package can parse standard database connection URLs:

```rust
let db_url = "postgres://myuser:mypassword@localhost:5432/mydb";
match Credentials::from_url(db_url) {
    Ok(creds) => {
        println!("Host: {}", creds.host());
        println!("Database: {}", creds.name());
        println!("User: {}", creds.user());
    }
    Err(e) => {
        eprintln!("Failed to parse: {e}");
    }
}
```

**Supported URL formats:**

- `postgres://user:pass@host:port/database`
- `postgresql://user:pass@host:port/database`
- `mysql://user:pass@host:port/database` (parsing only)

### 4. Manual Credential Creation

For programmatic credential management:

```rust
let creds = Credentials::new(
    "database.example.com".to_string(),  // host
    "production_db".to_string(),         // database name
    "app_user".to_string(),              // username
    Some("secure_password".to_string()), // password (optional)
);
```

**Key points:**

- Builder-style API for credential construction
- Password is optional (use `None` for passwordless auth)
- Host can include port (`localhost:5432`)
- All string fields are owned for flexibility

### 5. Error Handling

The example demonstrates proper error handling patterns:

```rust
match Credentials::from_url("invalid-url-without-protocol") {
    Ok(_) => println!("Unexpected success"),
    Err(e) => println!("Caught expected error: {e}"),
}
```

**Common errors:**

- `CredentialsParseError::InvalidUrl` - URL format is incorrect
- `CredentialsParseError::MissingHost` - No host specified
- `CredentialsParseError::MissingDatabase` - No database name
- `CredentialsParseError::UnsupportedScheme` - Unknown protocol
- `InitDbError::CredentialsRequired` - Credentials needed but not provided

## Key Concepts

### Database Abstraction

The `Database` trait provides a unified interface across different backends:

- **SQLite backends**: `rusqlite`, `sqlx`, `turso`
- **PostgreSQL backends**: `tokio-postgres`, `sqlx`
- **Connection pooling**: Automatic connection management
- **Type-safe queries**: Compile-time query validation (with SQLx)

### Feature Flags

The package uses Cargo features to select database backends:

- `sqlite-rusqlite` - Synchronous SQLite (wrapped for async)
- `sqlite-sqlx` - Async SQLite via SQLx
- `postgres-raw` - Direct tokio-postgres
- `postgres-sqlx` - PostgreSQL via SQLx
- `turso` - Turso/libSQL support

This example uses `sqlite-sqlx` for simplicity.

### Credential Management

Three ways to provide credentials:

1. **Environment variables**: `DATABASE_URL`, `DB_HOST`, etc.
2. **URL parsing**: `Credentials::from_url()`
3. **Manual construction**: `Credentials::new()`
4. **AWS SSM** (with `creds` feature): Automatic parameter store retrieval

### Connection Pooling

All backends automatically create connection pools:

- Default pool size: 5 connections
- Configurable per backend
- Automatic connection recycling
- Thread-safe sharing via `Box<dyn Database>`

## Testing the Example

### 1. Basic Test

Run the example and verify all 5 steps complete successfully:

```bash
cargo run --manifest-path packages/database_connection/examples/basic_usage/Cargo.toml
```

### 2. Testing with Logging

Enable debug logging to see internal operations:

```bash
RUST_LOG=debug cargo run --manifest-path packages/database_connection/examples/basic_usage/Cargo.toml
```

### 3. Testing Different Backends

To test with different database backends, modify `Cargo.toml`:

```toml
# For rusqlite instead of sqlx
[dependencies]
switchy_database_connection = { workspace = true, features = ["sqlite-rusqlite"] }
```

Note: PostgreSQL and MySQL examples require running database servers, which are outside the scope of this basic example.

### 4. Testing Error Cases

The example includes error handling tests. Try modifying the URLs to test different error conditions:

- Remove `://` to test `InvalidUrl`
- Use empty host to test `MissingHost`
- Remove database name to test `MissingDatabase`

## Troubleshooting

### Permission Denied

**Problem**: Cannot create database file

```
Error: Permission denied (os error 13)
```

**Solution**: Ensure write permissions in the current directory, or specify a writable path:

```rust
let db_path = Path::new("/tmp/example_database.db");
```

### Feature Not Enabled

**Problem**: Database backend not available

```
Error: No such file or directory
```

**Solution**: Ensure the correct feature is enabled in `Cargo.toml`. This example requires `sqlite-sqlx`.

### Missing Tokio Runtime

**Problem**: Async runtime not available

```
Error: there is no reactor running
```

**Solution**: This example uses `#[tokio::main]`, which is already configured. If integrating into your code, ensure Tokio runtime is initialized.

## Related Examples

- [Switchy Async Cancel Example](../../../async/examples/cancel/README.md) - Demonstrates cancellation tokens with async operations
- [Switchy Async Simulated Example](../../../async/examples/simulated/README.md) - Shows task spawning patterns

## Further Reading

- [Main Package README](../../README.md) - Comprehensive documentation with all feature combinations
- [Switchy Database Package](../../../database/README.md) - Database trait abstraction
- [SQLx Documentation](https://docs.rs/sqlx/) - SQLx query builder and connection pooling
- [Rusqlite Documentation](https://docs.rs/rusqlite/) - Synchronous SQLite bindings
