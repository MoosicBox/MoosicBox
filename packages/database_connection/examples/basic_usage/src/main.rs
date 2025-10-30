#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::path::Path;

use switchy_database_connection::{Credentials, init};

/// Demonstrates basic database connection initialization.
///
/// This example shows how to:
/// 1. Initialize an in-memory `SQLite` database
/// 2. Initialize a file-based `SQLite` database
/// 3. Parse credentials from a URL
/// 4. Handle common initialization errors
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Enable logging to see what's happening
    pretty_env_logger::init();

    println!("=== Database Connection Examples ===\n");

    // Example 1: In-memory SQLite database
    println!("1. Initializing in-memory SQLite database...");
    match init(None, None).await {
        Ok(_db) => {
            println!("   ✓ In-memory database initialized successfully");
            println!(
                "   This database exists only in memory and will be lost when the program exits.\n"
            );
        }
        Err(e) => {
            eprintln!("   ✗ Failed to initialize in-memory database: {e}");
            return Err(e.into());
        }
    }

    // Example 2: File-based SQLite database
    println!("2. Initializing file-based SQLite database...");
    let db_path = Path::new("./example_database.db");
    match init(Some(db_path), None).await {
        Ok(_db) => {
            println!("   ✓ File-based database initialized successfully");
            println!("   Database file created at: {}", db_path.display());
            println!("   This database persists on disk.\n");
        }
        Err(e) => {
            eprintln!("   ✗ Failed to initialize file-based database: {e}");
            return Err(e.into());
        }
    }

    // Example 3: Parsing credentials from a URL
    println!("3. Parsing database credentials from URL...");
    let db_url = "postgres://myuser:mypassword@localhost:5432/mydb";
    match Credentials::from_url(db_url) {
        Ok(creds) => {
            println!("   ✓ Credentials parsed successfully:");
            println!("     - Host: {}", creds.host());
            println!("     - Database: {}", creds.name());
            println!("     - User: {}", creds.user());
            println!(
                "     - Password: {}",
                if creds.password().is_some() {
                    "***"
                } else {
                    "(none)"
                }
            );
            println!(
                "   Note: This example uses SQLite, so PostgreSQL credentials won't be used.\n"
            );
        }
        Err(e) => {
            eprintln!("   ✗ Failed to parse credentials: {e}");
            return Err(e.into());
        }
    }

    // Example 4: Creating credentials manually
    println!("4. Creating credentials manually...");
    let creds = Credentials::new(
        "database.example.com".to_string(),
        "production_db".to_string(),
        "app_user".to_string(),
        Some("secure_password".to_string()),
    );
    println!("   ✓ Credentials created:");
    println!("     - Host: {}", creds.host());
    println!("     - Database: {}", creds.name());
    println!("     - User: {}", creds.user());
    println!("     - Has password: {}\n", creds.password().is_some());

    // Example 5: Error handling demonstration
    println!("5. Demonstrating error handling...");
    println!("   Testing invalid URL format...");
    match Credentials::from_url("invalid-url-without-protocol") {
        Ok(_) => println!("   ✗ Expected error but got success"),
        Err(e) => println!("   ✓ Caught expected error: {e}"),
    }

    println!("\n=== All examples completed successfully ===");
    println!("\nNext steps:");
    println!("  - To use PostgreSQL, enable postgres-sqlx or postgres-raw features");
    println!("  - To use TLS, add postgres-native-tls or postgres-openssl features");
    println!("  - To use AWS SSM for credentials, enable the 'creds' feature");
    println!("  - See the README.md for comprehensive documentation");

    // Clean up the example database file
    if db_path.exists() {
        if let Err(e) = std::fs::remove_file(db_path) {
            eprintln!("\nWarning: Failed to clean up example database file: {e}");
        } else {
            println!("\nCleaned up example database file.");
        }
    }

    Ok(())
}
