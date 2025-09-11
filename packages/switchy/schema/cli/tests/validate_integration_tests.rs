use assert_cmd::Command;
use insta::assert_snapshot;
use regex::Regex;
use std::path::{Path, PathBuf};
use switchy_fs::TempDir;

/// Get path to test migrations directory
fn load_test_migrations(scenario: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test-resources")
        .join("migrations")
        .join(scenario)
}

/// Strip ANSI color codes for snapshot comparison
fn strip_ansi_codes(text: &str) -> String {
    let re = Regex::new(r"\x1b\[[0-9;]*m").unwrap();
    re.replace_all(text, "").to_string()
}

/// Create a simple test database URL
fn create_test_db_url() -> String {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let url = format!("sqlite://{}", db_path.display());

    // Keep temp_dir alive by leaking it
    std::mem::forget(temp_dir);
    url
}

#[switchy_async::test(no_simulator)]
async fn test_validate_with_valid_migrations() {
    let migrations_dir = load_test_migrations("valid");
    let db_url = create_test_db_url();

    let output = Command::cargo_bin("switchy-migrate")
        .unwrap()
        .args([
            "validate",
            "-d",
            &db_url,
            "-m",
            &migrations_dir.to_string_lossy(),
        ])
        .output()
        .expect("Failed to run command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let clean_output = strip_ansi_codes(&stdout);

    // Filter out paths for consistent snapshots
    let filtered = clean_output
        .replace(
            &migrations_dir.to_string_lossy().to_string(),
            "[MIGRATIONS_DIR]",
        )
        .replace(&db_url, "[DATABASE_URL]");

    assert_snapshot!("validate_with_valid_migrations", filtered);
    // Command should fail because no migration table exists
    assert!(!output.status.success());
}

#[switchy_async::test(no_simulator)]
async fn test_validate_empty_database() {
    let migrations_dir = load_test_migrations("valid");
    let db_url = create_test_db_url();

    let output = Command::cargo_bin("switchy-migrate")
        .unwrap()
        .args([
            "validate",
            "-d",
            &db_url,
            "-m",
            &migrations_dir.to_string_lossy(),
        ])
        .output()
        .expect("Failed to run command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let clean_output = strip_ansi_codes(&stdout);

    // Filter out paths for consistent snapshots
    let filtered = clean_output
        .replace(
            &migrations_dir.to_string_lossy().to_string(),
            "[MIGRATIONS_DIR]",
        )
        .replace(&db_url, "[DATABASE_URL]");

    assert_snapshot!("validate_empty_database_integration", filtered);
    // Command should fail because no migration table exists
    assert!(!output.status.success());
}

#[switchy_async::test(no_simulator)]
async fn test_validate_verbose_mode() {
    let migrations_dir = load_test_migrations("valid");
    let db_url = create_test_db_url();

    let output = Command::cargo_bin("switchy-migrate")
        .unwrap()
        .args([
            "validate",
            "-d",
            &db_url,
            "-m",
            &migrations_dir.to_string_lossy(),
            "--verbose",
        ])
        .output()
        .expect("Failed to run command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let clean_output = strip_ansi_codes(&stdout);

    // Filter out paths for consistent snapshots
    let filtered = clean_output
        .replace(
            &migrations_dir.to_string_lossy().to_string(),
            "[MIGRATIONS_DIR]",
        )
        .replace(&db_url, "[DATABASE_URL]");

    assert_snapshot!("validate_verbose_mode_integration", filtered);
    // Command should fail because no migration table exists
    assert!(!output.status.success());
}
