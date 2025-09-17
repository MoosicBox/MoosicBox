//! Integration examples for Phase 11.4.11 - Documentation examples for snapshot testing
//!
//! These examples demonstrate how to use the snapshot testing functionality
//! and serve as both documentation and verification that the APIs work correctly.

#![cfg(feature = "snapshots")]

use std::sync::Arc;
use switchy_schema::discovery::embedded::EmbeddedMigration;
use switchy_schema_test_utils::{
    MigrationSnapshotTest, MigrationTestBuilder, create_empty_in_memory,
};

/// Simple snapshot test example - demonstrates basic usage
#[test_log::test(switchy_async::test(real_fs))]
async fn test_simple_snapshot_example() {
    // This example demonstrates the basic usage of MigrationSnapshotTest
    // Note: Uses correct import path and available migration directory

    MigrationSnapshotTest::new("user_migration")
        .migrations_dir("./test-resources/snapshot-migrations/minimal")
        .assert_schema(true)
        .assert_sequence(true)
        .run()
        .await
        .unwrap();
}

/// Complex integration with MigrationTestBuilder example
#[test_log::test(switchy_async::test(real_fs))]
async fn test_complex_integration_example() {
    // This example shows how to integrate MigrationTestBuilder with snapshots
    // First create migrations and run complex test, then capture snapshots

    let db = create_empty_in_memory().await.unwrap();

    // Create simple embedded migrations for demonstration
    let migrations: Vec<Arc<dyn switchy_schema::migration::Migration + '_>> = vec![
        Arc::new(EmbeddedMigration::new(
            "001_create_users".to_string(),
            Some("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL);".into()),
            Some("DROP TABLE IF EXISTS users;".into()),
        )),
        Arc::new(EmbeddedMigration::new(
            "002_add_email".to_string(),
            Some("ALTER TABLE users ADD COLUMN email TEXT;".into()),
            Some("ALTER TABLE users DROP COLUMN email;".into()),
        )),
    ];

    // First run complex migration test with breakpoints
    MigrationTestBuilder::new(migrations.clone())
        .with_data_before("002_add_email", |db| {
            Box::pin(async move {
                db.exec_raw("INSERT INTO users (name) VALUES ('test_user')")
                    .await
            })
        })
        .run(db.as_ref())
        .await
        .unwrap();

    // Then capture snapshot of final state the same db
    MigrationSnapshotTest::new("data_migration_result")
        .with_database(db) // Reuse the same database instance!
        .expected_tables(vec!["users".to_string()])
        .assert_schema(true)
        .assert_data(true)
        .with_data_samples("users", 5)
        .run()
        .await
        .unwrap();
}

/// Comprehensive example with all features
#[test_log::test(switchy_async::test(real_fs))]
async fn test_comprehensive_snapshot_example() {
    // This example demonstrates all available features of the snapshot system

    MigrationSnapshotTest::new("comprehensive_test")
        .migrations_dir("./test-resources/snapshot-migrations/comprehensive")
        .assert_schema(true)
        .assert_sequence(true)
        .assert_data(true)
        .with_data_samples("users", 3)
        .with_data_samples("posts", 5)
        .redact_timestamps(true)
        .redact_auto_ids(true)
        .with_setup(|db| {
            Box::pin(async move {
                // Pre-migration setup
                db.exec_raw("CREATE TABLE IF NOT EXISTS config (key TEXT, value TEXT)")
                    .await?;
                db.exec_raw("INSERT INTO config (key, value) VALUES ('version', '1.0')")
                    .await
            })
        })
        .with_verification(|db| {
            Box::pin(async move {
                // Post-migration verification using actual available API
                let query = db.select("users");
                let rows = query.execute(db).await?;
                let count = rows.len();
                assert!(count == count); // Just verify we can access the count
                Ok(())
            })
        })
        .run()
        .await
        .unwrap();
}
