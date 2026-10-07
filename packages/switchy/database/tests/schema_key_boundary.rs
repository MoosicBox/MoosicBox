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
        data_type: DataType::BigInt,
        nullable: false,
        auto_increment: false,
        default: None,
    }
}

#[tokio::test]
async fn schema_keys_are_identifiers_without_raw() {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    db.create_table("parent")
        .column(column("id"))
        .primary_key("id")
        .execute(&db)
        .await
        .unwrap();
    db.create_table("child")
        .column(column("id"))
        .foreign_key(("id", "parent"))
        .execute(&db)
        .await
        .unwrap();
    let child = db.get_table_info("child").await.unwrap().unwrap();
    let reference = child.foreign_keys.values().next().unwrap();
    assert_eq!(reference.referenced_table, "parent");
    assert_eq!(reference.referenced_column, "id");
    db.insert("parent")
        .value("id", 1)
        .execute(&db)
        .await
        .unwrap();
    db.insert("child")
        .value("id", 1)
        .execute(&db)
        .await
        .unwrap();
    assert!(
        db.insert("parent")
            .value("id", 1)
            .execute(&db)
            .await
            .is_err()
    );

    #[cfg(not(feature = "raw-sql"))]
    {
        // These would form valid primary-key syntax if interpreted as SQL.
        for key in ["id DESC", "id, other", "id), UNIQUE (other"] {
            assert!(
                db.create_table("rejected")
                    .column(column("id"))
                    .column(column("other"))
                    .primary_key(key)
                    .execute(&db)
                    .await
                    .is_err()
            );
            assert!(!db.table_exists("rejected").await.unwrap());
        }
        db.create_table("parent(id)")
            .column(column("id"))
            .primary_key("id")
            .execute(&db)
            .await
            .unwrap();
        db.create_table("literal_target")
            .column(column("id"))
            .foreign_key(("id", "parent(id)"))
            .execute(&db)
            .await
            .unwrap();
        let target = db.get_table_info("literal_target").await.unwrap().unwrap();
        let reference = target.foreign_keys.values().next().unwrap();
        assert_eq!(reference.referenced_table, "parent(id)");
        assert_eq!(reference.column, "id");
    }
    db.close().await.unwrap();
}
