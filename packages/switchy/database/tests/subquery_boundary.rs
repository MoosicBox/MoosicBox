#![cfg(all(
    feature = "sqlite-rusqlite",
    feature = "schema",
    not(feature = "raw-sql")
))]

use std::sync::Arc;
use switchy_database::{
    Database,
    query::{FilterableQuery, select},
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType},
};

#[tokio::test]
async fn subquery_inputs_are_identifiers_not_sql() {
    let connection = rusqlite::Connection::open_in_memory().unwrap();
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(connection))]);
    assert_subquery_boundary(&db).await;
    db.close().await.unwrap();
}

#[cfg(feature = "sqlite-sqlx")]
#[tokio::test]
async fn sqlx_subquery_inputs_are_identifiers_not_sql() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let db = switchy_database::sqlx::sqlite::SqliteSqlxDatabase::new(Arc::new(
        tokio::sync::Mutex::new(pool),
    ));
    assert_subquery_boundary(&db).await;
    db.close().await.unwrap();
}

async fn assert_subquery_boundary(db: &dyn Database) {
    db.create_table("items")
        .column(Column {
            name: "value".into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .execute(db)
        .await
        .unwrap();
    db.insert("items")
        .value("value", 7)
        .execute(db)
        .await
        .unwrap();
    assert_eq!(
        db.select("items")
            .where_in("value", select("items").columns(&["value"]))
            .execute(db)
            .await
            .unwrap()
            .len(),
        1
    );
    // SQLite's double-quoted-string compatibility may treat an unknown quoted
    // column as a string, but it must never evaluate the supplied expression.
    for projection in [
        &["3 + 4"][..],
        &["value\" FROM items WHERE 1=1 --"][..],
        &["items.value"][..],
    ] {
        let result = db
            .select("items")
            .where_in("value", select("items").columns(projection))
            .execute(db)
            .await;
        // Backends may reject unknown identifiers instead of using SQLite's
        // legacy string fallback. Neither behavior may execute the fragment.
        assert!(result.is_err() || result.unwrap().is_empty());
    }
    assert!(
        db.select("items")
            .where_in("value", select("items WHERE 1=1 --").columns(&["value"]))
            .execute(db)
            .await
            .is_err()
    );
    assert_eq!(db.select("items").execute(db).await.unwrap().len(), 1);
}
