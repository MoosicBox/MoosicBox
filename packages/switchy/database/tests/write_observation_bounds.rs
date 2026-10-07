//! Finite byte bounds preserve transitions without copying unavailable facts.
#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
use std::sync::Arc;
use switchy_database::{
    Database, DatabaseValue,
    query::FilterableQuery,
    rusqlite::RusqliteDatabase,
    schema::{
        Column, DataType,
        write_guard::{
            GuardEvent, GuardPredicate, GuardScalar, GuardValue, ObservationColumn,
            ObservationEvent, WriteGuard, WriteObservation,
        },
    },
};

async fn fixture() -> RusqliteDatabase {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    for table in ["source", "events"] {
        let mut schema = db.create_table(table);
        for (name, data_type) in [
            ("id", DataType::BigInt),
            ("fact", DataType::Text),
            ("status", DataType::BigInt),
        ] {
            schema = schema.column(Column {
                name: name.into(),
                data_type,
                nullable: name == "fact",
                auto_increment: false,
                default: None,
            });
        }
        schema.execute(&db).await.unwrap();
    }
    db
}
fn bounded(status: bool) -> GuardValue {
    let value = Box::new(GuardValue::NewColumn("fact".into()));
    if status {
        GuardValue::BoundedTextStatus {
            value,
            max_bytes: 4,
        }
    } else {
        GuardValue::BoundedText {
            value,
            max_bytes: 4,
        }
    }
}
fn observation() -> WriteObservation {
    WriteObservation {
        name: "bounded_projection".into(),
        table: "source".into(),
        event: ObservationEvent::AfterInsert,
        when: GuardPredicate::Equal(
            GuardValue::Constant(GuardScalar::Integer(1)),
            GuardValue::Constant(GuardScalar::Integer(1)),
        ),
        target_table: "events".into(),
        columns: vec![
            ObservationColumn {
                column: "id".into(),
                value: GuardValue::NewColumn("id".into()),
            },
            ObservationColumn {
                column: "fact".into(),
                value: bounded(false),
            },
            ObservationColumn {
                column: "status".into(),
                value: bounded(true),
            },
        ],
    }
}
#[tokio::test]
async fn complete_utf8_nul_empty_and_unavailable_facts_are_distinguished() {
    let db = fixture().await;
    let tx = db.begin_transaction().await.unwrap();
    tx.install_write_observation(&observation()).await.unwrap();
    tx.commit().await.unwrap();
    for (id, source, expected, status) in [
        (0_i64, DatabaseValue::Null, DatabaseValue::Null, 0_i64),
        (
            1,
            DatabaseValue::String(String::new()),
            DatabaseValue::String(String::new()),
            1,
        ),
        (
            2,
            DatabaseValue::String("éé".into()),
            DatabaseValue::String("éé".into()),
            1,
        ),
        (
            3,
            DatabaseValue::String("ééa".into()),
            DatabaseValue::Null,
            2,
        ),
        (
            4,
            DatabaseValue::String("a\0bc".into()),
            DatabaseValue::String("a\0bc".into()),
            1,
        ),
        (
            5,
            DatabaseValue::String("a\0bcd".into()),
            DatabaseValue::Null,
            2,
        ),
        (
            6,
            DatabaseValue::String("x".repeat(65_536)),
            DatabaseValue::Null,
            2,
        ),
    ] {
        db.insert("source")
            .value("id", id)
            .value("fact", source.clone())
            .value("status", 0_i64)
            .execute(&db)
            .await
            .unwrap();
        let rows = db
            .select("events")
            .where_eq("id", id)
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("fact"), Some(expected));
        assert_eq!(rows[0].get("status"), Some(DatabaseValue::Int64(status)));
        assert_eq!(
            db.select("source")
                .where_eq("id", id)
                .execute(&db)
                .await
                .unwrap()[0]
                .get("fact"),
            Some(source)
        );
    }
}
#[tokio::test]
async fn predicate_bounds_bytes_and_rejects_null_and_numeric_storage() {
    let db = fixture().await;
    let tx = db.begin_transaction().await.unwrap();
    // Constants avoid column affinity coercion when exercising unsupported types.
    for (id, input, limit, allowed) in [
        (0_i64, GuardScalar::Text("é".into()), 2, true),
        (1, GuardScalar::Text("é".into()), 1, false),
        (2, GuardScalar::Text(String::new()), 0, true),
        (3, GuardScalar::Null, 4, false),
        (4, GuardScalar::Integer(1), 4, false),
    ] {
        tx.install_write_guard(&WriteGuard {
            name: format!("byte_bound_{id}"),
            table: "source".into(),
            event: GuardEvent::BeforeInsert,
            violation: "byte_bound_failed".into(),
            reject_when: GuardPredicate::All(vec![
                GuardPredicate::Equal(
                    GuardValue::NewColumn("id".into()),
                    GuardValue::Constant(GuardScalar::Integer(id)),
                ),
                GuardPredicate::Not(Box::new(GuardPredicate::ByteLengthAtMost(
                    GuardValue::Constant(input),
                    limit,
                ))),
            ]),
        })
        .await
        .unwrap();
        assert_eq!(
            tx.insert("source")
                .value("id", id)
                .value("fact", DatabaseValue::Null)
                .value("status", 0_i64)
                .execute(&*tx)
                .await
                .is_ok(),
            allowed
        );
    }
    tx.rollback().await.unwrap();
}
#[tokio::test]
async fn integer_projection_retains_extremes_and_marks_wrong_types_without_copying() {
    let db = fixture().await;
    db.create_table("integer_events")
        .column(Column {
            name: "id".into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "fact".into(),
            data_type: DataType::BigInt,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "status".into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .execute(&db)
        .await
        .unwrap();
    for (id, source, expected, status) in [
        (
            0_i64,
            GuardValue::Constant(GuardScalar::Null),
            DatabaseValue::Null,
            0_i64,
        ),
        (
            1,
            GuardValue::Constant(GuardScalar::Integer(i64::MIN)),
            DatabaseValue::Int64(i64::MIN),
            1,
        ),
        (
            2,
            GuardValue::Constant(GuardScalar::Integer(i64::MAX)),
            DatabaseValue::Int64(i64::MAX),
            1,
        ),
        (
            3,
            GuardValue::NewColumn("fact".into()),
            DatabaseValue::Null,
            2,
        ),
    ] {
        let mut mapping = observation();
        mapping.name = format!("integer_projection_{id}");
        mapping.target_table = "integer_events".into();
        mapping.when = GuardPredicate::Equal(
            GuardValue::NewColumn("id".into()),
            GuardValue::Constant(GuardScalar::Integer(id)),
        );
        mapping.columns[1].value = GuardValue::IntegerOnly {
            value: Box::new(source.clone()),
        };
        mapping.columns[2].value = GuardValue::IntegerStatus {
            value: Box::new(source),
        };
        let tx = db.begin_transaction().await.unwrap();
        tx.install_write_observation(&mapping).await.unwrap();
        let mut invalid = mapping.clone();
        invalid.name = format!("nested_integer_{id}");
        invalid.columns[1].value = GuardValue::IntegerOnly {
            value: Box::new(bounded(false)),
        };
        assert!(tx.install_write_observation(&invalid).await.is_err());
        tx.commit().await.unwrap();
        // A huge malformed source remains a valid source write; only the projection is NULL.
        db.insert("source")
            .value("id", id)
            .value("fact", "9".repeat(65_536))
            .value("status", 0_i64)
            .execute(&db)
            .await
            .unwrap();
        let rows = db
            .select("integer_events")
            .where_eq("id", id)
            .execute(&db)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("fact"), Some(expected));
        assert_eq!(rows[0].get("status"), Some(DatabaseValue::Int64(status)));
    }
}

#[tokio::test]
async fn numeric_mapping_is_unavailable_and_nested_mapping_is_rejected() {
    let db = fixture().await;
    let tx = db.begin_transaction().await.unwrap();
    let mut mapping = observation();
    mapping.columns[1].value = GuardValue::BoundedText {
        value: Box::new(GuardValue::Constant(GuardScalar::Integer(1))),
        max_bytes: 4,
    };
    mapping.columns[2].value = GuardValue::BoundedTextStatus {
        value: Box::new(GuardValue::Constant(GuardScalar::Integer(1))),
        max_bytes: 4,
    };
    tx.install_write_observation(&mapping).await.unwrap();
    let mut invalid = mapping.clone();
    invalid.name = "nested_mapping".into();
    invalid.columns[1].value = GuardValue::BoundedText {
        value: Box::new(bounded(false)),
        max_bytes: 4,
    };
    assert!(tx.install_write_observation(&invalid).await.is_err());
    tx.commit().await.unwrap();
    db.insert("source")
        .value("id", 1_i64)
        .value("fact", "source retained")
        .value("status", 0_i64)
        .execute(&db)
        .await
        .unwrap();
    let rows = db.select("events").execute(&db).await.unwrap();
    assert_eq!(rows[0].get("fact"), Some(DatabaseValue::Null));
    assert_eq!(rows[0].get("status"), Some(DatabaseValue::Int64(3)));
}
