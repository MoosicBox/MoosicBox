#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database, DatabaseValue,
    query::{FilterableQuery as _, select},
    rusqlite::{RusqliteDatabase, select_on_connection},
    schema::{Column, DataType},
};

#[tokio::test]
async fn borrowed_selection_uses_the_existing_transaction_and_does_not_commit() {
    let connection = Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ));
    let db = RusqliteDatabase::new(vec![Arc::clone(&connection)]);
    db.create_table("attempts")
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
    db.insert("attempts")
        .value("id", 7_i64)
        .execute(&db)
        .await
        .unwrap();
    drop(db);
    let mut guard = connection.lock().await;
    let transaction = guard.transaction().unwrap();
    assert!(!transaction.is_autocommit());
    let query = select("attempts")
        .columns(&["id"])
        .where_eq("id", 7_i64)
        .limit(1);
    let rows = select_on_connection(&transaction, &query).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("id"), Some(DatabaseValue::Int64(7)));
    assert!(!transaction.is_autocommit());
    switchy_database::rusqlite::relax_integer_nullability_on_connection(
        &transaction,
        "attempts",
        "id",
    )
    .unwrap();
    let rows = select_on_connection(&transaction, &query).unwrap();
    assert_eq!(rows[0].get("id"), Some(DatabaseValue::Int64(7)));
    transaction.rollback().unwrap();
    assert!(guard.is_autocommit());
    drop(guard);
    let db = RusqliteDatabase::new(vec![Arc::clone(&connection)]);
    assert!(!db.get_table_columns("attempts").await.unwrap()[0].nullable);
    drop(db);
    let mut guard = connection.lock().await;
    let transaction = guard.transaction().unwrap();
    switchy_database::rusqlite::relax_integer_nullability_on_connection(
        &transaction,
        "attempts",
        "id",
    )
    .unwrap();
    transaction.commit().unwrap();
    drop(guard);
    let db = RusqliteDatabase::new(vec![connection]);
    assert!(db.get_table_columns("attempts").await.unwrap()[0].nullable);
    db.insert("attempts")
        .value("id", DatabaseValue::Null)
        .execute(&db)
        .await
        .unwrap();
}
