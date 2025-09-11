use assert_cmd::Command;
use insta::assert_snapshot;
use regex::Regex;
use std::path::Path;
use switchy_fs::TempDir;

/// Get path to test migrations directory
fn load_test_migrations(scenario: &str) -> std::path::PathBuf {
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

/// Create a simple test database URL for testing
fn create_test_db_url() -> String {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let url = format!("sqlite://{}", db_path.display());

    // Keep temp_dir alive by leaking it
    std::mem::forget(temp_dir);
    url
}

#[test]
fn test_validate_command_help() {
    let mut cmd = Command::cargo_bin("switchy-migrate").unwrap();
    cmd.args(["validate", "--help"]);

    let output = cmd.output().expect("Failed to run command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let clean_output = strip_ansi_codes(&stdout);

    assert_snapshot!("validate_command_help", clean_output);
    assert!(output.status.success());
}

#[test]
fn test_validate_empty_migrations_directory() {
    let migrations_dir = load_test_migrations("empty");
    let db_url = create_test_db_url();

    let mut cmd = Command::cargo_bin("switchy-migrate").unwrap();
    cmd.args([
        "validate",
        "-d",
        &db_url,
        "-m",
        &migrations_dir.to_string_lossy(),
    ]);

    let output = cmd.output().expect("Failed to run command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Filter out paths and URLs for consistent snapshots
    let filtered = stdout
        .replace(
            &migrations_dir.to_string_lossy().to_string(),
            "[MIGRATIONS_DIR]",
        )
        .replace(&db_url, "[DATABASE_URL]");
    let clean_output = strip_ansi_codes(&filtered);

    // Print stderr for debugging
    if !output.status.success() {
        println!("Command failed with stderr: {}", stderr);
    }

    assert_snapshot!("validate_empty_migrations", clean_output);
    // Empty migrations directory with no migration table should fail
    // This is expected behavior since there's no __switchy_migrations table
    assert!(!output.status.success());
}

#[test]
fn test_validate_with_verbose_flag() {
    let migrations_dir = load_test_migrations("empty");
    let db_url = create_test_db_url();

    let mut cmd = Command::cargo_bin("switchy-migrate").unwrap();
    cmd.args([
        "validate",
        "-d",
        &db_url,
        "-m",
        &migrations_dir.to_string_lossy(),
        "--verbose",
    ]);

    let output = cmd.output().expect("Failed to run command");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Filter out paths and URLs for consistent snapshots
    let filtered = stdout
        .replace(
            &migrations_dir.to_string_lossy().to_string(),
            "[MIGRATIONS_DIR]",
        )
        .replace(&db_url, "[DATABASE_URL]");
    let clean_output = strip_ansi_codes(&filtered);

    assert_snapshot!("validate_empty_migrations_verbose", clean_output);
    // Should fail because migration table doesn't exist
    assert!(!output.status.success());
}

#[test]
fn test_validate_with_strict_flag() {
    let migrations_dir = load_test_migrations("empty");
    let db_url = create_test_db_url();

    let mut cmd = Command::cargo_bin("switchy-migrate").unwrap();
    cmd.args([
        "validate",
        "-d",
        &db_url,
        "-m",
        &migrations_dir.to_string_lossy(),
        "--strict",
    ]);

    let output = cmd.output().expect("Failed to run command");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Filter out paths and URLs for consistent snapshots
    let filtered = stdout
        .replace(
            &migrations_dir.to_string_lossy().to_string(),
            "[MIGRATIONS_DIR]",
        )
        .replace(&db_url, "[DATABASE_URL]");
    let clean_output = strip_ansi_codes(&filtered);

    assert_snapshot!("validate_empty_migrations_strict", clean_output);
    // Should fail because migration table doesn't exist
    assert!(!output.status.success());
}

#[switchy_async::test(no_simulator)]
async fn test_validate_nonexistent_database() {
    let migrations_dir = load_test_migrations("empty");

    let mut cmd = Command::cargo_bin("switchy-migrate").unwrap();
    cmd.args([
        "validate",
        "-d",
        "sqlite:///nonexistent/path/test.db",
        "-m",
        &migrations_dir.to_string_lossy(),
    ]);

    let output = cmd.output().expect("Failed to run command");
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Filter out the specific path for consistent snapshots
    let filtered = stderr.replace(
        &migrations_dir.to_string_lossy().to_string(),
        "[MIGRATIONS_DIR]",
    );
    let clean_output = strip_ansi_codes(&filtered);

    assert_snapshot!("validate_nonexistent_database", clean_output);
    assert!(!output.status.success()); // Should fail
}

#[switchy_async::test(no_simulator)]
async fn test_validate_nonexistent_migrations_directory() {
    let db_url = create_test_db_url();

    let mut cmd = Command::cargo_bin("switchy-migrate").unwrap();
    cmd.args(["validate", "-d", &db_url, "-m", "/nonexistent/migrations"]);

    let output = cmd.output().expect("Failed to run command");
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Filter out the database URL for consistent snapshots
    let filtered = stderr.replace(&db_url, "[DATABASE_URL]");
    let clean_output = strip_ansi_codes(&filtered);

    assert_snapshot!("validate_nonexistent_migrations_dir", clean_output);
    assert!(!output.status.success()); // Should fail
}
