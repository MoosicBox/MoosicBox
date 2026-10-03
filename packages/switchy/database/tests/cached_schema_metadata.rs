#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database,
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType},
};

#[tokio::test]
async fn cached_select_uses_recreated_table_columns() {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    for name in ["original", "replacement", "unicode_漢字"] {
        db.create_table("recreated")
            .column(Column {
                name: name.into(),
                data_type: DataType::Text,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .execute(&db)
            .await
            .unwrap();
        db.insert("recreated")
            .value(name, "retained value")
            .execute(&db)
            .await
            .unwrap();
        let rows = db.select("recreated").execute(&db).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].columns,
            vec![(name.into(), "retained value".into())]
        );
        db.drop_table("recreated").execute(&db).await.unwrap();
    }
    db.close().await.unwrap();
}
