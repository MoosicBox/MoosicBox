//! SQL discovery remains inspectable without granting SQL execution.
#![cfg(all(feature = "directory", feature = "embedded", not(feature = "raw-sql")))]

use bytes::Bytes;
use switchy_schema::{
    MigrationError,
    checksum_database::ChecksumDatabase,
    discovery::{directory::FileMigration, embedded::EmbeddedMigration},
    migration::Migration,
};

#[switchy_async::test]
async fn sql_discovery_rejects_execution_without_raw_capability() {
    let db = ChecksumDatabase::new();
    let file = FileMigration::new(
        "file".into(),
        "/unused".into(),
        Some("SELECT 1".into()),
        Some("SELECT 2".into()),
    );
    let embedded = EmbeddedMigration::new(
        "embedded".into(),
        Some(Bytes::from_static(b"SELECT 1")),
        Some(Bytes::from_static(b"SELECT 2")),
    );
    for migration in [&file as &dyn Migration<'static>, &embedded] {
        assert!(matches!(
            migration.up(&db).await,
            Err(MigrationError::RawSqlDisabled)
        ));
        assert!(matches!(
            migration.down(&db).await,
            Err(MigrationError::RawSqlDisabled)
        ));
        assert_eq!(migration.up_checksum().await.unwrap().len(), 32);
        assert_eq!(migration.down_checksum().await.unwrap().len(), 32);
    }
}

#[switchy_async::test]
async fn empty_discovery_preserves_no_op_behavior() {
    let db = ChecksumDatabase::new();
    let file = FileMigration::new("file".into(), "/unused".into(), Some("  ".into()), None);
    let embedded = EmbeddedMigration::new("embedded".into(), Some(Bytes::new()), None);
    for migration in [&file as &dyn Migration<'static>, &embedded] {
        migration.up(&db).await.unwrap();
        migration.down(&db).await.unwrap();
    }
}
