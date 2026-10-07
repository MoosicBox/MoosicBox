#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]
use switchy_database::{
    Database, DatabaseValue as V,
    query::{FilterableQuery, SortDirection, bounded_text, identifier},
    rusqlite::RusqliteDatabase,
    schema::{
        Column, DataType, SchemaComparison as C, SchemaExpression as E, alter_table, create_index,
    },
};

fn column(name: &str, data_type: DataType) -> Column {
    Column {
        name: name.into(),
        data_type,
        nullable: true,
        auto_increment: false,
        default: None,
    }
}

#[tokio::test]
async fn bounded_projection_and_prepare_only_planner_validation() {
    let db = RusqliteDatabase::new(vec![std::sync::Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    db.create_table("records")
        .column(column("revision", DataType::BigInt))
        .column(column("payload", DataType::Blob))
        .execute(&db)
        .await
        .unwrap();
    alter_table("records")
        .add_column_check(
            "predecessor".into(),
            DataType::BigInt,
            true,
            None,
            E::column("predecessor").compare(C::Less, E::column("revision")),
        )
        .execute(&db)
        .await
        .unwrap();
    for payload in [
        V::String("漢字".into()),
        V::String("a\0b".into()),
        V::Int64(42),
        V::Null,
        V::String("x".repeat(1024 * 1024)),
    ] {
        db.insert("records")
            .value("revision", 2)
            .value("payload", payload)
            .execute(&db)
            .await
            .unwrap();
    }
    assert!(
        db.insert("records")
            .value("revision", 2)
            .value("predecessor", 2)
            .execute(&db)
            .await
            .is_err()
    );
    let rows = db
        .select("records")
        .project_as(
            bounded_text(identifier("payload"), 3, V::String("invalid".into())),
            "bounded",
        )
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 5);
    assert_eq!(rows[0].get("bounded"), Some(V::String("invalid".into())));
    assert_eq!(rows[1].get("bounded"), Some(V::String("a\0b".into())));
    for row in &rows[2..] {
        assert_eq!(row.get("bounded"), Some(V::String("invalid".into())));
    }
    create_index("partial")
        .table("records")
        .column_ordered("revision", SortDirection::Desc)
        .predicate(E::column("revision").compare(C::IsNot, E::Null))
        .execute(&db)
        .await
        .unwrap();
    let usable = db
        .select("records")
        .where_not_eq("revision", V::Null)
        .indexed_by("partial");
    db.validate_select(&usable).await.unwrap();
    assert!(
        db.validate_select(&db.select("records").indexed_by("partial"))
            .await
            .is_err()
    );
    assert!(
        db.validate_select(&db.select("records").indexed_by("missing\"; --"))
            .await
            .is_err()
    );
    assert_eq!(db.select("records").execute(&db).await.unwrap().len(), 5);
    db.create_table("parents")
        .column(column("id", DataType::BigInt))
        .column(column("revision", DataType::BigInt))
        .primary_key_columns(["id", "revision"])
        .execute(&db)
        .await
        .unwrap();
    db.create_table("children")
        .column(column("id", DataType::BigInt))
        .column(column("revision", DataType::BigInt))
        .composite_foreign_key(["id", "revision"], "parents", ["id", "revision"])
        .execute(&db)
        .await
        .unwrap();
    assert!(
        db.insert("children")
            .value("id", 1)
            .value("revision", 2)
            .execute(&db)
            .await
            .is_err()
    );
    db.insert("parents")
        .value("id", 1)
        .value("revision", 2)
        .execute(&db)
        .await
        .unwrap();
    db.insert("children")
        .value("id", 1)
        .value("revision", 2)
        .execute(&db)
        .await
        .unwrap();
    let receipt_predicate = E::And(vec![
        E::In(
            Box::new(E::column("status")),
            ["admitted", "running", "cancelling", "sibling_cancelling"]
                .into_iter()
                .map(|value| E::Text(value.into()))
                .collect(),
        ),
        E::column("receipt_json").compare(C::IsNot, E::Null),
    ]);
    db.create_table("workflow_attempts")
        .column(column("run_id", DataType::Text))
        .column(column("dispatch_identity", DataType::Text))
        .column(column("status", DataType::Text))
        .column(column("receipt_json", DataType::Text))
        .execute(&db)
        .await
        .unwrap();
    create_index("workflow_receipt_recovery_page")
        .table("workflow_attempts")
        .columns(vec!["run_id", "dispatch_identity"])
        .predicate(receipt_predicate.clone())
        .execute(&db)
        .await
        .unwrap();
    let query = db
        .select("workflow_attempts")
        .indexed_by("workflow_receipt_recovery_page")
        .where_eq("run_id", "")
        .where_gt("dispatch_identity", "")
        .filter(Box::new(
            switchy_database::query::schema_predicate(&receipt_predicate).unwrap(),
        ))
        .limit(1);
    db.validate_select(&query).await.unwrap();
    assert!(
        db.validate_select(
            &db.select("workflow_attempts")
                .indexed_by("workflow_receipt_recovery_page")
                .where_in(
                    "status",
                    vec!["admitted", "running", "cancelling", "sibling_cancelling"]
                )
                .where_not_eq("receipt_json", V::Null)
        )
        .await
        .is_err()
    );
    db.sqlite_foreign_keys(false).await.unwrap();
    db.insert("children")
        .value("id", 9)
        .value("revision", 9)
        .execute(&db)
        .await
        .unwrap();
    db.sqlite_foreign_keys(true).await.unwrap();
    assert!(
        db.insert("children")
            .value("id", 10)
            .value("revision", 10)
            .execute(&db)
            .await
            .is_err()
    );
    assert!(db.sqlite_defer_foreign_keys(true).await.is_err());
    let tx = db.begin_transaction().await.unwrap();
    assert!(tx.sqlite_foreign_keys(false).await.is_err());
    tx.sqlite_defer_foreign_keys(true).await.unwrap();
    tx.insert("children")
        .value("id", 11)
        .value("revision", 11)
        .execute(tx.as_ref())
        .await
        .unwrap();
    tx.insert("parents")
        .value("id", 11)
        .value("revision", 11)
        .execute(tx.as_ref())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    #[cfg(feature = "sqlite-test-fixtures")]
    {
        db.sqlite_ignore_check_constraints(true).await.unwrap();
        db.insert("records")
            .value("revision", 2)
            .value("predecessor", 3)
            .execute(&db)
            .await
            .unwrap();
        db.sqlite_ignore_check_constraints(false).await.unwrap();
        assert!(
            db.insert("records")
                .value("revision", 2)
                .value("predecessor", 3)
                .execute(&db)
                .await
                .is_err()
        );
    }
    db.sqlite_set_busy_timeout(std::time::Duration::ZERO)
        .await
        .unwrap();
    db.insert("records")
        .value("revision", 2)
        .value("payload", switchy_database::query::zero_blob(2_000_000))
        .or_ignore()
        .execute_count(&db)
        .await
        .unwrap();
    db.insert("records")
        .value("revision", 2)
        .value("payload", switchy_database::query::cast_blob("pending"))
        .or_ignore()
        .execute_count(&db)
        .await
        .unwrap();
    let rows = db
        .select("records")
        .project_as(bounded_text(identifier("payload"), 3, V::Null), "bounded")
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        rows.len(),
        if cfg!(feature = "sqlite-test-fixtures") {
            8
        } else {
            7
        }
    );
    assert_eq!(rows[rows.len() - 2].get("bounded"), Some(V::Null));
    assert_eq!(rows[rows.len() - 1].get("bounded"), Some(V::Null));
    assert!(db.select("records").execute(&db).await.is_err());
    let tx = db.begin_transaction().await.unwrap();
    tx.sqlite_set_busy_timeout(std::time::Duration::ZERO)
        .await
        .unwrap();
    assert!(
        tx.insert("children")
            .value("id", 99)
            .value("revision", 99)
            .or_ignore()
            .execute_count(tx.as_ref())
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
    switchy_database::schema::create_trigger("temp_guard", "records")
        .temporary(true)
        .before_insert()
        .raise_abort("temporary")
        .execute(&db)
        .await
        .unwrap();
    assert!(db.trigger_exists("temp_guard").await.unwrap());
    assert!(
        db.insert("records")
            .value("revision", 2)
            .or_ignore()
            .execute_count(&db)
            .await
            .is_err()
    );
    db.drop_trigger("temp_guard", false).await.unwrap();
    assert_eq!(
        db.exec_update_count(&db.update("records").value(
            "revision",
            switchy_database::query::add(identifier("revision"), 2)
        ))
        .await
        .unwrap(),
        if cfg!(feature = "sqlite-test-fixtures") {
            8
        } else {
            7
        }
    );
    assert!(db.sqlite_is_autocommit().await.unwrap());
    let tx = db.begin_transaction().await.unwrap();
    assert!(!tx.sqlite_is_autocommit().await.unwrap());
    tx.rollback().await.unwrap();
    assert!(db.sqlite_is_autocommit().await.unwrap());
    db.create_table("blob_fixture")
        .column(column("revision", DataType::Blob))
        .execute(&db)
        .await
        .unwrap();
    db.insert("blob_fixture")
        .value("revision", switchy_database::query::cast_blob("\u{0001}"))
        .or_ignore()
        .execute_count(&db)
        .await
        .unwrap();
    let row = db
        .select("blob_fixture")
        .project_as(
            switchy_database::query::storage_type(identifier("revision")),
            "storage",
        )
        .project_as(
            switchy_database::query::cast_text(identifier("revision")),
            "bytes",
        )
        .execute(&db)
        .await
        .unwrap()
        .remove(0);
    assert_eq!(row.get("storage"), Some(V::String("blob".into())));
    assert_eq!(row.get("bytes"), Some(V::String("\u{0001}".into())));
    db.close().await.unwrap();
}
