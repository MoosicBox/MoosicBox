use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use switchy_fs::{TempDir, sync::read_dir_sorted};

#[switchy_async::test(no_simulator)]
async fn test_cli_help_command() {
    let mut cmd = cargo_bin_cmd!("switchy-migrate");
    cmd.arg("--help");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "A CLI tool for managing database schema migrations",
        ))
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("migrate"))
        .stdout(predicate::str::contains("rollback"));
}

#[switchy_async::test(no_simulator)]
async fn test_cli_version_command() {
    let mut cmd = cargo_bin_cmd!("switchy-migrate");
    cmd.arg("--version");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("switchy-migrate"));
}

#[switchy_async::test(no_simulator)]
async fn test_create_command_help() {
    let mut cmd = cargo_bin_cmd!("switchy-migrate");
    cmd.args(["create", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Create a new migration file"))
        .stdout(predicate::str::contains("--migrations-dir"));
}

#[switchy_async::test(no_simulator)]
async fn test_status_command_help() {
    let mut cmd = cargo_bin_cmd!("switchy-migrate");
    cmd.args(["status", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Show migration status"))
        .stdout(predicate::str::contains("--database-url"))
        .stdout(predicate::str::contains("--migrations-dir"))
        .stdout(predicate::str::contains("--migration-table"));
}

#[switchy_async::test(no_simulator)]
async fn test_migrate_command_help() {
    let mut cmd = cargo_bin_cmd!("switchy-migrate");
    cmd.args(["migrate", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Run pending migrations"))
        .stdout(predicate::str::contains("--database-url"))
        .stdout(predicate::str::contains("--migrations-dir"))
        .stdout(predicate::str::contains("--up-to"))
        .stdout(predicate::str::contains("--steps"))
        .stdout(predicate::str::contains("--dry-run"));
}

#[switchy_async::test(no_simulator)]
async fn test_rollback_command_help() {
    let mut cmd = cargo_bin_cmd!("switchy-migrate");
    cmd.args(["rollback", "--help"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Rollback migrations"))
        .stdout(predicate::str::contains("--database-url"))
        .stdout(predicate::str::contains("--to"))
        .stdout(predicate::str::contains("--steps"))
        .stdout(predicate::str::contains("--dry-run"));
}

#[switchy_async::test(no_simulator)]
async fn test_create_migration_file() {
    let temp_dir = TempDir::new().unwrap();
    let migrations_dir = temp_dir.path().join("migrations");

    let mut cmd = cargo_bin_cmd!("switchy-migrate");
    cmd.args([
        "create",
        "test_migration",
        "--migrations-dir",
        migrations_dir.to_str().unwrap(),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created migration:"));

    // Verify migration directory was created (not individual files)
    let migration_dirs: Vec<_> = read_dir_sorted(&migrations_dir)
        .unwrap()
        .into_iter()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();

    assert_eq!(migration_dirs.len(), 1);
    assert!(migration_dirs[0].contains("test_migration"));
    assert!(migration_dirs[0].len() > 20); // Should have timestamp prefix
}

#[switchy_async::test(no_simulator)]
async fn test_status_command_missing_database_url() {
    let mut cmd = cargo_bin_cmd!("switchy-migrate");
    cmd.args(["status"]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("database-url"));
}

#[switchy_async::test(no_simulator)]
async fn test_migrate_command_missing_database_url() {
    let mut cmd = cargo_bin_cmd!("switchy-migrate");
    cmd.args(["migrate"]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("database-url"));
}

#[switchy_async::test(no_simulator)]
async fn test_rollback_command_missing_database_url() {
    let mut cmd = cargo_bin_cmd!("switchy-migrate");
    cmd.args(["rollback"]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("database-url"));
}

#[switchy_async::test(no_simulator)]
async fn test_create_migration_with_custom_dir() {
    let temp_dir = TempDir::new().unwrap();
    let custom_migrations_dir = temp_dir.path().join("custom_migrations");

    let mut cmd = cargo_bin_cmd!("switchy-migrate");
    cmd.args([
        "create",
        "custom_dir_test",
        "--migrations-dir",
        custom_migrations_dir.to_str().unwrap(),
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created migration:"));

    // Verify migration directory was created in custom directory
    let migration_dirs: Vec<_> = read_dir_sorted(&custom_migrations_dir)
        .unwrap()
        .into_iter()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();

    assert_eq!(migration_dirs.len(), 1);
    assert!(migration_dirs[0].contains("custom_dir_test"));
}

#[switchy_async::test(no_simulator)]
async fn test_invalid_command() {
    let mut cmd = cargo_bin_cmd!("switchy-migrate");
    cmd.arg("invalid-command");

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[switchy_async::test(no_simulator)]
async fn test_environment_variable_support() {
    let temp_dir = TempDir::new().unwrap();
    let migrations_dir = temp_dir.path().join("env_migrations");

    let mut cmd = cargo_bin_cmd!("switchy-migrate");
    cmd.env("SWITCHY_MIGRATIONS_DIR", migrations_dir.to_str().unwrap());
    cmd.args(["create", "env_test"]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Created migration:"));

    // Verify migration directory was created using environment variable
    let migration_dirs: Vec<_> = read_dir_sorted(&migrations_dir)
        .unwrap()
        .into_iter()
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .collect();

    assert_eq!(migration_dirs.len(), 1);
    assert!(migration_dirs[0].contains("env_test"));
}
