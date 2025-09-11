//! Integration tests for checksum functionality across all migration types

use bytes::Bytes;
use std::sync::Arc;
use switchy_async::test as async_test;
use switchy_schema::discovery::code::CodeMigration;
use switchy_schema::discovery::embedded::EmbeddedMigration;
use switchy_schema::migration::Migration;

#[async_test]
async fn test_all_migration_types_async_flow() {
    // Create migrations of each type
    let embedded = Arc::new(EmbeddedMigration::new(
        "embedded_test".to_string(),
        Some(Bytes::from(
            "CREATE TABLE embedded_test (id INTEGER PRIMARY KEY)",
        )),
        Some(Bytes::from("DROP TABLE embedded_test")),
    ));

    let code = Arc::new(CodeMigration::new(
        "code_test".to_string(),
        Box::new("CREATE TABLE code_test (id INTEGER PRIMARY KEY)".to_string()),
        Some(Box::new("DROP TABLE code_test".to_string())),
    ));

    // Collect all migrations
    let migrations: Vec<Arc<dyn Migration<'static>>> = vec![embedded, code];

    // Run migrations and capture checksums
    let mut up_checksums = Vec::new();
    let mut down_checksums = Vec::new();

    for migration in &migrations {
        let up_checksum = migration.up_checksum().await.unwrap();
        let down_checksum = migration.down_checksum().await.unwrap();

        up_checksums.push(up_checksum);
        down_checksums.push(down_checksum);
    }

    // Verify all produce valid, unique checksums
    for (i, checksum) in up_checksums.iter().enumerate() {
        assert_eq!(checksum.len(), 32, "Up checksum {} should be 32 bytes", i);
        assert_ne!(
            checksum,
            &Bytes::from(vec![0u8; 32]),
            "Up checksum {} should not be all zeros",
            i
        );
    }

    for (i, checksum) in down_checksums.iter().enumerate() {
        assert_eq!(checksum.len(), 32, "Down checksum {} should be 32 bytes", i);
        assert_ne!(
            checksum,
            &Bytes::from(vec![0u8; 32]),
            "Down checksum {} should not be all zeros",
            i
        );
    }

    // Verify different migration types produce different checksums
    if up_checksums.len() > 1 {
        assert_ne!(
            up_checksums[0], up_checksums[1],
            "Different migration types should produce different up checksums"
        );
    }

    if down_checksums.len() > 1 {
        assert_ne!(
            down_checksums[0], down_checksums[1],
            "Different migration types should produce different down checksums"
        );
    }

    // Test that same migration produces consistent checksums across calls
    let embedded_checksum1 = migrations[0].up_checksum().await.unwrap();
    let embedded_checksum2 = migrations[0].up_checksum().await.unwrap();
    assert_eq!(
        embedded_checksum1, embedded_checksum2,
        "Same migration should produce identical checksums across calls"
    );
}

#[async_test]
async fn test_migration_checksum_stability() {
    // Test that checksums are stable across multiple async calls
    let migration = Arc::new(EmbeddedMigration::new(
        "stability_test".to_string(),
        Some(Bytes::from("CREATE TABLE stability_test (id INTEGER)")),
        Some(Bytes::from("DROP TABLE stability_test")),
    ));

    let mut checksums = Vec::new();

    // Calculate checksum multiple times in async context
    for _ in 0..5 {
        let checksum = migration.up_checksum().await.unwrap();
        checksums.push(checksum);
    }

    // All checksums should be identical
    for (i, checksum) in checksums.iter().enumerate().skip(1) {
        assert_eq!(
            checksums[0], *checksum,
            "Checksum {} should match first checksum (stability test)",
            i
        );
    }

    // Verify checksum is valid SHA256 (32 bytes)
    assert_eq!(checksums[0].len(), 32);
}

#[async_test]
async fn test_different_content_produces_different_checksums() {
    // Test that small changes in migration content produce different checksums
    let migration1 = Arc::new(EmbeddedMigration::new(
        "content_test".to_string(),
        Some(Bytes::from("CREATE TABLE test1 (id INTEGER)")),
        None,
    ));

    let migration2 = Arc::new(EmbeddedMigration::new(
        "content_test".to_string(),                           // Same ID
        Some(Bytes::from("CREATE TABLE test2 (id INTEGER)")), // Different content
        None,
    ));

    let checksum1 = migration1.up_checksum().await.unwrap();
    let checksum2 = migration2.up_checksum().await.unwrap();

    assert_ne!(
        checksum1, checksum2,
        "Different content should produce different checksums even with same ID"
    );
}
