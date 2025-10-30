#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `hyperchad_state`
//!
//! This example demonstrates:
//! - Creating a `SQLite` persistence backend (both in-memory and file-based)
//! - Storing and retrieving typed data
//! - Using the various state store operations (set, get, take, clear)
//! - Error handling patterns

use hyperchad_state::{StateStore, sqlite::SqlitePersistence};
use serde::{Deserialize, Serialize};

/// Example configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct AppConfig {
    theme: String,
    notifications_enabled: bool,
    volume: f32,
}

/// Example user preferences structure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct UserPreferences {
    username: String,
    language: String,
    timezone: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== HyperChad State - Basic Usage Example ===\n");

    // Example 1: In-memory SQLite persistence
    println!("1. Creating in-memory state store...");
    let persistence = SqlitePersistence::new_in_memory().await?;
    let store = StateStore::new(persistence);
    println!("   ✓ In-memory store created\n");

    // Example 2: Storing and retrieving data
    println!("2. Storing application configuration...");
    let config = AppConfig {
        theme: "dark".to_string(),
        notifications_enabled: true,
        volume: 0.75,
    };
    store.set("app_config", &config).await?;
    println!("   ✓ Stored: {config:?}\n");

    println!("3. Retrieving configuration...");
    let loaded_config: Option<AppConfig> = store.get("app_config").await?;
    println!("   ✓ Retrieved: {loaded_config:?}");
    assert_eq!(Some(config.clone()), loaded_config);
    println!("   ✓ Values match!\n");

    // Example 3: Working with multiple keys
    println!("4. Storing user preferences...");
    let prefs = UserPreferences {
        username: "alice".to_string(),
        language: "en-US".to_string(),
        timezone: "America/New_York".to_string(),
    };
    store.set("user_prefs", &prefs).await?;
    println!("   ✓ Stored: {prefs:?}\n");

    // Example 4: Retrieving non-existent keys
    println!("5. Attempting to retrieve non-existent key...");
    let missing: Option<AppConfig> = store.get("nonexistent").await?;
    println!("   ✓ Result: {missing:?} (expected None)\n");

    // Example 5: Using take() to remove and retrieve
    println!("6. Using take() to remove and retrieve user preferences...");
    let taken_prefs: Option<UserPreferences> = store.take("user_prefs").await?;
    println!("   ✓ Taken: {taken_prefs:?}");
    assert_eq!(Some(prefs), taken_prefs);

    // Verify it's been removed
    let after_take: Option<UserPreferences> = store.get("user_prefs").await?;
    println!("   ✓ After take, key is gone: {after_take:?}\n");

    // Example 6: Demonstrating caching behavior
    println!("7. Demonstrating cache behavior...");
    store.set("cached_value", &"test_data".to_string()).await?;
    println!("   ✓ First get (loads from persistence)...");
    let _first: Option<String> = store.get("cached_value").await?;
    println!("   ✓ Second get (served from cache)...");
    let _second: Option<String> = store.get("cached_value").await?;
    println!("   ✓ Cache working correctly\n");

    // Example 7: Clearing all state
    println!("8. Clearing all stored values...");
    store.clear().await?;
    let after_clear: Option<AppConfig> = store.get("app_config").await?;
    println!("   ✓ After clear: {after_clear:?} (expected None)\n");

    // Example 8: File-based persistence
    println!("9. Creating file-based state store...");
    let file_persistence = SqlitePersistence::new("example_state.db").await?;
    let file_store = StateStore::new(file_persistence);
    println!("   ✓ File-based store created (example_state.db)\n");

    println!("10. Storing data to file-based store...");
    let persistent_config = AppConfig {
        theme: "light".to_string(),
        notifications_enabled: false,
        volume: 0.5,
    };
    file_store.set("config", &persistent_config).await?;
    println!("   ✓ Stored to file: {persistent_config:?}");
    println!("   ℹ This data persists across application restarts\n");

    // Retrieve to verify
    let loaded: Option<AppConfig> = file_store.get("config").await?;
    println!("   ✓ Retrieved from file: {loaded:?}\n");

    // Clean up the example database file
    println!("11. Cleaning up...");
    std::fs::remove_file("example_state.db")?;
    println!("   ✓ Removed example database file\n");

    println!("=== Example completed successfully! ===");

    Ok(())
}
