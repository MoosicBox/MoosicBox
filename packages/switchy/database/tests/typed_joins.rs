#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database,
    query::{join_columns, qualified_column},
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType},
};

#[tokio::test]
async fn typed_join_preserves_equality_and_left_join_semantics() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(connection))]);
    for table in ["parents", "children"] {
        db.create_table(table)
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
    }
    for id in [1, 2] {
        db.insert("parents")
            .value("id", id)
            .execute(&db)
            .await
            .unwrap();
    }
    db.insert("children")
        .value("id", 1)
        .execute(&db)
        .await
        .unwrap();
    for (left, expected) in [(false, 1), (true, 2)] {
        let rows = db
            .select("parents")
            .joins(vec![join_columns(
                "children",
                qualified_column("parents", "id"),
                qualified_column("children", "id"),
                left,
            )])
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(rows.len(), expected);
    }
    for name in ["id OR 1=1 --", "id\" = 1 OR 1=1 --", "children.id"] {
        assert!(
            db.select("parents")
                .joins(vec![join_columns(
                    "children",
                    qualified_column("parents", "id"),
                    qualified_column("children", name),
                    false
                )])
                .execute(&db)
                .await
                .is_err()
        );
    }
    for table in ["children ON 1=1 --", "children\" ON 1=1 --"] {
        assert!(
            db.select("parents")
                .joins(vec![join_columns(
                    table,
                    qualified_column("parents", "id"),
                    qualified_column("children", "id"),
                    false,
                )])
                .execute(&db)
                .await
                .is_err()
        );
    }
    db.close().await.unwrap();
}
