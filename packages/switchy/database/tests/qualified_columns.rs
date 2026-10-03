#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database,
    query::{FilterableQuery, qualified_column},
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType},
};

#[tokio::test]
async fn qualified_columns_are_expressions_not_fragments() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(connection))]);
    db.create_table("items")
        .column(Column {
            name: "value".into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .execute(&db)
        .await
        .unwrap();
    db.insert("items")
        .value("value", 7)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        db.select("items")
            .where_eq("value", qualified_column("items", "value"))
            .execute(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        db.select("items")
            .where_eq("value", qualified_column("items", "value"))
            .where_eq("value", 8)
            .execute(&db)
            .await
            .unwrap()
            .is_empty()
    );
    // An unknown qualified column is an error, not a WHERE clause escape.
    for name in ["value OR 1=1 --", "value\" = 7 OR 1=1 --", "items.value"] {
        assert!(
            db.select("items")
                .where_eq("value", qualified_column("items", name))
                .execute(&db)
                .await
                .is_err()
        );
    }
    db.close().await.unwrap();
}
