#![cfg(all(feature = "sqlite-sqlx", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database, DatabaseValue,
    schema::{Column, DataType},
    sqlx::sqlite::SqliteSqlxDatabase,
};

#[tokio::test]
async fn string_defaults_are_data() {
    check("items", "value", DataType::Text).await;
}

#[cfg(not(feature = "raw-sql"))]
#[tokio::test]
async fn schema_names_are_exact_identifiers() {
    check(
        "items.dot\"; --",
        "column.dot\" DEFAULT 9 --",
        DataType::Custom("TEXT); DROP TABLE sentinel; --".into()),
    )
    .await;
}

async fn check(table: &str, column: &str, data_type: DataType) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let db = SqliteSqlxDatabase::new(Arc::new(switchy_async::sync::Mutex::new(pool)));
    let default = "é'); DROP TABLE sentinel; --";
    db.create_table(table)
        .column(Column {
            name: "id".into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: column.into(),
            data_type,
            nullable: false,
            auto_increment: false,
            default: Some(DatabaseValue::String(default.into())),
        })
        .execute(&db)
        .await
        .unwrap();
    assert!(db.table_exists(table).await.unwrap());
    let row = db.insert(table).value("id", 1).execute(&db).await.unwrap();
    assert_eq!(row.get(column), Some(DatabaseValue::String(default.into())));
    let explicit = "漢字'); DROP TABLE sentinel; --";
    let row = db
        .insert(table)
        .value("id", 2)
        .value(column, explicit)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        row.get(column),
        Some(DatabaseValue::String(explicit.into()))
    );
    db.close().await.unwrap();
}
