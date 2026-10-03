#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database,
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType, create_index, drop_index},
};

#[tokio::test]
async fn index_names_are_identifiers() {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    db.create_table("items")
        .column(Column {
            name: "value".into(),
            data_type: DataType::Text,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .execute(&db)
        .await
        .unwrap();
    let mut names = vec!["ordinary_index"];
    if !cfg!(feature = "raw-sql") {
        names.extend([
            "idx.with.dot",
            "idx\"quote`",
            "idx ON items(value); DROP TABLE items; --",
        ]);
    }
    for name in names {
        create_index(name)
            .table("items")
            .column("value")
            .unique(true)
            .execute(&db)
            .await
            .unwrap();
        db.insert("items")
            .value("value", name)
            .execute(&db)
            .await
            .unwrap();
        assert!(
            db.insert("items")
                .value("value", name)
                .execute(&db)
                .await
                .is_err()
        );
        let info = db.get_table_info("items").await.unwrap().unwrap();
        assert_eq!(info.indexes[name].columns, ["value"]);
        drop_index(name, "items").execute(&db).await.unwrap();
        db.insert("items")
            .value("value", name)
            .execute(&db)
            .await
            .unwrap();
        db.delete("items").execute(&db).await.unwrap();
    }
    // A column string is never an index expression or sort clause.
    assert!(
        create_index("bad_column")
            .table("items")
            .column("value DESC")
            .execute(&db)
            .await
            .is_err()
    );
    db.close().await.unwrap();
}
