//! Multi-row mutation names must remain identifiers, including on conflict updates.
#![cfg(all(feature = "sqlite-sqlx", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database, DatabaseValue,
    query::identifier,
    schema::{Column, DataType},
    sqlx::sqlite::SqliteSqlxDatabase,
};

#[tokio::test]
async fn bulk_upsert_ordinary_names() {
    check("items", "id", "value").await;
}

#[cfg(not(feature = "raw-sql"))]
#[tokio::test]
async fn bulk_upsert_delimiter_names() {
    check("items.dot\"; --", "id.dot\"", "value\" = 7 --").await;
}

async fn check(table: &str, key: &str, value: &str) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let db = SqliteSqlxDatabase::new(Arc::new(switchy_async::sync::Mutex::new(pool)));
    db.create_table(table)
        .columns(
            vec![key, value]
                .into_iter()
                .map(|name| Column {
                    name: name.into(),
                    data_type: DataType::BigInt,
                    nullable: false,
                    auto_increment: false,
                    default: None,
                })
                .collect(),
        )
        .unique_columns([key])
        .execute(&db)
        .await
        .unwrap();
    for number in [2_i64, 3] {
        db.upsert_multi(table)
            .values(vec![vec![(key, 1_i64), (value, number)]])
            .unique(vec![Box::new(identifier(key))])
            .execute(&db)
            .await
            .unwrap();
    }
    let rows = db
        .select(table)
        .columns(&[value])
        .execute(&db)
        .await
        .unwrap();
    let row = db
        .select(table)
        .columns(&[value])
        .execute_first(&db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.get(value), Some(DatabaseValue::Int64(3)));
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get(value), Some(DatabaseValue::Int64(3)));
    db.close().await.unwrap();
}
