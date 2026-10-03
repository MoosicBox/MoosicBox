#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database,
    query::{FilterableQuery, where_not_like},
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType},
};

fn column(name: &str, data_type: DataType) -> Column {
    Column {
        name: name.into(),
        data_type,
        nullable: false,
        auto_increment: false,
        default: None,
    }
}

#[tokio::test]
async fn auto_increment_does_not_reuse_deleted_ids() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(connection))]);
    let mut id = column("id", DataType::BigInt);
    id.auto_increment = true;
    db.create_table("events")
        .column(id)
        .column(column("payload", DataType::Text))
        .primary_key("id")
        .execute(&db)
        .await
        .unwrap();
    db.insert("events")
        .value("payload", "first")
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(db.exec_delete_count(&db.delete("events")).await.unwrap(), 1);
    db.insert("events")
        .value("payload", "second")
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        db.select("events")
            .where_eq("id", 2)
            .execute(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    let mut invalid = column("id", DataType::Text);
    invalid.auto_increment = true;
    assert!(matches!(
        db.create_table("invalid")
            .column(invalid)
            .primary_key("id")
            .execute(&db)
            .await,
        Err(switchy_database::DatabaseError::InvalidSchema(_))
    ));
    db.close().await.unwrap();
}

#[tokio::test]
async fn usage_constraints_and_bounded_discovery() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(connection))]);
    db.create_table("usage_meta")
        .column(column("id", DataType::BigInt))
        .check_integer_equals("id", 1)
        .execute(&db)
        .await
        .unwrap();
    db.insert("usage_meta")
        .value("id", 1)
        .execute(&db)
        .await
        .unwrap();
    assert!(
        db.insert("usage_meta")
            .value("id", 2)
            .execute(&db)
            .await
            .is_err()
    );
    db.create_table("usage_rows")
        .column(column("session_id", DataType::Text))
        .column(column("request_key", DataType::Text))
        .column(column("staged", DataType::BigInt))
        .unique_columns(["session_id", "request_key", "staged"])
        .execute(&db)
        .await
        .unwrap();
    for staged in [0, 1] {
        db.insert("usage_rows")
            .value("session_id", "s")
            .value("request_key", "r")
            .value("staged", staged)
            .execute(&db)
            .await
            .unwrap();
    }
    assert!(
        db.insert("usage_rows")
            .value("session_id", "s")
            .value("request_key", "r")
            .value("staged", 1)
            .execute(&db)
            .await
            .is_err()
    );
    let rows = db
        .select("sqlite_master")
        .where_eq("type", "table")
        .where_not_like("name", "sqlite_%")
        .limit(3)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    let rows = db
        .select("sqlite_master")
        .filter(Box::new(where_not_like("name", "%' OR 1=1 --")))
        .limit(1)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
}
