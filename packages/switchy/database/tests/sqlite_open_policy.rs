//! Connection policy regression using only the public typed database interface.
#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]
use std::{num::NonZeroUsize, time::Duration};
use switchy_database::{Database, DatabaseError, TransactionMode};
use switchy_database_connection::{SqliteAccess, SqliteOpenOptions, open_sqlite_rusqlite};

#[tokio::test]
async fn readonly_no_create_and_immediate_lock_release() {
    let path = std::env::temp_dir().join(format!(
        "switchy-policy-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut options = SqliteOpenOptions {
        access: SqliteAccess::ReadOnly,
        allow_uri: false,
        busy_timeout: Duration::ZERO,
        foreign_keys: true,
        connections: NonZeroUsize::new(1).unwrap(),
    };
    assert!(open_sqlite_rusqlite(&path, &options).is_err());
    assert!(!path.exists());
    options.access = SqliteAccess::ReadWriteCreate;
    let first = open_sqlite_rusqlite(&path, &options).unwrap();
    options.access = SqliteAccess::ReadWrite;
    let second = open_sqlite_rusqlite(&path, &options).unwrap();
    let tx = first
        .begin_transaction_with_mode(TransactionMode::Immediate)
        .await
        .unwrap();
    assert!(matches!(
        second
            .begin_transaction_with_mode(TransactionMode::Immediate)
            .await,
        Err(DatabaseError::Busy | DatabaseError::Locked)
    ));
    drop(tx);
    let tx = second
        .begin_transaction_with_mode(TransactionMode::Immediate)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    first.close().await.unwrap();
    second.close().await.unwrap();
    options.access = SqliteAccess::ReadOnly;
    let reader = open_sqlite_rusqlite(&path, &options).unwrap();
    assert!(
        reader
            .create_table("forbidden")
            .column(switchy_database::schema::Column {
                name: "id".into(),
                data_type: switchy_database::schema::DataType::BigInt,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .execute(&reader)
            .await
            .is_err()
    );
    reader.close().await.unwrap();
    std::fs::remove_file(path).unwrap();
}
