//! Example demonstrating temporary directory functionality.
//!
//! This example shows how to use `switchy_fs` temporary directory features including:
//! * Creating basic temporary directories that auto-cleanup on drop
//! * Using custom prefixes for temp directory names
//! * Keeping temp directories (preventing automatic cleanup)
//! * Manually closing temp directories for immediate cleanup

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use switchy_fs::{TempDir, tempdir};

fn main() -> std::io::Result<()> {
    println!("Demo: switchy_fs temp_dir functionality");

    // Example 1: Basic temp directory creation
    {
        println!("\n1. Basic temp directory creation:");
        let temp_dir = tempdir()?;
        let path = temp_dir.path();
        println!("Created temp directory at: {}", path.display());

        // Create a file in the temp directory (in real mode)
        #[cfg(all(feature = "std", not(feature = "simulator")))]
        {
            use std::io::Write;

            let file_path = path.join("example.txt");
            let mut file = switchy_fs::sync::OpenOptions::new()
                .create(true)
                .write(true)
                .open(&file_path)?;
            writeln!(file, "Hello from switchy_fs temp directory!")?;
            println!("Created file: {}", file_path.display());
        }

        #[cfg(feature = "simulator")]
        {
            println!("In simulator mode, directory exists in simulated filesystem");
        }

        // Directory will be cleaned up when temp_dir is dropped
    }

    // Example 2: Temp directory with prefix
    {
        println!("\n2. Temp directory with prefix:");
        let temp_dir = TempDir::with_prefix("my-app-")?;
        println!("Created temp directory at: {}", temp_dir.path().display());

        #[cfg(feature = "simulator")]
        {
            let file_name = temp_dir.path().file_name().unwrap().to_string_lossy();
            println!(
                "Directory name starts with prefix: {}",
                file_name.starts_with("my-app-")
            );
        }
    }

    // Example 3: Keeping a temp directory (preventing cleanup)
    {
        println!("\n3. Keeping a temp directory:");
        let temp_dir = tempdir()?;
        let path = temp_dir.path().to_path_buf();
        println!("Created temp directory at: {}", path.display());

        // Keep the directory (prevent automatic cleanup)
        let kept_path = temp_dir.keep();
        println!("Kept directory at: {}", kept_path.display());

        #[cfg(all(feature = "std", not(feature = "simulator")))]
        {
            println!("Directory still exists: {}", kept_path.exists());
            // Manual cleanup since we kept it
            switchy_fs::sync::remove_dir_all(kept_path)?;
            println!("Manually cleaned up kept directory");
        }

        #[cfg(feature = "simulator")]
        {
            println!("In simulator mode, directory exists in simulated filesystem");
        }
    }

    // Example 4: Manual close
    {
        println!("\n4. Manual close:");
        let temp_dir = tempdir()?;
        println!("Created temp directory at: {}", temp_dir.path().display());

        // Manually close (clean up immediately)
        temp_dir.close()?;
        println!("Manually closed temp directory");
    }

    println!("\nDemo completed!");
    Ok(())
}
