//! Integration tests for checksum functionality across all migration types

use bytes::Bytes;
use switchy_async::test as async_test;
use switchy_database::Database;
use switchy_schema::checksum_database::ChecksumDatabase;

#[cfg(all(feature = "embedded", feature = "code"))]
#[async_test]
async fn test_all_migration_types_async_flow() {
    use std::sync::Arc;

    use switchy_schema::{
        discovery::{code::CodeMigration, embedded::EmbeddedMigration},
        migration::Migration,
    };

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

#[cfg(feature = "embedded")]
#[async_test]
async fn test_migration_checksum_stability() {
    use std::sync::Arc;

    use switchy_schema::{discovery::embedded::EmbeddedMigration, migration::Migration as _};

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

#[cfg(feature = "embedded")]
#[async_test]
async fn test_different_content_produces_different_checksums() {
    use std::sync::Arc;

    use switchy_schema::{discovery::embedded::EmbeddedMigration, migration::Migration as _};

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

// ============================================================================
// Complex Transaction Flow Integration Tests
// ============================================================================

#[async_test]
async fn test_complex_transaction_flows_produce_stable_checksums() {
    // Test Case 1: Deep nesting with mixed outcomes
    async fn create_deep_mixed_pattern() -> Bytes {
        let db = ChecksumDatabase::new();

        // Begin TX1
        let tx1 = db.begin_transaction().await.unwrap();
        tx1.exec_raw("Operation A").await.unwrap();

        // Begin TX2 (nested in TX1)
        let tx2 = tx1.begin_transaction().await.unwrap();
        tx2.exec_raw("Operation B").await.unwrap();

        // Begin TX3 (nested in TX2)
        let tx3 = tx2.begin_transaction().await.unwrap();
        tx3.exec_raw("Operation C").await.unwrap();
        tx3.commit().await.unwrap(); // Commit TX3

        tx2.exec_raw("Operation D").await.unwrap();

        // Begin TX4 (nested in TX2)
        let tx4 = tx2.begin_transaction().await.unwrap();
        tx4.exec_raw("Operation E").await.unwrap();
        tx4.rollback().await.unwrap(); // Rollback TX4

        tx2.commit().await.unwrap(); // Commit TX2

        tx1.exec_raw("Operation F").await.unwrap();
        tx1.commit().await.unwrap(); // Commit TX1

        db.finalize().await
    }

    // Test Case 2: Interleaved operations with depth changes
    async fn create_interleaved_pattern() -> Bytes {
        let db = ChecksumDatabase::new();

        // Operation at depth 0
        db.exec_raw("ROOT_OP_1").await.unwrap();

        let tx1 = db.begin_transaction().await.unwrap();
        tx1.exec_raw("TX1_OP_1").await.unwrap();

        let tx2 = tx1.begin_transaction().await.unwrap();
        tx2.exec_raw("TX2_OP_1").await.unwrap();

        let tx3 = tx2.begin_transaction().await.unwrap();
        tx3.exec_raw("TX3_OP_1").await.unwrap();
        tx3.commit().await.unwrap();

        // Back to TX2 level
        tx2.exec_raw("TX2_OP_2").await.unwrap();
        tx2.commit().await.unwrap();

        // Back to TX1 level
        tx1.exec_raw("TX1_OP_2").await.unwrap();
        tx1.commit().await.unwrap();

        // Back to root level
        db.exec_raw("ROOT_OP_2").await.unwrap();

        db.finalize().await
    }

    // Run each pattern multiple times to verify stability
    let pattern1_checksums = [
        create_deep_mixed_pattern().await,
        create_deep_mixed_pattern().await,
        create_deep_mixed_pattern().await,
    ];

    let pattern2_checksums = [
        create_interleaved_pattern().await,
        create_interleaved_pattern().await,
        create_interleaved_pattern().await,
    ];

    // Verify each pattern produces stable checksums
    assert_eq!(
        pattern1_checksums[0], pattern1_checksums[1],
        "Deep mixed pattern should produce stable checksums"
    );
    assert_eq!(
        pattern1_checksums[1], pattern1_checksums[2],
        "Deep mixed pattern should be stable across all runs"
    );

    assert_eq!(
        pattern2_checksums[0], pattern2_checksums[1],
        "Interleaved pattern should produce stable checksums"
    );
    assert_eq!(
        pattern2_checksums[1], pattern2_checksums[2],
        "Interleaved pattern should be stable across all runs"
    );

    // Verify different patterns produce different checksums
    assert_ne!(
        pattern1_checksums[0], pattern2_checksums[0],
        "Different complex patterns should produce different checksums"
    );

    // Verify all checksums are valid SHA256 (32 bytes)
    assert_eq!(pattern1_checksums[0].len(), 32);
    assert_eq!(pattern2_checksums[0].len(), 32);
}
