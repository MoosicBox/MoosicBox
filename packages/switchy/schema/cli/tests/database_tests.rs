use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;
use switchy_fs::{
    TempDir,
    sync::{OpenOptions, create_dir_all},
};

#[switchy_async::test(no_simulator)]
async fn test_status_with_sqlite_empty_database() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let migrations_dir = temp_dir.path().join("migrations");
    create_dir_all(&migrations_dir).unwrap();

    let database_url = format!("sqlite://{}", db_path.display());

    let mut cmd = Command::cargo_bin("switchy-migrate").unwrap();
    cmd.args([
        "status",
        "--database-url",
        &database_url,
        "--migrations-dir",
        migrations_dir.to_str().unwrap(),
    ]);

    // Should succeed but show no migrations
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No migrations found"));
}

#[switchy_async::test(no_simulator)]
async fn test_create_and_status_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("workflow.db");
    let migrations_dir = temp_dir.path().join("migrations");

    let database_url = format!("sqlite://{}", db_path.display());

    // Step 1: Create a migration
    let mut create_cmd = Command::cargo_bin("switchy-migrate").unwrap();
    create_cmd.args([
        "create",
        "initial_schema",
        "--migrations-dir",
        migrations_dir.to_str().unwrap(),
    ]);

    create_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("Created migration:"));

    // Step 2: Check status shows pending migration
    let mut status_cmd = Command::cargo_bin("switchy-migrate").unwrap();
    status_cmd.args([
        "status",
        "--database-url",
        &database_url,
        "--migrations-dir",
        migrations_dir.to_str().unwrap(),
    ]);

    status_cmd
        .assert()
        .success()
        .stdout(predicate::str::contains("initial_schema"))
        .stdout(predicate::str::contains("pending").or(predicate::str::contains("unapplied")));
}

#[switchy_async::test(no_simulator)]
async fn test_migrate_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("dryrun.db");
    let migrations_dir = temp_dir.path().join("migrations");
    create_dir_all(&migrations_dir).unwrap();

    // Create a migration directory with proper structure
    let migration_id = "001_create_users";
    let migration_dir = migrations_dir.join(migration_id);
    create_dir_all(&migration_dir).unwrap();

    // Create up.sql
    let up_content = "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL);";
    let mut up_file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(migration_dir.join("up.sql"))
        .unwrap();
    up_file.write_all(up_content.as_bytes()).unwrap();

    // Create down.sql
    let down_content = "DROP TABLE users;";
    let mut down_file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(migration_dir.join("down.sql"))
        .unwrap();
    down_file.write_all(down_content.as_bytes()).unwrap();

    let database_url = format!("sqlite://{}", db_path.display());

    let mut cmd = Command::cargo_bin("switchy-migrate").unwrap();
    cmd.args([
        "migrate",
        "--database-url",
        &database_url,
        "--migrations-dir",
        migrations_dir.to_str().unwrap(),
        "--dry-run",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("dry run").or(predicate::str::contains("Dry run")))
        .stdout(predicate::str::contains("001_create_users"));
}

#[switchy_async::test(no_simulator)]
async fn test_invalid_database_url() {
    let temp_dir = TempDir::new().unwrap();
    let migrations_dir = temp_dir.path().join("migrations");
    create_dir_all(&migrations_dir).unwrap();

    let mut cmd = Command::cargo_bin("switchy-migrate").unwrap();
    cmd.args([
        "status",
        "--database-url",
        "invalid://database/url",
        "--migrations-dir",
        migrations_dir.to_str().unwrap(),
    ]);

    cmd.assert().failure().stderr(
        predicate::str::contains("Error")
            .or(predicate::str::contains("Unsupported database scheme")),
    );
}

#[switchy_async::test(no_simulator)]
async fn test_missing_migrations_directory() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let nonexistent_dir = temp_dir.path().join("nonexistent");

    let database_url = format!("sqlite://{}", db_path.display());

    let mut cmd = Command::cargo_bin("switchy-migrate").unwrap();
    cmd.args([
        "status",
        "--database-url",
        &database_url,
        "--migrations-dir",
        nonexistent_dir.to_str().unwrap(),
    ]);

    // Should fail with error when directory doesn't exist
    cmd.assert().failure().stderr(
        predicate::str::contains("No such file or directory")
            .or(predicate::str::contains("NotFound")),
    );
}

#[switchy_async::test(no_simulator)]
async fn test_rollback_dry_run() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("rollback.db");
    let migrations_dir = temp_dir.path().join("migrations");
    create_dir_all(&migrations_dir).unwrap();

    let database_url = format!("sqlite://{}", db_path.display());

    let mut cmd = Command::cargo_bin("switchy-migrate").unwrap();
    cmd.args([
        "rollback",
        "--database-url",
        &database_url,
        "--migrations-dir",
        migrations_dir.to_str().unwrap(),
        "--steps",
        "1",
        "--dry-run",
    ]);

    // Should not fail even with no applied migrations in dry-run mode
    cmd.assert().success().stdout(
        predicate::str::contains("dry run")
            .or(predicate::str::contains("Dry run"))
            .or(predicate::str::contains("no migrations")),
    );
}

#[switchy_async::test(no_simulator)]
async fn test_custom_migration_table() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("custom_table.db");
    let migrations_dir = temp_dir.path().join("migrations");
    create_dir_all(&migrations_dir).unwrap();

    let database_url = format!("sqlite://{}", db_path.display());

    let mut cmd = Command::cargo_bin("switchy-migrate").unwrap();
    cmd.args([
        "status",
        "--database-url",
        &database_url,
        "--migrations-dir",
        migrations_dir.to_str().unwrap(),
        "--migration-table",
        "custom_migrations",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("No migrations found"));
}
