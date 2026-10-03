#![cfg(all(feature = "sqlite-rusqlite", not(feature = "raw-sql")))]

use std::sync::Arc;
use switchy_database::{
    Database,
    query::{FilterableQuery, identifier},
    rusqlite::RusqliteDatabase,
};

#[tokio::test]
async fn identifier_expression_is_not_sql() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            "CREATE TABLE items (\"name OR 1=1 --\" INTEGER); INSERT INTO items VALUES (7);",
        )
        .unwrap();
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(connection))]);
    let rows = db
        .select("items")
        .where_eq(identifier("name OR 1=1 --"), 7)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    let rows = db
        .select("items")
        .where_eq(identifier("name OR 1=1 --"), 8)
        .execute(&db)
        .await
        .unwrap();
    assert!(rows.is_empty());
}
