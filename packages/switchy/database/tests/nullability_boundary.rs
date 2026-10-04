#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database, DatabaseValue,
    query::select,
    rusqlite::{RusqliteDatabase, relax_integer_nullability_on_connection, select_on_connection},
    schema::{Column, DataType, create_index},
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
async fn indexed_column_failure_rolls_back_partial_ddl_without_committing_owner() {
    let connection = Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ));
    let db = RusqliteDatabase::new(vec![Arc::clone(&connection)]);
    db.create_table("limits")
        .column(column("cap"))
        .execute(&db)
        .await
        .unwrap();
    create_index("cap_index")
        .table("limits")
        .column("cap")
        .execute(&db)
        .await
        .unwrap();
    db.insert("limits")
        .value("cap", 17_i64)
        .execute(&db)
        .await
        .unwrap();
    drop(db);
    let mut guard = connection.lock().await;
    let transaction = guard.transaction().unwrap();
    assert!(relax_integer_nullability_on_connection(&transaction, "limits", "cap").is_err());
    assert!(!transaction.is_autocommit());
    let rows = select_on_connection(&transaction, &select("limits")).unwrap();
    assert_eq!(rows[0].columns.len(), 1, "temporary column rolled back");
    assert_eq!(rows[0].get("cap"), Some(DatabaseValue::Int64(17)));
    transaction.commit().unwrap();
    drop(guard);
    let db = RusqliteDatabase::new(vec![connection]);
    assert!(!db.get_table_columns("limits").await.unwrap()[0].nullable);
    assert!(
        db.get_table_info("limits")
            .await
            .unwrap()
            .unwrap()
            .indexes
            .contains_key("cap_index")
    );
}

#[tokio::test]
async fn adversarial_identifiers_and_autocommit_fail_closed() {
    let connection = Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ));
    let db = RusqliteDatabase::new(vec![Arc::clone(&connection)]);
    db.create_table("sentinel")
        .column(column("cap"))
        .execute(&db)
        .await
        .unwrap();
    db.insert("sentinel")
        .value("cap", i64::MAX)
        .execute(&db)
        .await
        .unwrap();
    drop(db);
    let mut guard = connection.lock().await;
    assert!(relax_integer_nullability_on_connection(&guard, "sentinel", "cap").is_err());
    let transaction = guard.transaction().unwrap();
    for (table, name) in [
        ("sentinel; DROP TABLE sentinel; --", "cap"),
        ("sentinel", "cap\"; DROP TABLE sentinel; --"),
        ("sentinel", "cap, other"),
    ] {
        assert!(relax_integer_nullability_on_connection(&transaction, table, name).is_err());
        assert!(!transaction.is_autocommit());
    }
    relax_integer_nullability_on_connection(&transaction, "sentinel", "cap").unwrap();
    assert_eq!(
        select_on_connection(&transaction, &select("sentinel")).unwrap()[0].get("cap"),
        Some(DatabaseValue::Int64(i64::MAX))
    );
    transaction.rollback().unwrap();
}
