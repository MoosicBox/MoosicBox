#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database,
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType, alter_table},
};

#[tokio::test]
async fn basic_alter_names_are_exact() {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    let table = if cfg!(feature = "raw-sql") {
        "ordinary"
    } else {
        "table.\"`; --"
    };
    db.create_table(table)
        .column(Column {
            name: "id".into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .execute(&db)
        .await
        .unwrap();
    let original = "payload\"`); DROP TABLE ordinary; --";
    let renamed = "renamed.\"`";
    alter_table(table)
        .add_column(original.into(), DataType::Text, true, None)
        .execute(&db)
        .await
        .unwrap();
    assert!(db.column_exists(table, original).await.unwrap());
    alter_table(table)
        .rename_column(original.into(), renamed.into())
        .execute(&db)
        .await
        .unwrap();
    assert!(db.column_exists(table, renamed).await.unwrap());
    let info = db.get_table_info(table).await.unwrap().unwrap();
    assert!(info.columns.contains_key(renamed));
    assert!(!db.column_exists(table, original).await.unwrap());
    alter_table(table)
        .modify_column(renamed.into(), DataType::Text, Some(true), None)
        .execute(&db)
        .await
        .unwrap();
    assert!(db.column_exists(table, renamed).await.unwrap());
    alter_table(table)
        .drop_column(renamed.into())
        .execute(&db)
        .await
        .unwrap();
    assert!(!db.column_exists(table, renamed).await.unwrap());
    assert!(db.column_exists(table, "id").await.unwrap());
    db.close().await.unwrap();
}

#[cfg(feature = "cascade")]
#[tokio::test]
async fn cascade_dependencies_use_exact_identifiers_and_rollback_together() {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    let (table, index) = if cfg!(feature = "raw-sql") {
        ("cascade_items", "cascade_index")
    } else {
        ("cascade.items\"`; --", "index\"`); DROP TABLE sentinel; --")
    };
    for name in [table, "sentinel"] {
        db.create_table(name)
            .column(Column {
                name: "id".into(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "payload".into(),
                data_type: DataType::Text,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .execute(&db)
            .await
            .unwrap();
    }
    switchy_database::schema::create_index(index)
        .table(table)
        .column("payload")
        .execute(&db)
        .await
        .unwrap();
    assert!(matches!(
        alter_table(table)
            .drop_column_restrict("payload".into())
            .execute(&db)
            .await,
        Err(switchy_database::DatabaseError::ForeignKeyViolation(_))
    ));
    assert!(
        db.get_table_info(table)
            .await
            .unwrap()
            .unwrap()
            .indexes
            .contains_key(index)
    );

    let transaction = db.begin_transaction().await.unwrap();
    alter_table(table)
        .drop_column_cascade("payload".into())
        .execute(transaction.as_ref())
        .await
        .unwrap();
    assert!(!transaction.column_exists(table, "payload").await.unwrap());
    assert!(
        !transaction
            .get_table_info(table)
            .await
            .unwrap()
            .unwrap()
            .indexes
            .contains_key(index)
    );
    transaction.rollback().await.unwrap();
    assert!(db.column_exists(table, "payload").await.unwrap());
    assert!(
        db.get_table_info(table)
            .await
            .unwrap()
            .unwrap()
            .indexes
            .contains_key(index)
    );

    alter_table(table)
        .drop_column_cascade("payload".into())
        .execute(&db)
        .await
        .unwrap();
    assert!(!db.column_exists(table, "payload").await.unwrap());
    assert!(
        db.get_table_info(table)
            .await
            .unwrap()
            .unwrap()
            .indexes
            .is_empty()
    );
    assert!(db.column_exists(table, "id").await.unwrap());
    assert!(db.column_exists("sentinel", "payload").await.unwrap());
    db.close().await.unwrap();
}

#[tokio::test]
async fn rejected_modify_default_releases_connection_without_schema_changes() {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    db.create_table("rejected_default")
        .column(Column {
            name: "payload".into(),
            data_type: DataType::Text,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .execute(&db)
        .await
        .unwrap();
    let before = db
        .get_table_info("rejected_default")
        .await
        .unwrap()
        .unwrap();
    let error = alter_table("rejected_default")
        .modify_column(
            "payload".into(),
            DataType::Text,
            Some(true),
            Some(switchy_database::DatabaseValue::StringOpt(None)),
        )
        .execute(&db)
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        switchy_database::DatabaseError::InvalidSchema(_)
    ));
    let after = db
        .get_table_info("rejected_default")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(before.columns, after.columns);
    // A fresh transaction must work: validation must not strand an open BEGIN.
    let transaction = db.begin_transaction().await.unwrap();
    transaction.rollback().await.unwrap();
    db.close().await.unwrap();
}

#[tokio::test]
async fn modified_column_string_default_is_literal() {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    db.create_table("defaults")
        .column(Column {
            name: "id".into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "payload".into(),
            data_type: DataType::Text,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .execute(&db)
        .await
        .unwrap();
    let default = "O'Brien'); DROP TABLE defaults; --";
    alter_table("defaults")
        .modify_column(
            "payload".into(),
            DataType::Text,
            Some(true),
            Some(default.into()),
        )
        .execute(&db)
        .await
        .unwrap();
    db.insert("defaults")
        .value("id", 1_i64)
        .execute(&db)
        .await
        .unwrap();
    let rows = db.select("defaults").execute(&db).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("payload"), Some(default.into()));
    alter_table("defaults")
        .add_column("added".into(), DataType::Text, true, Some(default.into()))
        .execute(&db)
        .await
        .unwrap();
    let rows = db
        .select("defaults")
        .columns(&["added"])
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(rows[0].get("added"), Some(default.into()));
    db.close().await.unwrap();
}
