#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database, DatabaseValue,
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType},
};

#[tokio::test]
async fn update_assignment_key_is_not_an_assignment_list() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(connection))]);
    let mut table = db.create_table("items");
    for name in ["id", "other"] {
        table = table.column(Column {
            name: name.into(),
            data_type: DataType::BigInt,
            nullable: false,
            default: None,
            auto_increment: false,
        });
    }
    table.execute(&db).await.unwrap();
    db.insert("items")
        .value("id", 1)
        .value("other", 2)
        .execute(&db)
        .await
        .unwrap();
    db.update("items")
        .value("id", 3)
        .execute(&db)
        .await
        .unwrap();
    let result = db
        .update("items")
        .value("id=99, other", 7)
        .execute(&db)
        .await;
    if cfg!(feature = "raw-sql") {
        assert!(result.is_ok());
    } else {
        assert!(result.is_err());
        let row = db
            .select("items")
            .execute_first(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.get("id"), Some(DatabaseValue::Int64(3)));
        assert_eq!(row.get("other"), Some(DatabaseValue::Int64(2)));
    }
    db.close().await.unwrap();
}

#[cfg(not(feature = "raw-sql"))]
#[tokio::test]
async fn update_assignment_preserves_exact_identifier_and_bound_value() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(connection))]);
    for (index, name) in ["table.column", "a\"b", "id=99, other", "漢字"]
        .into_iter()
        .enumerate()
    {
        let table_name = format!("exact_names_{index}");
        db.create_table(&table_name)
            .column(Column {
                name: name.into(),
                data_type: DataType::Text,
                nullable: false,
                default: None,
                auto_increment: false,
            })
            .execute(&db)
            .await
            .unwrap();
        db.insert(&table_name)
            .value(name, "before")
            .execute(&db)
            .await
            .unwrap();
        let payload = "🦀' , other='not an assignment\0";
        db.update(&table_name)
            .value(name, payload)
            .execute(&db)
            .await
            .unwrap();
        let row = db
            .select(&table_name)
            .execute_first(&db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.get(name), Some(DatabaseValue::String(payload.into())));
        db.drop_table(&table_name).execute(&db).await.unwrap();
    }
    db.close().await.unwrap();
}
