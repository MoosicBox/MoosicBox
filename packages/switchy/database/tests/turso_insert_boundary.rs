#![cfg(all(feature = "turso", feature = "schema"))]

use switchy_database::{
    Database, DatabaseValue,
    schema::{Column, DataType},
    turso::TursoDatabase,
};

#[tokio::test]
async fn insert_names_are_not_column_lists() {
    let db = TursoDatabase::new(":memory:").await.unwrap();
    db.create_table("items")
        .column(Column {
            name: "id".into(),
            data_type: DataType::BigInt,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .execute(&db)
        .await
        .unwrap();
    let result = db.insert("items(id)").value("id", 7).execute(&db).await;
    assert!(result.is_err());
    if !cfg!(feature = "raw-sql") {
        let result = db
            .insert("items")
            .value("id) VALUES (7) RETURNING * --", 8)
            .execute(&db)
            .await;
        assert!(result.is_err());
        assert!(db.select("items").execute(&db).await.unwrap().is_empty());
    }
    let row = db
        .insert("items")
        .value("id", 9)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(row.get("id"), Some(DatabaseValue::Int64(9)));
    db.close().await.unwrap();
}

#[cfg(not(feature = "raw-sql"))]
#[tokio::test]
async fn upsert_preserves_exact_names_and_rejects_conflict_fragments() {
    let db = TursoDatabase::new(":memory:").await.unwrap();
    let table = "items\".upsert";
    let key = "id\".key";
    db.create_table(table)
        .column(Column {
            name: key.into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .primary_key(key)
        .execute(&db)
        .await
        .unwrap();
    for _ in 0..2 {
        let row = db
            .upsert(table)
            .value(key, 7)
            .unique(&[key])
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(row.len(), 1);
        assert_eq!(row[0].get(key), Some(DatabaseValue::Int64(7)));
    }
    let rows = db
        .upsert_multi(table)
        .values(vec![vec![(key, 7)], vec![(key, 8)]])
        .unique(vec![Box::new(switchy_database::query::identifier(key))])
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        db.upsert(table)
            .value(key, 9)
            .unique(&["id) DO NOTHING --"])
            .execute(&db)
            .await
            .is_err()
    );
    let rows = db.select(table).execute(&db).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(
        rows.iter()
            .all(|row| matches!(row.get(key), Some(DatabaseValue::Int64(7 | 8))))
    );
    db.close().await.unwrap();
}

#[cfg(not(feature = "raw-sql"))]
#[tokio::test]
async fn insert_preserves_exact_delimited_names() {
    let db = TursoDatabase::new(":memory:").await.unwrap();
    for table in ["items.with.dot", "items\"quoted", "items (id)"] {
        let column = "value\".with.dot";
        db.create_table(table)
            .column(Column {
                name: column.into(),
                data_type: DataType::BigInt,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .execute(&db)
            .await
            .unwrap();
        let row = db
            .insert(table)
            .value(column, 42)
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(row.get(column), Some(DatabaseValue::Int64(42)));
        // Empty values take a separate DEFAULT VALUES rendering route.
        let row = db.insert(table).execute(&db).await.unwrap();
        assert_eq!(row.get(column), Some(DatabaseValue::Null));
    }
    db.close().await.unwrap();
}
