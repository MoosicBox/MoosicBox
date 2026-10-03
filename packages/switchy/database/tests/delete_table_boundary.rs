#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database, query::FilterableQuery, rusqlite::RusqliteDatabase, schema::DataType,
};

#[tokio::test]
async fn delete_table_boundary_preserves_rows_and_transaction_rollback() {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    let names: &[&str] = if cfg!(feature = "raw-sql") {
        &["items"]
    } else {
        &["items", "table.column", "a\"b", "items WHERE 1=1", "漢字"]
    };
    for name in names {
        db.create_table(name)
            .column(switchy_database::schema::Column {
                name: "id".into(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .execute(&db)
            .await
            .unwrap();
        for id in 1_i64..=3 {
            db.insert(name).value("id", id).execute(&db).await.unwrap();
        }
        assert_eq!(
            db.delete(name)
                .where_eq("id", 1_i64)
                .execute(&db)
                .await
                .unwrap()
                .len(),
            1
        );
        let transaction = db.begin_transaction().await.unwrap();
        assert!(
            transaction
                .delete(name)
                .execute_first(transaction.as_ref())
                .await
                .unwrap()
                .is_some()
        );
        transaction.rollback().await.unwrap();
        assert_eq!(db.select(name).execute(&db).await.unwrap().len(), 2);
        assert_eq!(
            db.delete(name).limit(1).execute(&db).await.unwrap().len(),
            1
        );
        assert_eq!(db.delete(name).execute(&db).await.unwrap().len(), 1);
    }
    db.insert("items")
        .value("id", 9_i64)
        .execute(&db)
        .await
        .unwrap();
    // A table-name fragment must not become a predicate in a no-raw build.
    let result = db.delete("items WHERE id=9").execute(&db).await;
    if cfg!(feature = "raw-sql") {
        assert_eq!(result.unwrap().len(), 1);
    } else {
        assert!(result.is_err());
        assert_eq!(db.select("items").execute(&db).await.unwrap().len(), 1);
    }
    db.close().await.unwrap();
}

#[tokio::test]
async fn insert_quotes_column_delimiters_and_binds_payloads() {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    let column = "value`雪";
    db.create_table("insert_columns")
        .column(switchy_database::schema::Column {
            name: if cfg!(feature = "raw-sql") {
                "\"value`雪\"".into()
            } else {
                column.into()
            },
            data_type: DataType::Text,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .execute(&db)
        .await
        .unwrap();
    let payload = "雪'; DROP TABLE insert_columns; --";
    let row = db
        .insert("insert_columns")
        .value(column, payload)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        row.get(column),
        Some(switchy_database::DatabaseValue::String(payload.into()))
    );
    assert_eq!(
        db.select("insert_columns")
            .execute(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    db.close().await.unwrap();
}
