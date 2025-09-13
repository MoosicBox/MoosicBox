#![cfg(feature = "snapshots")]

use switchy_schema_test_utils::snapshots::MigrationSnapshotTest;

#[test]
fn test_snapshot_infrastructure() {
    MigrationSnapshotTest::new("basic").run().unwrap();
}
