# Basic Usage Example

A comprehensive example demonstrating the core functionality of `moosicbox_menu` for querying and filtering artists from a music library.

## Summary

This example shows how to use the `moosicbox_menu` library to query artists from a music database with filtering and sorting capabilities. It demonstrates the fundamental operations needed to integrate menu functionality into a MoosicBox application.

## What This Example Demonstrates

- Initializing a `LibraryDatabase` for menu operations
- Setting up the required database schema for artists and albums
- Querying all artists from the library using `get_all_artists`
- Filtering artists by search terms using `ArtistFilters`
- Sorting artists by name in ascending and descending order using `ArtistSort`
- Working with `ArtistsRequest` to configure query parameters
- Processing and displaying query results

## Prerequisites

- Basic understanding of Rust async/await
- Familiarity with database concepts
- Understanding of the MoosicBox library structure

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/menu/examples/basic_usage/Cargo.toml
```

Or from within the example directory:

```bash
cd packages/menu/examples/basic_usage
cargo run
```

## Expected Output

```
MoosicBox Menu - Basic Usage Example
=====================================

Step 1: Initializing database...
✓ Database initialized

Step 2: Creating database schema...
✓ Schema created

Step 3: Inserting sample data...
✓ Sample data inserted

Step 4: Querying all artists...
✓ Found 4 artists:
  - Electronic Pioneers (ID: 3)
  - Indie Rock Collective (ID: 4)
  - Jazz Ensemble (ID: 2)
  - The Classic Rock Band (ID: 1)

Step 5: Searching for artists containing 'Rock'...
✓ Found 2 matching artists:
  - Indie Rock Collective (ID: 4)
  - The Classic Rock Band (ID: 1)

Step 6: Querying artists sorted by name (descending)...
✓ Artists sorted (descending):
  - The Classic Rock Band (ID: 1)
  - Jazz Ensemble (ID: 2)
  - Indie Rock Collective (ID: 4)
  - Electronic Pioneers (ID: 3)

Example completed successfully!

Key takeaways:
- Use ArtistsRequest to configure queries
- Apply filters via ArtistFilters (name, search)
- Sort results with ArtistSort (NameAsc, NameDesc)
- LibraryDatabase provides the database abstraction
```

## Code Walkthrough

### 1. Database Initialization

The example starts by creating an in-memory database:

```rust
let db = TursoDatabase::new(":memory:").await?;
```

In a real application, you would connect to an existing database file or remote database.

### 2. Schema Setup

The required tables for artists and albums are created:

```rust
async fn setup_schema(db: &TursoDatabase) -> Result<(), Box<dyn std::error::Error>> {
    db.exec_raw(
        "CREATE TABLE IF NOT EXISTS artists (
            id INTEGER PRIMARY KEY,
            title TEXT NOT NULL,
            cover TEXT,
            source TEXT NOT NULL
        )",
    )
    .await?;
    // ... more tables
}
```

### 3. Querying Artists with Filters

The core menu operation is performed using `ArtistsRequest`:

```rust
let request = ArtistsRequest {
    sources: None,                    // Query all sources
    sort: Some(ArtistSort::NameAsc),  // Sort by name ascending
    filters: ArtistFilters {
        name: None,                    // No name filter
        search: None,                  // No search filter
    },
};

let artists = get_all_artists(&library_db, &request).await?;
```

### 4. Search Filtering

To search for artists matching a term:

```rust
let search_request = ArtistsRequest {
    sources: None,
    sort: Some(ArtistSort::NameAsc),
    filters: ArtistFilters {
        name: None,
        search: Some("rock".to_lowercase()),  // Case-insensitive search
    },
};

let filtered_artists = get_all_artists(&library_db, &search_request).await?;
```

### 5. Custom Sorting

Change the sort order:

```rust
let sorted_request = ArtistsRequest {
    sources: None,
    sort: Some(ArtistSort::NameDesc),  // Descending order
    filters: ArtistFilters {
        name: None,
        search: None,
    },
};

let sorted_artists = get_all_artists(&library_db, &sorted_request).await?;
```

## Key Concepts

### ArtistsRequest

The `ArtistsRequest` struct configures all aspects of an artist query:

- **sources**: Optional filter by album sources (e.g., Library, Tidal, Qobuz)
- **sort**: Optional sort order using `ArtistSort` enum
- **filters**: Search and name filters via `ArtistFilters`

### ArtistFilters

Provides filtering capabilities:

- **name**: Filter by exact or partial artist name
- **search**: Generic search across artist fields (case-insensitive)

### ArtistSort

Available sort options:

- `ArtistSort::NameAsc` - Sort by name A-Z
- `ArtistSort::NameDesc` - Sort by name Z-A

### LibraryDatabase

A wrapper around database implementations that provides:

- Profile-based database management
- Thread-safe access via internal locking
- Integration with MoosicBox library operations

## Testing the Example

The example includes three query scenarios:

1. **Unfiltered query** - Retrieves all artists sorted alphabetically
2. **Search filter** - Finds artists matching "rock" (case-insensitive)
3. **Descending sort** - Shows artists in reverse alphabetical order

Modify the example to test:

- Different search terms in `ArtistFilters.search`
- Name-based filtering using `ArtistFilters.name`
- Adding more sample artists with diverse names
- Combining multiple filters

## Troubleshooting

### Database Not Found

If you see database-related errors, ensure:

- The `switchy_database` feature `turso` is enabled
- Your database path is correct (this example uses `:memory:`)

### No Artists Returned

If queries return empty results:

- Verify the sample data was inserted successfully
- Check that the database schema matches the expected structure
- Ensure the `LibraryDatabase` wrapper is correctly initialized

### Compilation Errors

If the example doesn't compile:

- Ensure all workspace dependencies are up to date
- Check that the `local` feature is enabled for `moosicbox_menu`
- Verify that `tokio` has the required runtime features

## Related Examples

This is currently the only example for `moosicbox_menu`. Future examples may include:

- Album management operations (add, remove, refavorite)
- Integration with music API sources (Tidal, Qobuz)
- HTTP API endpoint usage with Actix-web
- Album version management
