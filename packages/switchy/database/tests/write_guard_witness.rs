//! Typed guard behavior without the raw-sql feature or direct-driver statements.
#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
use std::sync::Arc;
use switchy_database::{
    Database, DatabaseValue, TransactionMode,
    query::FilterableQuery,
    rusqlite::RusqliteDatabase,
    schema::{
        Column, DataType,
        write_guard::{
            GuardEvent, GuardPredicate, GuardRelation, GuardScalar, GuardValue, WriteGuard,
        },
    },
};

const fn constant(value: i64) -> GuardValue {
    GuardValue::Constant(GuardScalar::Integer(value))
}
fn relation(table: &str, predicate: GuardPredicate) -> GuardRelation {
    GuardRelation {
        table: table.into(),
        alias: "w".into(),
        joins: vec![],
        predicate: Box::new(predicate),
    }
}
fn witness_column(name: &str) -> GuardValue {
    GuardValue::Column {
        relation: "w".into(),
        column: name.into(),
    }
}
async fn fixture() -> RusqliteDatabase {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    for (table, names) in [
        ("rows", vec!["id", "revision"]),
        ("opt_in", vec!["id"]),
        ("witness", vec!["id", "previous", "next"]),
    ] {
        let mut schema = db.create_table(table);
        for name in names {
            schema = schema.column(Column {
                name: name.into(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: false,
                default: None,
            });
        }
        schema.execute(&db).await.unwrap();
    }
    db
}
fn protected_update() -> WriteGuard {
    WriteGuard {
        name: "protected_update".into(),
        table: "rows".into(),
        event: GuardEvent::BeforeUpdate,
        violation: "missing_fresh_witness".into(),
        reject_when: GuardPredicate::All(vec![
            GuardPredicate::Exists(relation(
                "opt_in",
                GuardPredicate::Equal(witness_column("id"), GuardValue::OldColumn("id".into())),
            )),
            GuardPredicate::Any(vec![
                GuardPredicate::Not(Box::new(GuardPredicate::IntegerSuccessor {
                    previous: GuardValue::OldColumn("revision".into()),
                    next: GuardValue::NewColumn("revision".into()),
                })),
                GuardPredicate::NotEqual(
                    GuardValue::OldColumn("id".into()),
                    GuardValue::NewColumn("id".into()),
                ),
                GuardPredicate::NotExists(relation(
                    "witness",
                    GuardPredicate::All(vec![
                        GuardPredicate::Equal(
                            witness_column("id"),
                            GuardValue::OldColumn("id".into()),
                        ),
                        GuardPredicate::Equal(
                            witness_column("previous"),
                            GuardValue::OldColumn("revision".into()),
                        ),
                        GuardPredicate::Equal(
                            witness_column("next"),
                            GuardValue::NewColumn("revision".into()),
                        ),
                    ]),
                )),
            ]),
        ]),
    }
}

#[tokio::test]
async fn opted_in_transition_requires_fresh_witness_and_independent_rows_remain_writable() {
    let db = fixture().await;
    let tx = db
        .begin_transaction_with_mode(TransactionMode::Immediate)
        .await
        .unwrap();
    tx.install_write_guard(&protected_update()).await.unwrap();
    tx.commit().await.unwrap();
    for id in [1_i64, 2] {
        db.insert("rows")
            .value("id", id)
            .value("revision", 0_i64)
            .execute(&db)
            .await
            .unwrap();
    }
    db.insert("opt_in")
        .value("id", 1_i64)
        .execute(&db)
        .await
        .unwrap();
    assert!(
        db.update("rows")
            .value("revision", 1_i64)
            .where_eq("id", 1_i64)
            .execute(&db)
            .await
            .is_err()
    );
    db.update("rows")
        .value("revision", 1_i64)
        .where_eq("id", 2_i64)
        .execute(&db)
        .await
        .unwrap();
    // Witness and transition roll back together.
    let tx = db
        .begin_transaction_with_mode(TransactionMode::Immediate)
        .await
        .unwrap();
    tx.insert("witness")
        .value("id", 1_i64)
        .value("previous", 0_i64)
        .value("next", 1_i64)
        .execute(&*tx)
        .await
        .unwrap();
    tx.update("rows")
        .value("revision", 1_i64)
        .where_eq("id", 1_i64)
        .execute(&*tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert!(
        db.update("rows")
            .value("revision", 1_i64)
            .where_eq("id", 1_i64)
            .execute(&db)
            .await
            .is_err()
    );
    let tx = db
        .begin_transaction_with_mode(TransactionMode::Immediate)
        .await
        .unwrap();
    tx.insert("witness")
        .value("id", 1_i64)
        .value("previous", 0_i64)
        .value("next", 1_i64)
        .execute(&*tx)
        .await
        .unwrap();
    tx.update("rows")
        .value("revision", 1_i64)
        .where_eq("id", 1_i64)
        .execute(&*tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    // The committed witness cannot authorize a repeated, skipped, or subsequent transition.
    for next in [1_i64, 2, 3] {
        assert!(
            db.update("rows")
                .value("revision", next)
                .where_eq("id", 1_i64)
                .execute(&db)
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn successor_is_exact_and_null_type_and_overflow_fail_closed() {
    let db = fixture().await;
    for (index, previous, next, accepted) in [
        (0, constant(0), constant(1), true),
        (1, constant(-1), constant(0), true),
        (2, constant(i64::MAX - 1), constant(i64::MAX), true),
        (3, constant(i64::MAX), constant(i64::MIN), false),
        (4, constant(0), constant(2), false),
        (
            5,
            GuardValue::Constant(GuardScalar::Null),
            constant(1),
            false,
        ),
        (
            6,
            GuardValue::Constant(GuardScalar::Text("0".into())),
            constant(1),
            false,
        ),
        (7, constant(i64::MAX), constant(i64::MAX), false),
        (
            8,
            constant(0),
            GuardValue::Constant(GuardScalar::Null),
            false,
        ),
    ] {
        let guard = WriteGuard {
            name: format!("successor_{index}"),
            table: "rows".into(),
            event: GuardEvent::BeforeInsert,
            violation: "not_successor".into(),
            reject_when: GuardPredicate::All(vec![
                GuardPredicate::Equal(GuardValue::NewColumn("id".into()), constant(index)),
                GuardPredicate::Not(Box::new(GuardPredicate::IntegerSuccessor {
                    previous,
                    next,
                })),
            ]),
        };
        let tx = db.begin_transaction().await.unwrap();
        tx.install_write_guard(&guard).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            db.insert("rows")
                .value("id", index)
                .value("revision", 0_i64)
                .execute(&db)
                .await
                .is_ok(),
            accepted
        );
    }
}

#[tokio::test]
async fn delete_uses_old_and_invalid_row_references_are_rejected() {
    let db = fixture().await;
    let guard = WriteGuard {
        name: "protect_delete".into(),
        table: "rows".into(),
        event: GuardEvent::BeforeDelete,
        violation: "immutable_row".into(),
        reject_when: GuardPredicate::Equal(GuardValue::OldColumn("id".into()), constant(1)),
    };
    let tx = db.begin_transaction().await.unwrap();
    tx.install_write_guard(&guard).await.unwrap();
    let mut invalid = guard.clone();
    invalid.name = "invalid_new_on_delete".into();
    invalid.reject_when = GuardPredicate::IsNull(GuardValue::NewColumn("id".into()));
    assert!(tx.install_write_guard(&invalid).await.is_err());
    invalid.name = "invalid_old_on_insert".into();
    invalid.event = GuardEvent::BeforeInsert;
    invalid.reject_when = GuardPredicate::IsNull(GuardValue::OldColumn("id".into()));
    assert!(tx.install_write_guard(&invalid).await.is_err());
    tx.commit().await.unwrap();
    for id in [1_i64, 2] {
        db.insert("rows")
            .value("id", id)
            .value("revision", 0_i64)
            .execute(&db)
            .await
            .unwrap();
    }
    assert!(
        db.delete("rows")
            .where_eq("id", 1_i64)
            .execute(&db)
            .await
            .is_err()
    );
    db.delete("rows")
        .where_eq("id", 2_i64)
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(
        db.select("rows")
            .where_eq("id", 1_i64)
            .execute(&db)
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        db.select("rows")
            .where_eq("id", 2_i64)
            .execute(&db)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn nullable_negation_rejects_unknown_and_allows_only_true() {
    let db = fixture().await;
    db.create_table("nullable_rows")
        .column(Column {
            name: "value".into(),
            data_type: DataType::BigInt,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .execute(&db)
        .await
        .unwrap();
    let guard = WriteGuard {
        name: "nullable_not".into(),
        table: "nullable_rows".into(),
        event: GuardEvent::BeforeInsert,
        violation: "unknown_or_false".into(),
        reject_when: GuardPredicate::Not(Box::new(GuardPredicate::Equal(
            GuardValue::NewColumn("value".into()),
            constant(1),
        ))),
    };
    let tx = db.begin_transaction().await.unwrap();
    tx.install_write_guard(&guard).await.unwrap();
    tx.commit().await.unwrap();
    assert!(
        db.insert("nullable_rows")
            .value("value", DatabaseValue::Null)
            .execute(&db)
            .await
            .is_err()
    );
    assert!(
        db.insert("nullable_rows")
            .value("value", 0_i64)
            .execute(&db)
            .await
            .is_err()
    );
    db.insert("nullable_rows")
        .value("value", 1_i64)
        .execute(&db)
        .await
        .unwrap();
}

#[tokio::test]
async fn maximum_predecessor_never_accepts_promoted_real_successor() {
    let db = fixture().await;
    db.create_table("numeric_rows")
        .column(Column {
            name: "next".into(),
            data_type: DataType::Double,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .execute(&db)
        .await
        .unwrap();
    let guard = WriteGuard {
        name: "maximum_successor".into(),
        table: "numeric_rows".into(),
        event: GuardEvent::BeforeInsert,
        violation: "overflow_or_noninteger".into(),
        reject_when: GuardPredicate::Not(Box::new(GuardPredicate::IntegerSuccessor {
            previous: constant(i64::MAX),
            next: GuardValue::NewColumn("next".into()),
        })),
    };
    let tx = db.begin_transaction().await.unwrap();
    tx.install_write_guard(&guard).await.unwrap();
    tx.commit().await.unwrap();
    // Exactly 2^63: SQLite's overflowing integer addition would promote to this real.
    assert!(
        db.insert("numeric_rows")
            .value("next", DatabaseValue::Real64(9_223_372_036_854_775_808.0))
            .execute(&db)
            .await
            .is_err()
    );
    assert!(
        db.insert("numeric_rows")
            .value("next", DatabaseValue::Null)
            .execute(&db)
            .await
            .is_err()
    );
}
