//! Integration tests for strict mode checksum validation functionality
//!
//! Tests the CLI integration of checksum validation requirements including:
//! - CLI flag behavior
//! - Environment variable support
//! - Configuration priority (CLI > env var)
//! - Warning messages when CLI overrides env var

use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use tempfile::TempDir;

/// Helper to create test migration files
fn create_test_migrations(dir: &TempDir) -> Result<(), std::io::Error> {
    let migrations_dir = dir.path().join("migrations");
    fs::create_dir_all(&migrations_dir)?;

    // Create a simple test migration
    let migration_dir = migrations_dir.join("001_create_users");
    fs::create_dir_all(&migration_dir)?;

    fs::write(
        migration_dir.join("up.sql"),
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);",
    )?;
    fs::write(migration_dir.join("down.sql"), "DROP TABLE users;")?;

    Ok(())
}

/// Helper to create a modified version of test migration (causes checksum mismatch)
fn create_modified_migrations(dir: &TempDir) -> Result<(), std::io::Error> {
    let migrations_dir = dir.path().join("migrations");
    fs::create_dir_all(&migrations_dir)?;

    // Create the same migration but with different content
    let migration_dir = migrations_dir.join("001_create_users");
    fs::create_dir_all(&migration_dir)?;

    fs::write(
        migration_dir.join("up.sql"),
        "CREATE TABLE customers (id INTEGER PRIMARY KEY, name TEXT);", // Different table name!
    )?;
    fs::write(migration_dir.join("down.sql"), "DROP TABLE customers;")?;

    Ok(())
}

#[switchy_async::test(no_simulator)]
async fn test_cli_flag_enables_strict_mode() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    create_test_migrations(&temp_dir).expect("Failed to create migrations");

    let db_path = temp_dir.path().join("test.db");
    let migrations_path = temp_dir.path().join("migrations");

    // First run: establish checksums with regular migration
    cargo_bin_cmd!("switchy-migrate")
        .args([
            "migrate",
            "--database-url",
            &format!("sqlite://{}", db_path.to_string_lossy()),
            "--migrations-dir",
            &migrations_path.to_string_lossy(),
        ])
        .assert()
        .success();

    // Create modified migrations (should cause checksum mismatch)
    create_modified_migrations(&temp_dir).expect("Failed to create modified migrations");

    // Second run: with strict mode enabled should fail due to checksum mismatch
    let output = cargo_bin_cmd!("switchy-migrate")
        .args([
            "migrate",
            "--database-url",
            &format!("sqlite://{}", db_path.to_string_lossy()),
            "--migrations-dir",
            &migrations_path.to_string_lossy(),
            "--require-checksum-validation",
        ])
        .assert()
        .failure();

    // Verify it failed with checksum validation error
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(
        stderr.contains("Checksum validation failed") || stderr.contains("checksum"),
        "Should fail with checksum validation error, got: {}",
        stderr
    );
}

#[switchy_async::test(no_simulator)]
async fn test_env_var_enables_strict_mode() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    create_test_migrations(&temp_dir).expect("Failed to create migrations");

    let db_path = temp_dir.path().join("test.db");
    let migrations_path = temp_dir.path().join("migrations");

    // First run: establish checksums with regular migration
    cargo_bin_cmd!("switchy-migrate")
        .args([
            "migrate",
            "--database-url",
            &format!("sqlite://{}", db_path.to_string_lossy()),
            "--migrations-dir",
            &migrations_path.to_string_lossy(),
        ])
        .assert()
        .success();

    // Create modified migrations (should cause checksum mismatch)
    create_modified_migrations(&temp_dir).expect("Failed to create modified migrations");

    // Second run: with environment variable should fail due to checksum mismatch
    let output = cargo_bin_cmd!("switchy-migrate")
        .env("MIGRATION_REQUIRE_CHECKSUM_VALIDATION", "true")
        .args([
            "migrate",
            "--database-url",
            &format!("sqlite://{}", db_path.to_string_lossy()),
            "--migrations-dir",
            &migrations_path.to_string_lossy(),
        ])
        .assert()
        .failure();

    // Verify it failed with checksum validation error
    let stderr = String::from_utf8_lossy(&output.get_output().stderr);
    assert!(
        stderr.contains("Checksum validation failed") || stderr.contains("checksum"),
        "Should fail with checksum validation error, got: {}",
        stderr
    );
}

#[switchy_async::test(no_simulator)]
async fn test_cli_flag_overrides_env_var() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    create_test_migrations(&temp_dir).expect("Failed to create migrations");

    let db_path = temp_dir.path().join("test.db");
    let migrations_path = temp_dir.path().join("migrations");

    // First run: establish checksums with regular migration
    cargo_bin_cmd!("switchy-migrate")
        .args([
            "migrate",
            "--database-url",
            &format!("sqlite://{}", db_path.to_string_lossy()),
            "--migrations-dir",
            &migrations_path.to_string_lossy(),
        ])
        .assert()
        .success();

    // Create modified migrations (should cause checksum mismatch)
    create_modified_migrations(&temp_dir).expect("Failed to create modified migrations");

    // Run with both CLI flag and env var set
    let output = cargo_bin_cmd!("switchy-migrate")
        .env("MIGRATION_REQUIRE_CHECKSUM_VALIDATION", "true")
        .args([
            "migrate",
            "--database-url",
            &format!("sqlite://{}", db_path.to_string_lossy()),
            "--migrations-dir",
            &migrations_path.to_string_lossy(),
            "--require-checksum-validation",
        ])
        .assert()
        .failure();

    // Verify warning was printed about CLI override (warning goes to stdout)
    let stdout = String::from_utf8_lossy(&output.get_output().stdout);
    assert!(
        stdout.contains("Warning: CLI flag --require-checksum-validation overrides MIGRATION_REQUIRE_CHECKSUM_VALIDATION environment variable"),
        "Should show warning about CLI override in stdout, got: {}", stdout
    );

    let stderr = String::from_utf8_lossy(&output.get_output().stderr);

    // Should still fail due to checksum mismatch (both enable strict mode)
    assert!(
        stderr.contains("Checksum validation failed") || stderr.contains("checksum"),
        "Should fail with checksum validation error, got: {}",
        stderr
    );
}

#[switchy_async::test(no_simulator)]
async fn test_error_message_shows_all_mismatches() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let migrations_dir = temp_dir.path().join("migrations");
    fs::create_dir_all(&migrations_dir).expect("Failed to create migrations dir");

    // Create multiple test migrations
    let migration1_dir = migrations_dir.join("001_create_users");
    fs::create_dir_all(&migration1_dir).expect("Failed to create migration1 dir");
    fs::write(
        migration1_dir.join("up.sql"),
        "CREATE TABLE users (id INTEGER);",
    )
    .expect("Write failed");
    fs::write(migration1_dir.join("down.sql"), "DROP TABLE users;").expect("Write failed");

    let migration2_dir = migrations_dir.join("002_create_posts");
    fs::create_dir_all(&migration2_dir).expect("Failed to create migration2 dir");
    fs::write(
        migration2_dir.join("up.sql"),
        "CREATE TABLE posts (id INTEGER);",
    )
    .expect("Write failed");
    fs::write(migration2_dir.join("down.sql"), "DROP TABLE posts;").expect("Write failed");

    let db_path = temp_dir.path().join("test.db");
    let migrations_path = temp_dir.path().join("migrations");

    // First run: establish checksums
    cargo_bin_cmd!("switchy-migrate")
        .args([
            "migrate",
            "--database-url",
            &format!("sqlite://{}", db_path.to_string_lossy()),
            "--migrations-dir",
            &migrations_path.to_string_lossy(),
        ])
        .assert()
        .success();

    // Modify both migrations to cause multiple mismatches
    fs::write(
        migration1_dir.join("up.sql"),
        "CREATE TABLE customers (id INTEGER);",
    )
    .expect("Write failed"); // Different!
    fs::write(
        migration2_dir.join("up.sql"),
        "CREATE TABLE articles (id INTEGER);",
    )
    .expect("Write failed"); // Different!

    // Run with strict mode - should show multiple mismatches
    let output = cargo_bin_cmd!("switchy-migrate")
        .args([
            "migrate",
            "--database-url",
            &format!("sqlite://{}", db_path.to_string_lossy()),
            "--migrations-dir",
            &migrations_path.to_string_lossy(),
            "--require-checksum-validation",
        ])
        .assert()
        .failure();

    let stderr = String::from_utf8_lossy(&output.get_output().stderr);

    // Verify error message shows multiple mismatches
    assert!(
        stderr.contains("Checksum validation failed"),
        "Should show checksum validation failed, got: {}",
        stderr
    );

    // Should mention both migration IDs that failed
    assert!(
        stderr.contains("001_create_users") && stderr.contains("002_create_posts"),
        "Should mention both failed migrations, got: {}",
        stderr
    );
}
