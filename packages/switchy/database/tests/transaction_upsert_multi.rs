//! Multi-row upserts inside a transaction must only touch conflicting rows.
#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database, DatabaseValue,
    query::identifier,
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType},
};

async fn seeded_db() -> RusqliteDatabase {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    db.create_table("items")
        .column(Column {
            name: "id".into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "value".into(),
            data_type: DataType::Text,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .unique_columns(["id"])
        .execute(&db)
        .await
        .unwrap();
    db.upsert_multi("items")
        .values(vec![
            vec![("id", DatabaseValue::Int64(1)), ("value", "old".into())],
            vec![("id", DatabaseValue::Int64(10)), ("value", "keep".into())],
        ])
        .unique(vec![Box::new(identifier("id"))])
        .execute(&db)
        .await
        .unwrap();
    db
}

async fn values(db: &dyn Database) -> Vec<(i64, String)> {
    let mut rows: Vec<(i64, String)> = db
        .select("items")
        .execute(db)
        .await
        .unwrap()
        .iter()
        .map(|row| {
            let Some(DatabaseValue::Int64(id)) = row.get("id") else {
                panic!("missing id in {row:?}");
            };
            let Some(DatabaseValue::String(value)) = row.get("value") else {
                panic!("missing value in {row:?}");
            };
            (id, value)
        })
        .collect();
    rows.sort();
    rows
}

#[tokio::test]
async fn transaction_upsert_multi_updates_conflicts_and_inserts_new_rows() {
    let db = seeded_db().await;
    let tx = db.begin_transaction().await.unwrap();
    let returned = tx
        .upsert_multi("items")
        .values(vec![
            vec![("id", DatabaseValue::Int64(1)), ("value", "new".into())],
            vec![("id", DatabaseValue::Int64(2)), ("value", "added".into())],
        ])
        .unique(vec![Box::new(identifier("id"))])
        .execute(&*tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(returned.len(), 2);
    assert_eq!(
        values(&db).await,
        vec![
            (1, "new".to_string()),
            (2, "added".to_string()),
            (10, "keep".to_string()),
        ],
    );
}

#[tokio::test]
async fn transaction_upsert_multi_matches_non_transaction_behavior() {
    let rows = || {
        vec![
            vec![("id", DatabaseValue::Int64(1)), ("value", "new".into())],
            vec![("id", DatabaseValue::Int64(2)), ("value", "added".into())],
        ]
    };

    let direct = seeded_db().await;
    direct
        .upsert_multi("items")
        .values(rows())
        .unique(vec![Box::new(identifier("id"))])
        .execute(&direct)
        .await
        .unwrap();

    let transactional = seeded_db().await;
    let tx = transactional.begin_transaction().await.unwrap();
    tx.upsert_multi("items")
        .values(rows())
        .unique(vec![Box::new(identifier("id"))])
        .execute(&*tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(values(&direct).await, values(&transactional).await);
}
