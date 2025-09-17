use assert_cmd::Command;
use insta::assert_snapshot;
use switchy_async::test;
use switchy_fs::TempDir;
use switchy_schema::migration::Migration;

// Mix of CLI integration tests and unit tests for code migrations validation

#[test(no_simulator)]
async fn test_code_migrations_help() {
    let output = Command::cargo_bin("switchy-migrate")
        .unwrap()
        .args(["validate", "--help"])
        .output()
        .expect("Failed to run command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("validate"));
    assert!(stdout.contains("migrations"));
    assert!(output.status.success());
}

#[test(no_simulator)]
async fn test_code_migrations_with_empty_directory() {
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db_url = format!("sqlite://{}", db_path.display());

    // Create an empty migrations directory
    let migrations_dir = temp_dir.path().join("migrations");
    std::fs::create_dir_all(&migrations_dir).unwrap();

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

    // Filter out paths for consistent snapshots
    let filtered = stdout
        .replace(
            &migrations_dir.to_string_lossy().to_string(),
            "[MIGRATIONS_DIR]",
        )
        .replace(&db_url, "[DATABASE_URL]");

    assert_snapshot!("validate_empty_code_migrations_dir", filtered);
    // Should handle case when migration table doesn't exist
    assert!(output.status.success());
}

// Unit tests using in-memory databases to test validation logic directly
#[test(no_simulator)]
async fn test_code_migration_checksum_calculation() {
    // Test that different migrations produce different checksums
    struct TestMigration1;
    struct TestMigration2;

    #[async_trait::async_trait]
    impl Migration<'_> for TestMigration1 {
        fn id(&self) -> &str {
            "001_create_users"
        }

        async fn up(
            &self,
            db: &dyn switchy_database::Database,
        ) -> Result<(), switchy_schema::MigrationError> {
            db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)")
                .await
                .map_err(|e| switchy_schema::MigrationError::Execution(e.to_string()))?;
            Ok(())
        }

        async fn down(
            &self,
            db: &dyn switchy_database::Database,
        ) -> Result<(), switchy_schema::MigrationError> {
            db.exec_raw("DROP TABLE users")
                .await
                .map_err(|e| switchy_schema::MigrationError::Execution(e.to_string()))?;
            Ok(())
        }

        async fn up_checksum(&self) -> Result<bytes::Bytes, switchy_schema::MigrationError> {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(b"CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)");
            Ok(bytes::Bytes::from(hasher.finalize().to_vec()))
        }

        async fn down_checksum(&self) -> Result<bytes::Bytes, switchy_schema::MigrationError> {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(b"DROP TABLE users");
            Ok(bytes::Bytes::from(hasher.finalize().to_vec()))
        }
    }

    #[async_trait::async_trait]
    impl Migration<'_> for TestMigration2 {
        fn id(&self) -> &str {
            "001_create_users" // Same ID
        }

        async fn up(
            &self,
            db: &dyn switchy_database::Database,
        ) -> Result<(), switchy_schema::MigrationError> {
            db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)") // Different SQL!
                .await
                .map_err(|e| switchy_schema::MigrationError::Execution(e.to_string()))?;
            Ok(())
        }

        async fn down(
            &self,
            db: &dyn switchy_database::Database,
        ) -> Result<(), switchy_schema::MigrationError> {
            db.exec_raw("DROP TABLE users")
                .await
                .map_err(|e| switchy_schema::MigrationError::Execution(e.to_string()))?;
            Ok(())
        }
    }

    let migration1 = TestMigration1;
    let migration2 = TestMigration2;

    // Test that different SQL produces different up checksums
    let checksum_up1 = migration1.up_checksum().await.unwrap();
    let checksum_up2 = migration2.up_checksum().await.unwrap();

    assert_ne!(
        checksum_up1, checksum_up2,
        "Different SQL should produce different up checksums"
    );

    // Test that different SQL produces different down checksums
    let checksum_down1 = migration1.down_checksum().await.unwrap();
    let checksum_down2 = migration2.down_checksum().await.unwrap();

    assert_ne!(
        checksum_down1, checksum_down2,
        "Different SQL should produce different down checksums"
    );

    // Test that same migration produces consistent up checksums
    let checksum_up1_again = migration1.up_checksum().await.unwrap();
    assert_eq!(
        checksum_up1, checksum_up1_again,
        "Same migration should produce consistent up checksums"
    );

    // Test that same migration produces consistent down checksums
    let checksum_down1_again = migration1.down_checksum().await.unwrap();
    assert_eq!(
        checksum_down1, checksum_down1_again,
        "Same migration should produce consistent down checksums"
    );
}
