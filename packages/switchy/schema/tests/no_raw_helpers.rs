//! The schema helpers remain usable without arbitrary SQL execution.
#![cfg(not(feature = "raw-sql"))]

use switchy_schema_test_utils::{
    assertions::assert_foreign_key_integrity,
    create_empty_in_memory,
    integration_tests::{
        demonstrate_complex_breakpoint_patterns, demonstrate_rollback_functionality,
    },
};

#[switchy_async::test]
async fn foreign_key_inspection_preserves_unsupported_errors() {
    let db = create_empty_in_memory().await.unwrap();
    assert_foreign_key_integrity(&*db).await.unwrap();
    let unsupported = switchy_schema::checksum_database::ChecksumDatabase::new();
    assert!(matches!(
        assert_foreign_key_integrity(&unsupported).await,
        Err(switchy_database::DatabaseError::UnsupportedOperation(_))
    ));
}

#[switchy_async::test]
async fn sql_migration_demonstrations_fail_closed() {
    assert!(demonstrate_rollback_functionality().await.is_err());
    assert!(demonstrate_complex_breakpoint_patterns().await.is_err());
}
