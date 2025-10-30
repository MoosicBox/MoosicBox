# Basic Scan Operations Example

## Summary

This example demonstrates how to use the `moosicbox_music_api_helpers` package to enable, check, and perform music library scanning operations for different music sources.

## What This Example Demonstrates

- Creating a database connection for music library management
- Implementing the `MusicApi` trait (using a mock implementation)
- Enabling scanning for a specific music source
- Checking whether scanning is enabled for a source
- Triggering a scan operation to synchronize music metadata
- Understanding the relationship between music APIs, databases, and scan operations

## Prerequisites

- Basic understanding of Rust async/await patterns
- Familiarity with the concept of music APIs and library scanning
- Understanding of trait objects and dynamic dispatch in Rust
- Knowledge of database operations (helpful but not required)

## Running the Example

Execute the example from the repository root:

```bash
cargo run --manifest-path packages/music_api/helpers/examples/basic_scan_operations/Cargo.toml
```

Or with the fail-on-warnings feature:

```bash
cargo run --manifest-path packages/music_api/helpers/examples/basic_scan_operations/Cargo.toml --features fail-on-warnings
```

## Expected Output

The example will output step-by-step information about the scanning operations:

```
=== MoosicBox Music API Helpers - Basic Scan Operations Example ===

Step 1: Creating database connection
Creating mock database connection...
(In production, this would connect to a real SQLite/PostgreSQL database)
Database connection created successfully

Step 2: Creating mock music API
Created mock music API for source: Library

Step 3: Enabling scan for music source
Scan enabled successfully for source: Library

Step 4: Checking if scan is enabled
Scan enabled status for Library: true

Step 5: Performing scan operation
Note: This will fail in the example because we're using mock data
and don't have the full profile infrastructure set up.

Scan failed (expected in this mock example): ...
In a real application, the scan would:
- Fetch artists, albums, and tracks from the music source
- Store metadata in the database
- Update the library index
- Handle authentication if required

=== Example Summary ===
This example demonstrated:
1. Creating a database connection
2. Implementing a MusicApi trait (mock)
3. Enabling scanning for a music source
4. Checking scan status
5. Triggering a scan operation
...
```

## Code Walkthrough

### 1. Mock Music API Implementation

The example creates a mock implementation of the `MusicApi` trait to demonstrate the pattern:

```rust
struct MockMusicApi {
    source: ApiSource,
}

#[async_trait::async_trait]
impl MusicApi for MockMusicApi {
    fn source(&self) -> &ApiSource {
        &self.source
    }

    // ... other trait methods implemented as stubs
}
```

In a real application, this would be replaced with actual implementations like `SpotifyMusicApi`, `TidalMusicApi`, or `LocalLibraryMusicApi`.

### 2. Database Connection

The example creates an in-memory SQLite database:

```rust
fn create_mock_database() -> Result<LibraryDatabase, Box<dyn std::error::Error>> {
    LibraryDatabase::new_sqlite(":memory:").map_err(Into::into)
}
```

In production, you would connect to a persistent database with proper configuration.

### 3. Enabling Scan

The `enable_scan` function marks a music source as enabled for scanning:

```rust
enable_scan(music_api_trait, &db).await?;
```

This updates the database to indicate that this source should be scanned for new content.

### 4. Checking Scan Status

The `scan_enabled` function verifies whether scanning is enabled:

```rust
let enabled = scan_enabled(music_api_trait, &db).await?;
```

This is useful for conditional logic and UI updates.

### 5. Performing a Scan

The `scan` function performs the actual library scan:

```rust
scan(music_api_trait, &db).await?;
```

This fetches artists, albums, and tracks from the music source and stores them in the database.

## Key Concepts

### Music API Abstraction

The `MusicApi` trait provides a unified interface for accessing music metadata from different sources. Each implementation handles the specifics of communicating with a particular service or local library.

### Database Integration

The `LibraryDatabase` stores music metadata and tracks which sources are enabled for scanning. The `switchy` crate provides database abstraction for multi-profile support.

### Scan Operations

Scanning is the process of synchronizing music metadata from a source into the local database:

1. **Enable Scan**: Mark a source as active for scanning
2. **Check Status**: Verify if a source is enabled
3. **Perform Scan**: Fetch and store metadata from the source

### Error Handling

All helper functions return `Result<T, moosicbox_music_api::Error>` for comprehensive error handling:

- `Error::Unauthorized`: Authentication required but user not logged in
- `Error::MusicApiNotFound`: API implementation not found for source
- Database errors propagated from underlying operations

### Profile Management

The `scan` function uses the global `PROFILES` registry to access music API implementations. In production, this supports multiple user profiles with different configurations.

## Testing the Example

1. **Run the example**: Execute it and observe the output
2. **Verify database operations**: Note that the database is created successfully
3. **Check scan enabling**: Confirm that `enable_scan` succeeds
4. **Verify status check**: Confirm that `scan_enabled` returns `true`
5. **Observe scan failure**: The scan operation is expected to fail in this mock example due to missing profile infrastructure

## Troubleshooting

### "Profile is missing" panic

**Cause**: The `scan` function requires the "master" profile to be registered in the global `PROFILES` registry.

**Solution**: In a real application, ensure profiles are initialized before calling `scan`. This example demonstrates the API pattern but doesn't set up the full infrastructure.

### Database connection errors

**Cause**: Issues creating or accessing the database.

**Solution**: In production, ensure proper database configuration and permissions. This example uses an in-memory database which should always work.

### Authentication errors

**Cause**: Some music sources require authentication before scanning.

**Solution**: Implement the `ApiAuth` trait for your music API and ensure users are logged in before calling `scan`.

## Related Examples

This package currently has one example. For related functionality, see:

- Music API implementations in other MoosicBox packages
- Database integration examples in the `switchy` package
- Scanning infrastructure in the `moosicbox_scan` package
