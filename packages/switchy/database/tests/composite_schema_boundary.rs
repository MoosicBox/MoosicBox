#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database,
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType},
};

fn column(name: &str) -> Column {
    Column {
        name: name.into(),
        data_type: DataType::Int,
        nullable: false,
        auto_increment: false,
        default: None,
    }
}

#[tokio::test]
async fn composite_keys_and_explicit_references_preserve_order_and_exact_names() {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    db.create_table("parent")
        .column(column("event_seq"))
        .primary_key("event_seq")
        .execute(&db)
        .await
        .unwrap();
    db.create_table("child")
        .column(column("producer"))
        .column(column("key, quoted\""))
        .column(column("event_seq"))
        .primary_key_columns(["producer", "key, quoted\""])
        .foreign_key_column("event_seq", "parent", "event_seq")
        .execute(&db)
        .await
        .unwrap();
    db.insert("parent")
        .value("event_seq", 1)
        .execute(&db)
        .await
        .unwrap();
    db.insert("child")
        .value("producer", 1)
        .value("key, quoted\"", 2)
        .value("event_seq", 1)
        .execute(&db)
        .await
        .unwrap();
    assert!(
        db.insert("child")
            .value("producer", 1)
            .value("key, quoted\"", 2)
            .value("event_seq", 1)
            .execute(&db)
            .await
            .is_err()
    );
    let info = db.get_table_info("child").await.unwrap().unwrap();
    assert_eq!(
        info.foreign_keys.values().next().unwrap().referenced_column,
        "event_seq"
    );
    use switchy_database::query::FilterableQuery;
    let row = db
        .select("sqlite_master")
        .columns(&["sql"])
        .where_eq("name", "child")
        .execute_first(&db)
        .await
        .unwrap()
        .unwrap();
    let switchy_database::DatabaseValue::String(sql) = row.get("sql").unwrap() else {
        panic!("stored schema")
    };
    assert!(sql.contains("PRIMARY KEY (\"producer\", \"key, quoted\"\"\")"));
    db.close().await.unwrap();
}
