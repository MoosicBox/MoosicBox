#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic usage example for `moosicbox_config`
//!
//! This example demonstrates:
//! - Getting configuration directory paths
//! - Creating configuration directories
//! - Setting a custom root directory
//! - Working with profile directories
//! - Working with cache directories

use moosicbox_config::{
    AppType, get_app_config_dir_path, get_cache_dir_path, get_config_dir_path,
    get_profile_dir_path, make_cache_dir_path, make_config_dir_path, make_profile_dir_path,
    set_root_dir,
};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== MoosicBox Config - Basic Usage Example ===\n");

    // Example 1: Get the default config directory path
    println!("1. Default Configuration Directory:");
    if let Some(config_dir) = get_config_dir_path() {
        println!("   Config directory: {}", config_dir.display());
    } else {
        println!("   Could not determine config directory");
    }
    println!();

    // Example 2: Set a custom root directory for testing
    println!("2. Setting Custom Root Directory:");
    let temp_root = std::env::temp_dir().join("moosicbox_example");
    println!("   Setting root to: {}", temp_root.display());
    set_root_dir(temp_root.clone());

    if let Some(config_dir) = get_config_dir_path() {
        println!("   New config directory: {}", config_dir.display());
    }
    println!();

    // Example 3: Get app-specific config directories
    println!("3. App-Specific Configuration Directories:");
    for app_type in [AppType::App, AppType::Server, AppType::Local] {
        if let Some(app_dir) = get_app_config_dir_path(app_type) {
            println!("   {:?}: {}", app_type, app_dir.display());
        }
    }
    println!();

    // Example 4: Create config directory
    println!("4. Creating Configuration Directory:");
    if let Some(config_dir) = make_config_dir_path() {
        println!("   Created/verified: {}", config_dir.display());
        println!("   Exists: {}", config_dir.exists());
    } else {
        println!("   Failed to create config directory");
    }
    println!();

    // Example 5: Work with profile directories
    println!("5. Profile Directories:");
    let profile_names = ["default", "production", "development"];

    for profile_name in &profile_names {
        if let Some(profile_dir) = get_profile_dir_path(AppType::Server, profile_name) {
            println!("   {}: {}", profile_name, profile_dir.display());
        }
    }
    println!();

    // Example 6: Create a profile directory
    println!("6. Creating Profile Directory:");
    if let Some(profile_dir) = make_profile_dir_path(AppType::Server, "example_profile") {
        println!("   Created/verified: {}", profile_dir.display());
        println!("   Exists: {}", profile_dir.exists());

        // Demonstrate where a config file would be located
        let config_file = profile_dir.join("config.json5");
        println!("   Config file would be at: {}", config_file.display());
    } else {
        println!("   Failed to create profile directory");
    }
    println!();

    // Example 7: Work with cache directory
    println!("7. Cache Directory:");
    if let Some(cache_dir) = get_cache_dir_path() {
        println!("   Cache directory: {}", cache_dir.display());
    }

    if let Some(cache_dir) = make_cache_dir_path() {
        println!("   Created/verified: {}", cache_dir.display());
        println!("   Exists: {}", cache_dir.exists());
    }
    println!();

    // Example 8: Demonstrate typical directory structure
    println!("8. Example Directory Structure:");
    println!("   ~/.local/moosicbox/                (or custom root)");
    println!("   ├── server/");
    println!("   │   ├── config.json5              (global server config)");
    println!("   │   └── profiles/");
    println!("   │       ├── default/");
    println!("   │       │   └── config.json5      (profile-specific config)");
    println!("   │       └── production/");
    println!("   │           └── config.json5");
    println!("   ├── app/");
    println!("   │   └── config.json5");
    println!("   └── cache/                         (shared cache directory)");
    println!();

    // Example 9: Show how to construct complete paths
    println!("9. Complete Path Construction:");
    if let Some(server_dir) = get_app_config_dir_path(AppType::Server) {
        let global_config = server_dir.join("config.json5");
        println!("   Global config path: {}", global_config.display());
    }

    if let Some(profile_dir) = get_profile_dir_path(AppType::Server, "production") {
        let profile_config = profile_dir.join("config.json5");
        println!("   Profile config path: {}", profile_config.display());
    }
    println!();

    println!("=== Example Complete ===");
    println!();
    println!("Next steps:");
    println!("- Create config.json5 files in the directories shown above");
    println!("- See the package README for JSON5 configuration examples");
    println!("- Use moosicbox_config::file module to load configurations");

    // Clean up the temporary directory we created
    cleanup_temp_directory(&temp_root)?;

    Ok(())
}

/// Clean up temporary directories created during the example
fn cleanup_temp_directory(temp_root: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    if temp_root.exists() {
        std::fs::remove_dir_all(temp_root)?;
        println!("\nCleaned up temporary directory: {}", temp_root.display());
    }
    Ok(())
}
