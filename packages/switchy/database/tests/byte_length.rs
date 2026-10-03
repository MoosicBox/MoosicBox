#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database, DatabaseValue,
    query::{FilterableQuery, Sort, SortDirection, byte_length, identifier},
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType},
};

#[tokio::test]
async fn text_byte_length_preserves_utf8_nul_and_null() {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    db.create_table("payloads")
        .column(Column {
            name: "payload".into(),
            data_type: DataType::Text,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "bytes".into(),
            data_type: DataType::BigInt,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .execute(&db)
        .await
        .unwrap();
    for text in ["", "ascii", "漢字", "🦀", "e\u{301}", "a\0b", "' OR 1=1 --"] {
        db.insert("payloads")
            .value("payload", text)
            .value("bytes", i64::try_from(text.len()).unwrap())
            .execute(&db)
            .await
            .unwrap();
    }
    let rows = db
        .select("payloads")
        .where_eq("bytes", byte_length(identifier("payload")))
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 7);
    // Bound values are traversed through the expression, not interpolated.
    let rows = db
        .select("payloads")
        .where_eq("bytes", byte_length(DatabaseValue::String("🦀".into())))
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    db.insert("payloads")
        .value("payload", DatabaseValue::Null)
        .value("bytes", byte_length(DatabaseValue::Null))
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        db.select("payloads")
            .where_eq("bytes", DatabaseValue::Null)
            .execute(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    // ORDER BY expressions contribute parameters after WHERE, for both query paths.
    // SQLite uses a NULL result for an unbound parameter; this ordering makes
    // that bug observable instead of merely checking that execution succeeds.
    for first_only in [false, true] {
        let query = db.select("payloads").where_gte("bytes", 0).sorts(vec![
            Sort {
                expression: Box::new(switchy_database::query::where_eq(
                    identifier("bytes"),
                    byte_length(DatabaseValue::String("🦀".into())),
                )),
                direction: SortDirection::Desc,
            },
            Sort {
                expression: identifier("bytes").into(),
                direction: SortDirection::Asc,
            },
        ]);
        let row = if first_only {
            query.execute_first(&db).await.unwrap().unwrap()
        } else {
            query.execute(&db).await.unwrap().remove(0)
        };
        assert_eq!(row.get("payload"), Some(DatabaseValue::String("🦀".into())));
    }
    // The leased transaction must preserve the same WHERE/ORDER BY binding order.
    let tx = db.begin_transaction().await.unwrap();
    for first_only in [false, true] {
        let query = tx.select("payloads").where_gte("bytes", 0).sorts(vec![
            Sort {
                expression: Box::new(switchy_database::query::where_eq(
                    identifier("bytes"),
                    byte_length(DatabaseValue::String("🦀".into())),
                )),
                direction: SortDirection::Desc,
            },
            Sort {
                expression: identifier("bytes").into(),
                direction: SortDirection::Asc,
            },
        ]);
        let row = if first_only {
            query.execute_first(tx.as_ref()).await.unwrap().unwrap()
        } else {
            query.execute(tx.as_ref()).await.unwrap().remove(0)
        };
        assert_eq!(row.get("payload"), Some(DatabaseValue::String("🦀".into())));
    }
    tx.rollback().await.unwrap();
    db.close().await.unwrap();
}
