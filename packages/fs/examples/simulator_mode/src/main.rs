#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use switchy_fs::sync::{create_dir_all, read_to_string, write};

/// Simulates a configuration manager that uses the filesystem
fn save_config(path: &str, content: &str) -> std::io::Result<()> {
    // Create the config directory if needed
    if let Some(parent) = std::path::Path::new(path).parent() {
        create_dir_all(parent)?;
    }

    write(path, content.as_bytes())?;
    println!("Saved config to: {path}");
    Ok(())
}

/// Loads configuration from the filesystem
fn load_config(path: &str) -> std::io::Result<String> {
    let content = read_to_string(path)?;
    println!("Loaded config from: {path}");
    Ok(content)
}

/// Simulates processing user data
fn process_user_data(user_id: u32, data: &str) -> std::io::Result<()> {
    let path = format!("/tmp/users/{user_id}/data.txt");
    save_config(&path, data)?;

    // Verify we can read it back
    let loaded = load_config(&path)?;
    assert_eq!(loaded, data);
    println!("Successfully processed data for user {user_id}");
    Ok(())
}

fn main() -> std::io::Result<()> {
    println!("switchy_fs Simulator Mode Example\n");
    println!(
        "This example demonstrates using the simulator for testing without touching the real filesystem.\n"
    );

    // In simulator mode, reset the filesystem to a clean state
    #[cfg(feature = "simulator")]
    {
        switchy_fs::simulator::reset_fs();
        println!("✓ Simulator filesystem reset to clean state\n");
    }

    // Example 1: Basic simulator usage
    println!("=== Example 1: Basic Simulator Usage ===");
    save_config("/tmp/app/config.json", r#"{"debug": true}"#)?;
    let config = load_config("/tmp/app/config.json")?;
    println!("Config content: {config}\n");

    // Example 2: Testing file operations in isolation
    println!("=== Example 2: Testing in Isolation ===");
    println!("Processing multiple users...");
    process_user_data(1, "Alice's data")?;
    process_user_data(2, "Bob's data")?;
    process_user_data(3, "Charlie's data")?;
    println!();

    // Example 3: Verify isolation from real filesystem
    #[cfg(feature = "simulator")]
    {
        println!("=== Example 3: Simulator Isolation ===");
        // These files only exist in the simulated filesystem
        // They don't actually touch your real disk!
        assert!(switchy_fs::exists("/tmp/app/config.json"));
        assert!(switchy_fs::exists("/tmp/users/1/data.txt"));
        assert!(switchy_fs::exists("/tmp/users/2/data.txt"));
        assert!(switchy_fs::exists("/tmp/users/3/data.txt"));
        println!("✓ All files exist in simulated filesystem");
        println!("✓ No files were created on your real disk\n");
    }

    // Example 4: Reset and clean state
    #[cfg(feature = "simulator")]
    {
        println!("=== Example 4: Reset Simulator ===");
        switchy_fs::simulator::reset_fs();
        println!("✓ Simulator reset");

        // Files no longer exist after reset
        assert!(!switchy_fs::exists("/tmp/app/config.json"));
        assert!(!switchy_fs::exists("/tmp/users/1/data.txt"));
        println!("✓ All files removed from simulated filesystem\n");
    }

    // Example 5: Using with_real_fs for hybrid testing
    #[cfg(feature = "simulator-real-fs")]
    {
        println!("=== Example 5: Hybrid Testing (Real FS within Simulator) ===");

        // First, write to simulator
        write("/tmp/simulator-only.txt", b"This is in the simulator")?;
        println!("Created file in simulator: /tmp/simulator-only.txt");

        // Now use real filesystem temporarily
        switchy_fs::with_real_fs(|| {
            // This directory is created on the REAL filesystem
            create_dir_all("target/real_fs_test").expect("Failed to create real directory");
            write("target/real_fs_test/real-file.txt", b"This is a real file")
                .expect("Failed to write real file");
            println!("Created REAL file: target/real_fs_test/real-file.txt");
        });

        // Back to simulator
        assert!(switchy_fs::exists("/tmp/simulator-only.txt"));
        println!("✓ Simulator file still exists");

        // The real file was created on disk and persists
        let real_content =
            std::fs::read_to_string("target/real_fs_test/real-file.txt").unwrap_or_default();
        println!("✓ Real file persists: {}", !real_content.is_empty());

        // Cleanup real file
        let _ = std::fs::remove_dir_all("target/real_fs_test");
        println!();
    }

    println!("✅ All examples completed successfully!");
    println!("\nKey Benefits:");
    println!("  • No disk I/O - tests run faster");
    println!("  • No cleanup needed - reset with one call");
    println!("  • No file conflicts - each test can use same paths");
    println!("  • No permissions issues - complete control");
    println!("  • Deterministic - no race conditions from disk");

    Ok(())
}
