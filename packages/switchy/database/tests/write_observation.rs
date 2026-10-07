//! Automatic observation conformance using only typed consumer statements.
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
            GuardEvent, GuardPredicate, GuardRelation, GuardScalar, GuardValue, ObservationColumn,
            ObservationEvent, WriteGuard, WriteObservation,
        },
    },
};

async fn fixture() -> RusqliteDatabase {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    for (table, fields) in [
        ("source", vec!["id", "value"]),
        ("opt_in", vec!["id"]),
        ("events", vec!["sequence", "id", "before", "after", "kind"]),
    ] {
        let mut schema = db.create_table(table);
        for field in fields {
            schema = schema.column(Column {
                name: field.into(),
                data_type: if field == "sequence" {
                    DataType::Int
                } else {
                    DataType::BigInt
                },
                nullable: field == "before" || field == "after",
                auto_increment: field == "sequence",
                default: None,
            });
        }
        if table == "events" {
            schema = schema.primary_key("sequence");
        }
        schema.execute(&db).await.unwrap();
    }
    db
}
fn observation(event: ObservationEvent) -> WriteObservation {
    let (row, before, after, kind) = match event {
        ObservationEvent::AfterInsert => (
            GuardValue::NewColumn("id".into()),
            GuardValue::Constant(GuardScalar::Null),
            GuardValue::NewColumn("value".into()),
            1,
        ),
        ObservationEvent::AfterUpdate => (
            GuardValue::NewColumn("id".into()),
            GuardValue::OldColumn("value".into()),
            GuardValue::NewColumn("value".into()),
            2,
        ),
        ObservationEvent::AfterDelete => (
            GuardValue::OldColumn("id".into()),
            GuardValue::OldColumn("value".into()),
            GuardValue::Constant(GuardScalar::Null),
            3,
        ),
    };
    WriteObservation {
        name: format!("observe_{event:?}"),
        table: "source".into(),
        event,
        when: GuardPredicate::Exists(GuardRelation {
            table: "opt_in".into(),
            alias: "o".into(),
            joins: vec![],
            predicate: Box::new(GuardPredicate::Equal(
                row.clone(),
                GuardValue::Column {
                    relation: "o".into(),
                    column: "id".into(),
                },
            )),
        }),
        target_table: "events".into(),
        columns: [
            ("id", row),
            ("before", before),
            ("after", after),
            ("kind", GuardValue::Constant(GuardScalar::Integer(kind))),
        ]
        .into_iter()
        .map(|(column, value)| ObservationColumn {
            column: column.into(),
            value,
        })
        .collect(),
    }
}

#[tokio::test]
async fn opted_in_insert_update_delete_emit_exact_projection_and_rollback_together() {
    let db = fixture().await;
    assert!(
        db.install_write_observation(&observation(ObservationEvent::AfterInsert))
            .await
            .is_err()
    );
    let observations = [
        ObservationEvent::AfterInsert,
        ObservationEvent::AfterUpdate,
        ObservationEvent::AfterDelete,
    ]
    .map(observation);
    assert!(!db.verify_write_observation(&observations[0]).await.unwrap());
    let tx = db
        .begin_transaction_with_mode(TransactionMode::Immediate)
        .await
        .unwrap();
    for observation in &observations {
        tx.install_write_observation(observation).await.unwrap();
        tx.install_write_observation(observation).await.unwrap();
        assert!(tx.verify_write_observation(observation).await.unwrap());
    }
    tx.commit().await.unwrap();
    assert!(db.verify_write_observations(&observations).await.unwrap());
    db.insert("opt_in")
        .value("id", 1_i64)
        .execute(&db)
        .await
        .unwrap();
    // No knowledge of observations is required by these source writers.
    for id in [1_i64, 2] {
        db.insert("source")
            .value("id", id)
            .value("value", 10_i64)
            .execute(&db)
            .await
            .unwrap();
    }
    let tx = db
        .begin_transaction_with_mode(TransactionMode::Immediate)
        .await
        .unwrap();
    tx.update("source")
        .value("value", 99_i64)
        .where_eq("id", 1_i64)
        .execute(&*tx)
        .await
        .unwrap();
    assert_eq!(tx.select("events").execute(&*tx).await.unwrap().len(), 2);
    tx.rollback().await.unwrap();
    assert_eq!(db.select("events").execute(&db).await.unwrap().len(), 1);
    assert_eq!(
        db.select("source")
            .where_eq("id", 1_i64)
            .execute(&db)
            .await
            .unwrap()[0]
            .get("value"),
        Some(DatabaseValue::Int64(10))
    );
    for id in [1_i64, 2] {
        db.update("source")
            .value("value", 20_i64)
            .where_eq("id", id)
            .execute(&db)
            .await
            .unwrap();
        db.delete("source")
            .where_eq("id", id)
            .execute(&db)
            .await
            .unwrap();
    }
    let events = db.select("events").execute(&db).await.unwrap();
    assert_eq!(events.len(), 3);
    // Lookup by generated sequence rather than relying on unspecified SELECT ordering.
    for (seq, before, after, kind) in [
        (1_i64, DatabaseValue::Null, DatabaseValue::Int64(10), 1_i64),
        (2, DatabaseValue::Int64(10), DatabaseValue::Int64(20), 2),
        (3, DatabaseValue::Int64(20), DatabaseValue::Null, 3),
    ] {
        let rows = db
            .select("events")
            .where_eq("sequence", seq)
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("id"), Some(DatabaseValue::Int64(1)));
        assert_eq!(rows[0].get("before"), Some(before));
        assert_eq!(rows[0].get("after"), Some(after));
        assert_eq!(rows[0].get("kind"), Some(DatabaseValue::Int64(kind)));
    }
}

#[tokio::test]
async fn observation_target_failure_aborts_source_statement() {
    let db = fixture().await;
    let tx = db.begin_transaction().await.unwrap();
    tx.install_write_observation(&observation(ObservationEvent::AfterInsert))
        .await
        .unwrap();
    tx.install_write_guard(&WriteGuard {
        name: "reject_target".into(),
        table: "events".into(),
        event: GuardEvent::BeforeInsert,
        violation: "target_unavailable".into(),
        reject_when: GuardPredicate::Equal(
            GuardValue::NewColumn("id".into()),
            GuardValue::Constant(GuardScalar::Integer(1)),
        ),
    })
    .await
    .unwrap();
    tx.commit().await.unwrap();
    db.insert("opt_in")
        .value("id", 1_i64)
        .execute(&db)
        .await
        .unwrap();
    assert!(
        db.insert("source")
            .value("id", 1_i64)
            .value("value", 10_i64)
            .execute(&db)
            .await
            .is_err()
    );
    assert!(db.select("source").execute(&db).await.unwrap().is_empty());
    assert!(db.select("events").execute(&db).await.unwrap().is_empty());
    db.insert("source")
        .value("id", 2_i64)
        .value("value", 10_i64)
        .execute(&db)
        .await
        .unwrap();
}

#[tokio::test]
async fn absent_observation_does_not_change_independent_writers() {
    let db = fixture().await;
    db.insert("source")
        .value("id", 1_i64)
        .value("value", 10_i64)
        .execute(&db)
        .await
        .unwrap();
    db.update("source")
        .value("value", 20_i64)
        .where_eq("id", 1_i64)
        .execute(&db)
        .await
        .unwrap();
    db.delete("source")
        .where_eq("id", 1_i64)
        .execute(&db)
        .await
        .unwrap();
    assert!(db.select("events").execute(&db).await.unwrap().is_empty());
}

#[tokio::test]
async fn invalid_mappings_and_conflicting_contracts_fail_without_installation() {
    let db = fixture().await;
    let tx = db.begin_transaction().await.unwrap();
    let valid = observation(ObservationEvent::AfterInsert);
    for (index, mut invalid) in [
        valid.clone(),
        valid.clone(),
        valid.clone(),
        valid.clone(),
        valid.clone(),
    ]
    .into_iter()
    .enumerate()
    {
        match index {
            0 => invalid.columns.push(invalid.columns[0].clone()),
            1 => invalid.columns[0].value = GuardValue::OldColumn("id".into()),
            2 => invalid.target_table = "source".into(),
            3 => invalid.columns.clear(),
            _ => invalid.columns[0].column = "bad\0column".into(),
        }
        assert!(tx.install_write_observation(&invalid).await.is_err());
    }
    let mut invalid_delete = observation(ObservationEvent::AfterDelete);
    invalid_delete.columns[0].value = GuardValue::NewColumn("id".into());
    assert!(tx.install_write_observation(&invalid_delete).await.is_err());
    let mut invalid_relation = valid.clone();
    invalid_relation.columns[0].value = GuardValue::Column {
        relation: "o".into(),
        column: "id".into(),
    };
    assert!(
        tx.install_write_observation(&invalid_relation)
            .await
            .is_err()
    );
    let mut oversized = valid.clone();
    oversized.columns = vec![valid.columns[0].clone(); 129];
    assert!(tx.install_write_observation(&oversized).await.is_err());
    tx.install_write_observation(&valid).await.unwrap();
    let mut conflicting = valid.clone();
    conflicting.columns[0].value = GuardValue::Constant(GuardScalar::Integer(7));
    assert!(tx.install_write_observation(&conflicting).await.is_err());
    assert!(!tx.verify_write_observation(&conflicting).await.unwrap());
    tx.rollback().await.unwrap();
    assert!(!db.verify_write_observation(&valid).await.unwrap());
}
