#![cfg(all(
    any(feature = "sqlite-sqlx", feature = "sqlite-rusqlite"),
    feature = "schema"
))]

use std::sync::Arc;
use switchy_database::{
    Database, DatabaseValue,
    query::{FilterableQuery, SortDirection, byte_length, count_all, identifier, max, min},
    schema::{Column, DataType},
};

#[switchy_async::test]
async fn typed_inventory_and_byte_length() {
    #[cfg(feature = "sqlite-sqlx")]
    let db = {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        switchy_database::sqlx::sqlite::SqliteSqlxDatabase::new(Arc::new(
            switchy_async::sync::Mutex::new(pool),
        ))
    };
    #[cfg(not(feature = "sqlite-sqlx"))]
    let db = switchy_database::rusqlite::RusqliteDatabase::new(vec![Arc::new(
        switchy_async::sync::Mutex::new(rusqlite::Connection::open_in_memory().unwrap()),
    )]);
    check(&db).await;
}

#[cfg(feature = "sqlite-rusqlite")]
#[switchy_async::test]
async fn rusqlite_inventory_and_byte_length() {
    let db = switchy_database::rusqlite::RusqliteDatabase::new(vec![Arc::new(
        switchy_async::sync::Mutex::new(rusqlite::Connection::open_in_memory().unwrap()),
    )]);
    check(&db).await;
}

async fn check(db: &dyn Database) {
    db.create_table("events")
        .columns(vec![
            Column {
                name: "kind".into(),
                data_type: DataType::Text,
                nullable: false,
                auto_increment: false,
                default: None,
            },
            Column {
                name: "seq".into(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: false,
                default: None,
            },
        ])
        .execute(db)
        .await
        .unwrap();
    for (kind, seq) in [("é", 2_i64), ("é", 4), ("z", 8)] {
        db.insert("events")
            .value("kind", kind)
            .value("seq", seq)
            .execute(db)
            .await
            .unwrap();
    }
    let query = || {
        db.select("events")
            .project(identifier("kind"))
            .project_as(count_all(), "count")
            .project_as(min(identifier("seq")), "first")
            .project_as(max(identifier("seq")), "last")
            .group_by(identifier("kind"))
            .sort("kind", SortDirection::Desc)
    };
    let rows = query().execute(db).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get("count"), Some(DatabaseValue::Int64(2)));
    assert_eq!(rows[0].get("first"), Some(DatabaseValue::Int64(2)));
    assert_eq!(rows[0].get("last"), Some(DatabaseValue::Int64(4)));
    assert_eq!(
        query()
            .execute_first(db)
            .await
            .unwrap()
            .unwrap()
            .get("count"),
        Some(DatabaseValue::Int64(2))
    );
    let alias = "bytes\" FROM events; --";
    let row = db
        .select("events")
        .project_as(byte_length(identifier("kind")), alias)
        .project_as(DatabaseValue::Int64(17), "bound")
        .where_eq("seq", 2_i64)
        .execute_first(db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.get(alias), Some(DatabaseValue::Int64(2)));
    assert_eq!(row.get("bound"), Some(DatabaseValue::Int64(17)));
    #[cfg(feature = "raw-sql")]
    {
        let row = db
            .select("events")
            .columns(&["COUNT(*) AS total"])
            .execute_first(db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.get("total"), Some(DatabaseValue::Int64(3)));
    }
    #[cfg(not(feature = "raw-sql"))]
    {
        // SQL-looking names are literal identifiers, not expression fragments.
        assert!(
            db.select("events")
                .project_as(identifier("COUNT(*)"), "not_count")
                .execute_first(db)
                .await
                .is_err()
        );
    }
    #[cfg(not(feature = "raw-sql"))]
    {
        let table = "inventory\"; DROP TABLE events; --";
        let column = "kind.dot\" GROUP BY seq --";
        db.create_table(table)
            .column(Column {
                name: column.into(),
                data_type: DataType::Text,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .execute(db)
            .await
            .unwrap();
        db.insert(table)
            .value(column, "é")
            .execute(db)
            .await
            .unwrap();
        let row = db
            .select(table)
            .project_as(identifier(column), "kind")
            .project_as(count_all(), "count")
            .group_by(identifier(column))
            .sort(column, SortDirection::Asc)
            .execute_first(db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.get("kind"), Some(DatabaseValue::String("é".into())));
        assert_eq!(row.get("count"), Some(DatabaseValue::Int64(1)));
    }
    db.close().await.unwrap();
}
