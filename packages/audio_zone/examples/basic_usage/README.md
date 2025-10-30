# Basic Usage Example

This example demonstrates the fundamental operations for managing audio zones in the `moosicbox_audio_zone` package.

## Summary

This example shows how to create, read, update, and delete audio zones using an in-memory database. It demonstrates the complete lifecycle of audio zone management, from database setup through CRUD operations.

## What This Example Demonstrates

- Creating an in-memory database for testing and development
- Setting up the required database schema for audio zones
- Creating new audio zones
- Listing all audio zones
- Retrieving a specific audio zone by ID
- Updating an audio zone's properties
- Deleting an audio zone
- Verifying database state after operations

## Prerequisites

- Basic understanding of Rust async/await
- Familiarity with database operations
- Understanding of Result types and error handling

## Running the Example

From the repository root, run:

```bash
cargo run --manifest-path packages/audio_zone/examples/basic_usage/Cargo.toml
```

Or with logging enabled to see debug output:

```bash
RUST_LOG=debug cargo run --manifest-path packages/audio_zone/examples/basic_usage/Cargo.toml
```

## Expected Output

When you run the example, you should see output similar to:

```
=== MoosicBox Audio Zone Basic Usage Example ===

Step 1: Creating in-memory database...
Database schema created.

Step 2: Creating a new audio zone...
Created audio zone: 'Living Room' (ID: 1)

Step 3: Creating another audio zone...
Created audio zone: 'Kitchen' (ID: 2)

Step 4: Listing all audio zones...
Found 2 audio zones:
  - Living Room (ID: 1, 0 players)
  - Kitchen (ID: 2, 0 players)

Step 5: Retrieving audio zone by ID...
Retrieved zone: 'Living Room' (ID: 1, 0 players)

Step 6: Updating audio zone name...
Updated zone name: 'Updated Living Room' (ID: 1)

Step 7: Verifying update...
  - Updated Living Room (ID: 1)
  - Kitchen (ID: 2)

Step 8: Deleting audio zone...
Deleted zone: 'Kitchen' (ID: 2)

Step 9: Listing zones after deletion...
Remaining zones: 1
  - Updated Living Room (ID: 1)

=== Example completed successfully! ===
```

## Code Walkthrough

### Database Setup

The example starts by creating an in-memory database using the `SimulationDatabase`:

```rust
let db: Box<dyn Database> = Box::new(SimulationDatabase::new(None).await?);
let db_arc = Arc::new(db);
```

The `SimulationDatabase` is perfect for examples and testing as it provides a lightweight SQLite-based database that doesn't require external setup.

### Schema Creation

The `create_schema` function sets up three tables required for audio zone management:

```rust
// Main audio zones table
db.create_table("audio_zones")
    .column("id", |col| col.integer().primary_key().auto_increment())
    .column("name", |col| col.text().not_null())
    .execute(&***db)
    .await?;

// Players table for individual audio devices
db.create_table("players")
    .column("id", |col| col.integer().primary_key().auto_increment())
    .column("audio_output_id", |col| col.text().not_null())
    .column("name", |col| col.text().not_null())
    .column("playing", |col| col.bool().not_null().default(false))
    // ... timestamps
    .execute(&***db)
    .await?;

// Junction table for many-to-many relationship
db.create_table("audio_zone_players")
    .column("audio_zone_id", |col| col.integer().not_null())
    .column("player_id", |col| col.integer().not_null())
    .execute(&***db)
    .await?;
```

### Creating Audio Zones

To create a new audio zone, construct a `CreateAudioZone` struct with the zone name:

```rust
let new_zone = CreateAudioZone {
    name: "Living Room".to_string(),
};

let created_zone = create_audio_zone(&config_db, &new_zone).await?;
```

### Listing Audio Zones

The `zones()` function retrieves all audio zones from the database:

```rust
let all_zones = zones(&config_db).await?;
for zone in &all_zones {
    println!("  - {} (ID: {}, {} players)", zone.name, zone.id, zone.players.len());
}
```

### Retrieving a Specific Zone

Use `get_zone()` to retrieve a zone by its ID:

```rust
if let Some(zone) = get_zone(&config_db, zone_id).await? {
    println!("Retrieved zone: '{}'", zone.name);
}
```

This returns `Option<AudioZone>`, which is `None` if the zone doesn't exist.

### Updating Audio Zones

To update a zone, create an `UpdateAudioZone` struct with the zone ID and the fields you want to change:

```rust
let update = UpdateAudioZone {
    id: zone_id,
    name: Some("Updated Living Room".to_string()),
    players: None,  // No change to players
};

let updated_zone = update_audio_zone(&config_db, update).await?;
```

Fields set to `None` are not modified in the database.

### Deleting Audio Zones

Delete a zone using `delete_audio_zone()`:

```rust
if let Some(deleted) = delete_audio_zone(&config_db, zone_id).await? {
    println!("Deleted zone: '{}'", deleted.name);
}
```

This returns the deleted zone if it existed, or `None` if the zone wasn't found.

## Key Concepts

### Audio Zones

An **audio zone** represents a logical grouping of audio players that can play synchronized content together. This enables multi-room audio functionality where multiple devices coordinate playback.

### Database Abstraction

The `switchy_database` crate provides a database-agnostic interface. The example uses `SimulationDatabase` for simplicity, but in production you might use:

- `RusqliteDatabase` for SQLite
- `PostgresDatabase` for PostgreSQL
- Other backends as needed

All use the same `Database` trait, making your code portable across database systems.

### ConfigDatabase

The `ConfigDatabase` wrapper provides a convenient way to pass the database reference through your application. It implements `From` traits for easy conversion and `Deref` for transparent database access.

### Error Handling

All audio zone operations return `Result<T, DatabaseFetchError>`, allowing you to handle database errors appropriately:

```rust
match create_audio_zone(&config_db, &new_zone).await {
    Ok(zone) => println!("Created: {}", zone.name),
    Err(e) => eprintln!("Failed to create zone: {}", e),
}
```

### Schema Requirements

The audio zone package expects specific database tables:

- **audio_zones**: Stores zone metadata (id, name)
- **players**: Stores individual audio device information
- **audio_zone_players**: Junction table linking zones to players

Make sure these tables exist before using the audio zone API.

## Testing the Example

1. **Run the example**: Execute the command shown in the "Running the Example" section
2. **Verify output**: Check that all steps complete successfully
3. **Inspect the database**: The example uses an in-memory database, so data is lost when the program exits
4. **Modify the code**: Try changing zone names, creating more zones, or experimenting with the API

## Troubleshooting

### "Config database not initialized" Error

This error occurs if you try to use `ConfigDatabase` without properly initializing it. In this example, we create `ConfigDatabase` directly from a `Database` instance:

```rust
let config_db: ConfigDatabase = db_arc.into();
```

For production applications using the global database pattern, you would call `switchy_database::config::init()` first.

### Database Schema Errors

If you see errors about missing tables or columns, ensure the `create_schema()` function completes successfully before calling audio zone functions. You can add error logging to see what's happening:

```rust
RUST_LOG=debug cargo run --manifest-path packages/audio_zone/examples/basic_usage/Cargo.toml
```

### Compilation Errors

Make sure you're using a recent Rust toolchain. This example requires:

- Rust 2021 edition or later
- Tokio runtime for async execution

## Related Examples

This is currently the only example for `moosicbox_audio_zone`. For related database examples, see:

- Database backend examples in `packages/database/examples/` (if available)
- Other MoosicBox package examples demonstrating database usage
