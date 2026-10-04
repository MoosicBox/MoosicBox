//! Disposable spools use only typed operations and release files before cleanup.
#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use switchy_database::{
    Database, DatabaseError,
    schema::{Column, DataType},
};
use switchy_database_connection::open_sqlite_rusqlite_spool;

#[tokio::test]
async fn disposable_spool_releases_before_directory_cleanup() {
    let directory = std::env::temp_dir().join(format!(
        "switchy-spool-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("spool.sqlite");
    let db = open_sqlite_rusqlite_spool(&path).unwrap();
    db.create_table("items")
        .column(Column {
            name: "id".into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .primary_key("id")
        .execute(&db)
        .await
        .unwrap();
    db.insert("items")
        .value("id", 1)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(db.select("items").execute(&db).await.unwrap().len(), 1);
    db.close().await.unwrap();
    assert!(matches!(
        db.select("items").execute(&db).await,
        Err(DatabaseError::ConnectionClosed)
    ));
    // Reopening and reading verifies successful close did not merely hide a live lease.
    let reopened = open_sqlite_rusqlite_spool(&path).unwrap();
    assert_eq!(
        reopened
            .select("items")
            .execute(&reopened)
            .await
            .unwrap()
            .len(),
        1
    );
    reopened.close().await.unwrap();
    std::fs::remove_file(path).unwrap();
    std::fs::remove_dir(directory).unwrap();
}
