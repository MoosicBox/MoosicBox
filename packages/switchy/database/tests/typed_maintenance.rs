#![cfg(any(feature = "sqlite-rusqlite", feature = "turso"))]

#[cfg(feature = "sqlite-rusqlite")]
use std::sync::Arc;
use switchy_database::Database;
#[cfg(feature = "sqlite-rusqlite")]
use switchy_database::{DatabaseValue, rusqlite::RusqliteDatabase};

#[cfg(feature = "turso")]
#[tokio::test]
async fn turso_typed_maintenance() {
    let db = switchy_database::turso::TursoDatabase::new(":memory:")
        .await
        .unwrap();
    db.check_connection().await.unwrap();
    db.sqlite_temp_store_memory().await.unwrap();
    let transaction = db.begin_transaction().await.unwrap();
    transaction.sqlite_temp_store_memory().await.unwrap();
    transaction.rollback().await.unwrap();
    assert!(!db.sqlite_integrity_check().await.unwrap().is_empty());
    assert!(matches!(
        db.sqlite_foreign_key_check().await,
        Err(switchy_database::DatabaseError::UnsupportedOperation(_))
    ));
    assert_eq!(db.sqlite_checkpoint_truncate().await.unwrap().len(), 1);
    db.close().await.unwrap();
    assert!(db.check_connection().await.is_err());
}

#[cfg(feature = "sqlite-rusqlite")]
#[tokio::test]
async fn typed_maintenance_preserves_results_and_closed_errors() {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    db.check_connection().await.unwrap();
    db.sqlite_temp_store_memory().await.unwrap();
    let rows = db.sqlite_integrity_check().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("integrity_check"),
        Some(DatabaseValue::String("ok".into()))
    );
    assert!(db.sqlite_foreign_key_check().await.unwrap().is_empty());
    let rows = db.sqlite_checkpoint_truncate().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("busy"), Some(DatabaseValue::Int64(0)));
    db.close().await.unwrap();
    assert!(db.check_connection().await.is_err());
    assert!(db.sqlite_integrity_check().await.is_err());
}
