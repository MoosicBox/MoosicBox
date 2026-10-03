#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database,
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType, drop_table},
};

#[tokio::test]
async fn drop_table_names_are_exact_without_raw() {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    let mut names = vec!["ordinary_table"];
    if !cfg!(feature = "raw-sql") {
        names.extend([
            "table.with.dot",
            "table\"quote`",
            "victim; DROP TABLE sentinel; --",
        ]);
    }
    db.create_table("sentinel")
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
    for name in names {
        let statements = vec![drop_table(name)];
        #[cfg(feature = "cascade")]
        let statements = {
            let mut statements = statements;
            statements.extend([drop_table(name).restrict(), drop_table(name).cascade()]);
            statements
        };
        for statement in statements {
            db.create_table(name)
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
            statement.execute(&db).await.unwrap();
            assert!(!db.table_exists(name).await.unwrap());
            assert!(db.table_exists("sentinel").await.unwrap());
            drop_table(name).if_exists(true).execute(&db).await.unwrap();
        }
    }
    db.close().await.unwrap();
}
