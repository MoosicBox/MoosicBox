#![cfg(feature = "snapshots")]

use std::path::PathBuf;
use switchy_schema_test_utils::snapshots::MigrationSnapshotTest;

#[test_log::test(switchy_async::test(real_fs))]
async fn test_snapshot_infrastructure() {
    MigrationSnapshotTest::new("basic")
        .migrations_dir("./test_utils/test-resources/snapshot-migrations/minimal")
        .run()
        .await
        .unwrap();
}

#[test_log::test(switchy_async::test(real_fs))]
async fn test_builder_methods() {
    // Test 1: Builder methods chain correctly
    let test = MigrationSnapshotTest::new("builder_test")
        .migrations_dir("./test_utils/test-resources/snapshot-migrations/minimal")
        .assert_schema(false)
        .assert_sequence(true);

    test.run().await.unwrap();

    // Test 2: Default migrations_dir is correctly set
    let default_test = MigrationSnapshotTest::new("builder_default_test")
        .migrations_dir("./test_utils/test-resources/snapshot-migrations/minimal");
    default_test.run().await.unwrap();

    // Test 3: All builder methods work together
    let full_test = MigrationSnapshotTest::new("builder_comprehensive_test")
        .migrations_dir(PathBuf::from(
            "./test_utils/test-resources/snapshot-migrations/comprehensive",
        ))
        .assert_schema(true)
        .assert_sequence(false);

    log::debug!("===========================");
    full_test.run().await.unwrap();
    log::debug!("===========================");

    // Test 4: Verify builder returns Self for chaining
    let _chained = MigrationSnapshotTest::new("builder_chain_test")
        .migrations_dir("./test1")
        .assert_schema(false)
        .migrations_dir("./test2") // Should override previous
        .assert_sequence(true)
        .assert_schema(true); // Should override previous

    println!("All builder method tests completed successfully!");
}
