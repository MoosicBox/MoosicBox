#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database, DatabaseError,
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType},
};

fn open(path: &std::path::Path) -> RusqliteDatabase {
    RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open(path).unwrap(),
    ))])
}

#[tokio::test]
async fn observations_are_connection_affine_non_mutating_and_fail_closed() {
    let path = std::path::PathBuf::from(format!(
        "file:change-observation-{}?mode=memory&cache=shared",
        std::process::id()
    ));
    let db = open(&path);
    db.create_table("items")
        .column(Column {
            name: "id".into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .execute(&db)
        .await
        .unwrap();
    let before = db.sqlite_change_observation().await.unwrap();
    assert_eq!(before, db.sqlite_change_observation().await.unwrap());
    db.insert("items")
        .value("id", 1)
        .execute(&db)
        .await
        .unwrap();
    let local = db.sqlite_change_observation().await.unwrap();
    assert_eq!(local.data_version, before.data_version);
    assert_eq!(local.total_changes, before.total_changes + 1);

    let external = open(&path);
    external
        .insert("items")
        .value("id", 2)
        .execute(&external)
        .await
        .unwrap();
    let remote = db.sqlite_change_observation().await.unwrap();
    assert_ne!(remote.data_version, local.data_version);
    assert_eq!(remote.total_changes, local.total_changes);

    let tx = db.begin_transaction().await.unwrap();
    assert!(matches!(
        db.sqlite_change_observation().await,
        Err(DatabaseError::Busy)
    ));
    tx.rollback().await.unwrap();
    assert_eq!(remote, db.sqlite_change_observation().await.unwrap());
    for commit in [false, true] {
        let before = db.sqlite_change_observation().await.unwrap();
        let tx = db.begin_transaction().await.unwrap();
        assert_eq!(before, tx.sqlite_change_observation().await.unwrap());
        tx.select("items").execute(tx.as_ref()).await.unwrap();
        assert_eq!(before, tx.sqlite_change_observation().await.unwrap());
        tx.insert("items")
            .value("id", 3)
            .execute(tx.as_ref())
            .await
            .unwrap();
        let written = tx.sqlite_change_observation().await.unwrap();
        assert_eq!(written.total_changes, before.total_changes + 1);
        assert_eq!(written.data_version, before.data_version);
        if commit {
            tx.commit().await.unwrap();
        } else {
            tx.rollback().await.unwrap();
        }
        assert_eq!(written, db.sqlite_change_observation().await.unwrap());
    }
    db.close().await.unwrap();
    assert!(matches!(
        db.sqlite_change_observation().await,
        Err(DatabaseError::ConnectionClosed)
    ));

    let pool = RusqliteDatabase::new(
        (0..2)
            .map(|_| {
                Arc::new(switchy_async::sync::Mutex::new(
                    rusqlite::Connection::open(&path).unwrap(),
                ))
            })
            .collect(),
    );
    let tx = pool.begin_transaction().await.unwrap();
    // Even with only one available connection, this is still a multi-connection handle.
    assert!(matches!(
        pool.sqlite_change_observation().await,
        Err(DatabaseError::UnsupportedOperation(_))
    ));
    tx.rollback().await.unwrap();
}
