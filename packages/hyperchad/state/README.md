# HyperChad State

A state management package for the HyperChad framework, providing in-memory caching with optional persistent storage backends.

## Overview

`hyperchad_state` provides a `StateStore` that combines in-memory caching with pluggable persistence backends. Currently supports SQLite persistence.

## What it provides

- **StateStore** - Main interface for get/set/remove operations
- **In-memory caching** - Fast access using `BTreeMap` with `RwLock`
- **SQLite persistence** - Store state in SQLite database
- **Generic type support** - Works with any `Serialize` + `DeserializeOwned` types
- **Async operations** - All operations are async

## Basic usage

```rust
use hyperchad_state::{StateStore, persistence::sqlite::SqlitePersistence};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct Settings {
    theme: String,
    volume: f32,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create SQLite backend
    let persistence = SqlitePersistence::new("state.db").await?;
    let store = StateStore::new(persistence);

    // Store and retrieve data
    let settings = Settings { theme: "dark".to_string(), volume: 0.8 };
    store.set("settings", &settings).await?;

    let loaded: Settings = store.get("settings").await?.unwrap();
    println!("Theme: {}", loaded.theme);

    Ok(())
}
```

## StateStore API

- `set<T>(key, value) -> Result<(), Error>` - Store a value
- `get<T>(key) -> Result<Option<T>, Error>` - Retrieve a value
- `remove(key) -> Result<(), Error>` - Remove a value
- `take<T>(key) -> Result<Option<T>, Error>` - Remove and return a value
- `clear() -> Result<(), Error>` - Clear all values

## Persistence backends

### SQLite

```rust
// File-based database
let persistence = SqlitePersistence::new("state.db").await?;

// In-memory database
let persistence = SqlitePersistence::new_in_memory().await?;
```

Creates a `state` table with `key` and `value` columns.

## Features

- `default` - Enables `persistence-ios` and `persistence-sqlite`
- `persistence-sqlite` - Enable SQLite backend
- `persistence-ios` - Enable iOS-specific persistence (stub)
- `fail-on-warnings` - Treat warnings as errors

## Error types

```rust
pub enum Error {
    Database(switchy::database::DatabaseError),     // SQLite errors
    InitDb(switchy::database_connection::InitDbError), // DB init errors
    InvalidDbConfiguration,                         // Invalid DB config
    Serde(serde_json::Error),                      // JSON serialization errors
}
```

## Dependencies

- `switchy` - Database abstraction (optional, for SQLite)
- `serde` - Serialization framework
- `serde_json` - JSON serialization
- `async-trait` - Async trait support
- `thiserror` - Error handling
- `log` - Logging
- `moosicbox_assert` - Assertion utilities

## Related

- [`hyperchad`](../README.md) - Core HyperChad framework
- [`switchy_database`](../../database/README.md) - Database abstraction layer
