#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Basic database migration example for `moosicbox_schema`
//!
//! This example demonstrates how to:
//! 1. Initialize a `SQLite` database connection
//! 2. Run configuration migrations
//! 3. Run library migrations
//! 4. Handle migration errors
//! 5. Query the migration tracking table to verify results

use moosicbox_schema::{MigrateError, migrate_config, migrate_library};
use switchy_database::Database;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging to see migration progress
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    println!("Starting MoosicBox Schema Migration Example");
    println!("===========================================\n");

    // Step 1: Initialize SQLite database connection
    println!("Step 1: Initializing SQLite database connection...");
    let db = switchy_database_connection::init_sqlite_sqlx(None).await?;
    println!("✓ Database connection established\n");

    // Step 2: Run configuration migrations
    println!("Step 2: Running configuration migrations...");
    match migrate_config(&*db).await {
        Ok(()) => println!("✓ Configuration migrations completed successfully\n"),
        Err(e) => {
            eprintln!("✗ Configuration migrations failed: {e}");
            return Err(e.into());
        }
    }

    // Step 3: Run library migrations
    println!("Step 3: Running library migrations...");
    match migrate_library(&*db).await {
        Ok(()) => println!("✓ Library migrations completed successfully\n"),
        Err(e) => {
            eprintln!("✗ Library migrations failed: {e}");
            return Err(e.into());
        }
    }

    // Step 4: Verify migrations by querying the tracking table
    println!("Step 4: Verifying migrations...");
    display_migration_status(&*db).await?;

    println!("\n===========================================");
    println!("Migration example completed successfully!");

    Ok(())
}

/// Display the status of all migrations from the tracking table
async fn display_migration_status(db: &dyn Database) -> Result<(), MigrateError> {
    // Query the migration tracking table
    let migrations = db
        .select("__moosicbox_schema_migrations")
        .columns(&["id", "status", "run_on", "finished_on"])
        .sort("run_on", switchy_database::query::SortDirection::Asc)
        .execute(db)
        .await?;

    println!("Found {} tracked migrations:", migrations.len());

    // Count migrations by status
    let mut completed = 0;
    let mut failed = 0;
    let mut in_progress = 0;

    for migration in &migrations {
        if let Some(status_value) = migration.get("status")
            && let Some(status) = status_value.as_str()
        {
            match status {
                "completed" => completed += 1,
                "failed" => failed += 1,
                "in_progress" => in_progress += 1,
                _ => {}
            }
        }
    }

    println!("  - Completed: {completed}");
    println!("  - Failed: {failed}");
    println!("  - In Progress: {in_progress}");

    // Show the most recent 5 migrations
    if !migrations.is_empty() {
        println!("\nMost recent migrations:");

        for migration in migrations.iter().rev().take(5) {
            let id = migration
                .get("id")
                .and_then(|v| v.as_str().map(String::from));
            let status = migration
                .get("status")
                .and_then(|v| v.as_str().map(String::from));

            if let (Some(id), Some(status)) = (id, status) {
                let status_symbol = match status.as_str() {
                    "completed" => "✓",
                    "failed" => "✗",
                    "in_progress" => "⋯",
                    _ => "?",
                };
                println!("  {status_symbol} {id} [{status}]");
            }
        }
    }

    Ok(())
}
