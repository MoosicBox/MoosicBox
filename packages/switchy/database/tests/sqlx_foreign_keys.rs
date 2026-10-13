#![cfg(all(feature = "sqlite-sqlx", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database,
    schema::{Column, DataType},
    sqlx::sqlite::SqliteSqlxDatabase,
};

#[tokio::test]
async fn typed_foreign_keys_enforce_constraints_and_preserve_metadata() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let db = SqliteSqlxDatabase::new(Arc::new(switchy_async::sync::Mutex::new(pool)));
    db.sqlite_foreign_keys(false).await.unwrap();
    db.sqlite_foreign_keys(true).await.unwrap();
    let column = |name: &str| Column {
        name: name.into(),
        data_type: DataType::BigInt,
        nullable: false,
        auto_increment: false,
        default: None,
    };
    db.create_table("parent")
        .column(column("id"))
        .primary_key("id")
        .execute(&db)
        .await
        .unwrap();
    db.create_table("child")
        .column(column("parent_id"))
        .foreign_key_column("parent_id", "parent", "id")
        .execute(&db)
        .await
        .unwrap();
    assert!(db.column_exists("parent", "id").await.unwrap());
    assert!(!db.column_exists("parent", "missing").await.unwrap());
    assert!(
        db.insert("child")
            .value("parent_id", 1_i64)
            .execute(&db)
            .await
            .is_err()
    );
    db.insert("parent")
        .value("id", 1_i64)
        .execute(&db)
        .await
        .unwrap();
    db.insert("child")
        .value("parent_id", 1_i64)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(db.sqlite_foreign_key_check().await.unwrap(), []);
    db.close().await.unwrap();
}
