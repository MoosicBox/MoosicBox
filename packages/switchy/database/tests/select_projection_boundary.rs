#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database,
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType},
};

#[tokio::test]
async fn select_projection_and_table_respect_raw_boundary() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(connection))]);
    db.create_table("items")
        .column(Column {
            name: "id".into(),
            data_type: DataType::BigInt,
            nullable: false,
            default: None,
            auto_increment: false,
        })
        .execute(&db)
        .await
        .unwrap();
    db.insert("items")
        .value("id", 7)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        db.select("items")
            .columns(&["id"])
            .execute(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        db.select("items")
            .execute_first(&db)
            .await
            .unwrap()
            .is_some()
    );
    for table in ["items WHERE 1=0", "items AS other"] {
        let rows = db.select(table).execute(&db).await;
        let first = db.select(table).execute_first(&db).await;
        if cfg!(feature = "raw-sql") {
            assert!(rows.is_ok());
            assert!(first.is_ok());
        } else {
            assert!(rows.is_err());
            assert!(first.is_err());
        }
    }
    // SQLite may interpret an unknown double-quoted identifier as a string.
    // Whether it errors or returns that literal, it must not evaluate SQL.
    for projection in ["id + 1", "id AS renamed"] {
        let rows = db.select("items").columns(&[projection]).execute(&db).await;
        let first = db
            .select("items")
            .columns(&[projection])
            .execute_first(&db)
            .await;
        if cfg!(feature = "raw-sql") {
            assert!(rows.is_ok());
            assert!(first.is_ok());
        } else {
            for row in rows
                .into_iter()
                .flatten()
                .chain(first.into_iter().flatten())
            {
                assert_eq!(
                    row.columns[0].1,
                    switchy_database::DatabaseValue::String(projection.into())
                );
            }
        }
    }
    db.close().await.unwrap();
}
