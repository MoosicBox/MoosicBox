//! Persistent guards exercise typed schema APIs, not caller SQL.
#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use switchy_database::{
    Database, DatabaseErrorKind,
    schema::{
        Column, DataType, SchemaComparison as C, SchemaExpression as E, create_index,
        create_trigger,
    },
};
use switchy_database_connection::{SqliteAccess, SqliteOpenOptions, open_sqlite_rusqlite};

fn options() -> SqliteOpenOptions {
    SqliteOpenOptions {
        access: SqliteAccess::ReadWriteCreate,
        allow_uri: false,
        busy_timeout: std::time::Duration::ZERO,
        foreign_keys: true,
        connections: std::num::NonZeroUsize::new(1).unwrap(),
    }
}
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
async fn persistent_correlated_guards_checks_partial_indexes_and_backup() {
    let directory = std::env::temp_dir().join(format!(
        "switchy-guards-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("source.db");
    let backup = directory.join("backup.db");
    let db = open_sqlite_rusqlite(&path, &options()).unwrap();
    db.create_table("barriers")
        .column(column("run_id"))
        .execute(&db)
        .await
        .unwrap();
    db.create_table("attempts")
        .column(column("run_id"))
        .column(column("generation"))
        .column(column("concurrency_cap"))
        .check(E::column("generation").compare(C::Greater, E::Integer(0)))
        .execute(&db)
        .await
        .unwrap();
    let predicate = E::exists(
        "barriers",
        E::column("run_id").compare(C::Equal, E::New("run_id".into())),
    );
    let trigger = "guard\"`); DROP TABLE barriers; --";
    create_trigger(trigger, "attempts")
        .before_insert()
        .when(predicate)
        .raise_abort("frozen 'run'; --")
        .execute(&db)
        .await
        .unwrap();
    create_index("active_generations")
        .table("attempts")
        .column("run_id")
        .unique(true)
        .predicate(E::column("generation").compare(C::Greater, E::Integer(1)))
        .execute(&db)
        .await
        .unwrap();
    db.insert("barriers")
        .value("run_id", 1)
        .execute(&db)
        .await
        .unwrap();
    for (run_id, generation) in [(1, 1), (2, 0)] {
        let error = db
            .insert("attempts")
            .value("run_id", run_id)
            .value("concurrency_cap", 1)
            .value("generation", generation)
            .execute(&db)
            .await
            .unwrap_err();
        assert_eq!(error.classification(), Some(DatabaseErrorKind::Constraint));
    }
    for generation in [1, 1, 2] {
        db.insert("attempts")
            .value("run_id", 2)
            .value("concurrency_cap", 1)
            .value("generation", generation)
            .execute(&db)
            .await
            .unwrap();
    }
    assert_eq!(
        db.insert("attempts")
            .value("run_id", 2)
            .value("concurrency_cap", 1)
            .value("generation", 3)
            .execute(&db)
            .await
            .unwrap_err()
            .classification(),
        Some(DatabaseErrorKind::Constraint)
    );
    let tx = db.begin_transaction().await.unwrap();
    tx.drop_trigger(trigger, false).await.unwrap();
    assert!(!tx.trigger_exists(trigger).await.unwrap());
    tx.rollback().await.unwrap();
    assert!(db.trigger_exists(trigger).await.unwrap());
    let tx = db.begin_transaction().await.unwrap();
    tx.sqlite_relax_integer_nullability("attempts", "concurrency_cap")
        .await
        .unwrap();
    assert!(
        tx.get_table_info("attempts")
            .await
            .unwrap()
            .unwrap()
            .columns["concurrency_cap"]
            .nullable
    );
    assert!(tx.trigger_exists(trigger).await.unwrap());
    assert_eq!(
        tx.insert("attempts")
            .value("run_id", 3)
            .value("concurrency_cap", 1)
            .value("generation", 0)
            .execute(tx.as_ref())
            .await
            .unwrap_err()
            .classification(),
        Some(DatabaseErrorKind::Constraint)
    );
    tx.rollback().await.unwrap();
    assert!(
        !db.get_table_info("attempts")
            .await
            .unwrap()
            .unwrap()
            .columns["concurrency_cap"]
            .nullable
    );
    let tx = db.begin_transaction().await.unwrap();
    tx.sqlite_relax_integer_nullability("attempts", "concurrency_cap")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(db.trigger_exists(trigger).await.unwrap());
    assert_eq!(
        db.insert("attempts")
            .value("run_id", 1)
            .value("concurrency_cap", 1)
            .value("generation", 1)
            .execute(&db)
            .await
            .unwrap_err()
            .classification(),
        Some(DatabaseErrorKind::Constraint)
    );
    create_trigger("freeze_update", "attempts")
        .before_update_of(["generation"])
        .when(E::Old("run_id".into()).compare(C::Equal, E::Integer(2)))
        .raise_abort("immutable generation")
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        db.exec_update_count(
            &db.update("attempts")
                .value("concurrency_cap", 1)
                .value("generation", 9)
        )
        .await
        .unwrap_err()
        .classification(),
        Some(DatabaseErrorKind::Constraint)
    );
    create_trigger("ignore_delete", "attempts")
        .before_delete()
        .raise_ignore()
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        db.exec_delete_count(&db.delete("attempts")).await.unwrap(),
        0
    );
    let tx = db
        .begin_transaction_with_mode(switchy_database::TransactionMode::Immediate)
        .await
        .unwrap();
    assert!(matches!(
        tx.sqlite_backup_to(&backup).await,
        Err(switchy_database::DatabaseError::UnsupportedOperation(_))
    ));
    tx.rollback().await.unwrap();
    let insert = db
        .insert("attempts")
        .value("run_id", 2)
        .value("concurrency_cap", 1)
        .value("generation", 4)
        .on_conflict_do_nothing(["run_id"]);
    // A partial unique index is not a full conflict target; SQLite must reject it.
    assert!(insert.execute_count(&db).await.is_err());
    db.create_table("retries")
        .column(column("id"))
        .column(column("other"))
        .primary_key("id")
        .unique_columns(["other"])
        .check(E::column("other").compare(C::Greater, E::Integer(0)))
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        db.insert("retries")
            .value("id", 1)
            .value("other", 1)
            .on_conflict_do_nothing(["id"])
            .execute_count(&db)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        db.insert("retries")
            .value("id", 1)
            .value("other", 2)
            .on_conflict_do_nothing(["id"])
            .execute_count(&db)
            .await
            .unwrap(),
        0
    );
    for (id, other) in [(2, 1), (2, 0)] {
        assert_eq!(
            db.insert("retries")
                .value("id", id)
                .value("other", other)
                .on_conflict_do_nothing(["id"])
                .execute_count(&db)
                .await
                .unwrap_err()
                .classification(),
            Some(DatabaseErrorKind::Constraint)
        );
    }
    for (id, other) in [
        (1, switchy_database::DatabaseValue::Int64(2)),
        (2, switchy_database::DatabaseValue::Int64(1)),
        (2, switchy_database::DatabaseValue::Int64(0)),
        (2, switchy_database::DatabaseValue::Null),
    ] {
        assert_eq!(
            db.insert("retries")
                .value("id", id)
                .value("other", other)
                .or_ignore()
                .execute_count(&db)
                .await
                .unwrap(),
            0
        );
    }
    db.sqlite_backup_to(&backup).await.unwrap();
    db.close().await.unwrap();
    let copied = open_sqlite_rusqlite(&backup, &options()).unwrap();
    assert!(copied.trigger_exists(trigger).await.unwrap());
    assert_eq!(
        copied
            .select("attempts")
            .execute(&copied)
            .await
            .unwrap()
            .len(),
        3
    );
    assert_eq!(
        copied
            .insert("attempts")
            .value("run_id", 1)
            .value("concurrency_cap", 1)
            .value("generation", 1)
            .execute(&copied)
            .await
            .unwrap_err()
            .classification(),
        Some(DatabaseErrorKind::Constraint)
    );
    copied.drop_trigger(trigger, false).await.unwrap();
    copied.drop_trigger(trigger, true).await.unwrap();
    copied.close().await.unwrap();
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(backup).unwrap();
    std::fs::remove_dir(directory).unwrap();
}
