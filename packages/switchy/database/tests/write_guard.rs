//! Write-guard backend conformance tests.
//!
//! On 2026-10-04 the actual user approved direct SQL ONLY here to exercise
//! unchanged old-writer INSERT/UPDATE/UPSERT, conflict clauses, prepared statements,
//! and their observable outcomes: "yes, but make sure its only enabled for tests
//! if possible". Schema fixtures and guard installation remain typed. These driver
//! calls live in this integration-test target only; no production raw SQL API or
//! `raw-sql` feature is enabled or broadened by this exception.
//!
//! Remove the exception and direct calls when an unchanged-old-artifact conformance
//! harness replaces these statement-level tests with equivalent coverage, or when
//! this old-writer compatibility contract is explicitly retired. This approval
//! does not cover other fixtures, migrations, production callers or test files.

#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]
use std::sync::Arc;
use switchy_database::{
    Database, TransactionMode,
    rusqlite::RusqliteDatabase,
    schema::{Column, DataType, write_guard::*},
};
fn guard(event: GuardEvent) -> WriteGuard {
    WriteGuard {
        name: format!("guard_{event:?}"),
        table: "legacy".into(),
        event,
        violation: "identity_conflict".into(),
        reject_when: GuardPredicate::Exists(GuardRelation {
            table: "modern".into(),
            alias: "other".into(),
            joins: vec![],
            predicate: Box::new(GuardPredicate::Equal(
                GuardValue::NewColumn("id".into()),
                GuardValue::Column {
                    relation: "other".into(),
                    column: "id".into(),
                },
            )),
        }),
    }
}
async fn fixture() -> RusqliteDatabase {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    for name in ["legacy", "modern"] {
        db.create_table(name)
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
    }
    db.insert("modern")
        .value("id", 7_i64)
        .execute(&db)
        .await
        .unwrap();
    db
}
#[tokio::test]
async fn oversized_guards_and_dropped_installation_fail_without_publication() {
    let db = fixture().await;
    let tx = db.begin_transaction().await.unwrap();
    let mut oversized = guard(GuardEvent::BeforeInsert);
    oversized.reject_when = GuardPredicate::All(vec![
        GuardPredicate::IsNull(GuardValue::NewColumn(
            "id".into()
        ));
        4097
    ]);
    assert!(tx.install_write_guard(&oversized).await.is_err());
    oversized.reject_when =
        GuardPredicate::IsNull(GuardValue::Constant(GuardScalar::Text("x".repeat(262_145))));
    assert!(tx.install_write_guard(&oversized).await.is_err());
    oversized.reject_when = GuardPredicate::IsNull(GuardValue::NewColumn("id".into()));
    for _ in 0..66 {
        oversized.reject_when = GuardPredicate::All(vec![oversized.reject_when]);
    }
    assert!(tx.install_write_guard(&oversized).await.is_err());
    tx.install_write_guard(&guard(GuardEvent::BeforeInsert))
        .await
        .unwrap();
    drop(tx);
    assert!(
        !db.verify_write_guard(&guard(GuardEvent::BeforeInsert))
            .await
            .unwrap()
    );
    db.insert("legacy")
        .value("id", 7_i64)
        .execute(&db)
        .await
        .unwrap();
}

#[tokio::test]
async fn two_connections_racing_same_identity_cannot_commit_both_families() {
    let path = std::env::temp_dir().join(format!(
        "switchy-guards-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open(&path).unwrap(),
    ))]);
    for table in ["legacy", "modern"] {
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
    }
    let tx = db
        .begin_transaction_with_mode(TransactionMode::Immediate)
        .await
        .unwrap();
    tx.install_write_guard(&guard(GuardEvent::BeforeInsert))
        .await
        .unwrap();
    let mut reverse = guard(GuardEvent::BeforeInsert);
    reverse.name = "reverse".into();
    reverse.table = "modern".into();
    if let GuardPredicate::Exists(relation) = &mut reverse.reject_when {
        relation.table = "legacy".into();
    }
    tx.install_write_guard(&reverse).await.unwrap();
    tx.commit().await.unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let handles = ["legacy", "modern"].map(|table| {
        let path = path.clone();
        let barrier = Arc::clone(&barrier);
        std::thread::spawn(move || {
            let mut connection = rusqlite::Connection::open(path).unwrap();
            connection
                .busy_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            barrier.wait();
            let tx = connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .unwrap();
            let result = match table {
                "legacy" => tx.execute("INSERT INTO legacy VALUES (1)", []),
                _ => tx.execute("INSERT INTO modern VALUES (1)", []),
            };
            if result.is_ok() {
                tx.commit().unwrap();
                true
            } else {
                false
            }
        })
    });
    let successes = handles
        .into_iter()
        .map(|handle| usize::from(handle.join().unwrap()))
        .sum::<usize>();
    assert_eq!(successes, 1);
    let total = db.select("legacy").execute(&db).await.unwrap().len()
        + db.select("modern").execute(&db).await.unwrap().len();
    assert_eq!(total, 1);
    db.close().await.unwrap();
    std::fs::remove_file(path).unwrap();
}

#[tokio::test]
async fn installation_is_transactional_idempotent_and_conflicting_definitions_fail() {
    let db = fixture().await;
    let guard = guard(GuardEvent::BeforeInsert);
    assert!(db.install_write_guard(&guard).await.is_err());
    let tx = db
        .begin_transaction_with_mode(TransactionMode::Immediate)
        .await
        .unwrap();
    tx.install_write_guard(&guard).await.unwrap();
    assert!(tx.verify_write_guard(&guard).await.unwrap());
    assert!(
        tx.verify_write_guards(std::slice::from_ref(&guard))
            .await
            .unwrap()
    );
    tx.install_write_guard(&guard).await.unwrap();
    let mut conflict = guard.clone();
    conflict.violation = "different".into();
    assert!(tx.install_write_guard(&conflict).await.is_err());
    assert!(
        !tx.verify_write_guards(std::slice::from_ref(&conflict))
            .await
            .unwrap()
    );
    tx.rollback().await.unwrap();
    assert!(!db.verify_write_guard(&guard).await.unwrap());
    assert!(
        !db.verify_write_guards(std::slice::from_ref(&guard))
            .await
            .unwrap()
    );
    let tx = db.begin_transaction().await.unwrap();
    tx.install_write_guard(&guard).await.unwrap();
    tx.commit().await.unwrap();
    assert!(db.verify_write_guard(&guard).await.unwrap());
    assert!(
        db.insert("legacy")
            .value("id", 7_i64)
            .execute(&db)
            .await
            .is_err()
    );
    db.insert("legacy")
        .value("id", 8_i64)
        .execute(&db)
        .await
        .unwrap();
}
#[tokio::test]
async fn equality_join_correlates_new_row_and_prepared_statement_recompiles() {
    let connection = Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ));
    let db = RusqliteDatabase::new(vec![Arc::clone(&connection)]);
    for table in ["legacy", "modern"] {
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
    }
    db.insert("modern")
        .value("id", 7_i64)
        .execute(&db)
        .await
        .unwrap();
    drop(db);
    let connection = connection.lock().await;
    let mut old_statement = connection
        .prepare("INSERT INTO legacy VALUES (?1)")
        .unwrap();
    let tx = connection.unchecked_transaction().unwrap();
    let mut joined = guard(GuardEvent::BeforeInsert);
    if let GuardPredicate::Exists(relation) = &mut joined.reject_when {
        relation.joins.push(GuardJoin {
            table: "modern".into(),
            alias: "joined".into(),
            left: GuardValue::Column {
                relation: "other".into(),
                column: "id".into(),
            },
            right: GuardValue::Column {
                relation: "joined".into(),
                column: "id".into(),
            },
        });
    }
    switchy_database::rusqlite::install_write_guard_on_connection(&tx, &joined).unwrap();
    tx.commit().unwrap();
    assert!(old_statement.execute([7_i64]).is_err());
    old_statement.execute([8_i64]).unwrap();
}

#[tokio::test]
async fn hostile_constants_are_data_and_invalid_scopes_fail_closed() {
    let db = fixture().await;
    let tx = db.begin_transaction().await.unwrap();
    let mut hostile = guard(GuardEvent::BeforeInsert);
    hostile.name = "guard\"; DROP TABLE modern; --".into();
    hostile.reject_when = GuardPredicate::Any(vec![
        GuardPredicate::Equal(
            GuardValue::Constant(GuardScalar::Text("'; END; DROP TABLE modern; --".into())),
            GuardValue::Constant(GuardScalar::Text("different".into())),
        ),
        GuardPredicate::All(vec![
            GuardPredicate::IsNotNull(GuardValue::NewColumn("id".into())),
            GuardPredicate::InValues(
                GuardValue::NewColumn("id".into()),
                vec![GuardScalar::Integer(7)],
            ),
        ]),
    ]);
    tx.install_write_guard(&hostile).await.unwrap();
    assert!(tx.verify_write_guard(&hostile).await.unwrap());
    assert!(
        tx.insert("legacy")
            .value("id", 7_i64)
            .execute(tx.as_ref())
            .await
            .is_err()
    );
    tx.insert("legacy")
        .value("id", 8_i64)
        .execute(tx.as_ref())
        .await
        .unwrap();
    for invalid in [
        GuardValue::OldColumn("id".into()),
        GuardValue::NewColumn("id\0".into()),
        GuardValue::Column {
            relation: "unbound".into(),
            column: "id".into(),
        },
    ] {
        let mut candidate = hostile.clone();
        candidate.name = "invalid".into();
        candidate.reject_when = GuardPredicate::IsNull(invalid);
        assert!(tx.install_write_guard(&candidate).await.is_err());
    }
    tx.commit().await.unwrap();
    assert_eq!(db.select("modern").execute(&db).await.unwrap().len(), 1);
}

#[tokio::test]
async fn unmodified_sqlite_writers_cannot_ignore_or_replace_rejection() {
    let connection = Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ));
    let db = RusqliteDatabase::new(vec![Arc::clone(&connection)]);
    for name in ["legacy", "modern"] {
        db.create_table(name)
            .unique_columns(["id"])
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
    }
    db.insert("modern")
        .value("id", 7_i64)
        .execute(&db)
        .await
        .unwrap();
    drop(db);
    let mut connection = connection.lock().await;
    let tx = connection.transaction().unwrap();
    for event in [GuardEvent::BeforeInsert, GuardEvent::BeforeUpdate] {
        switchy_database::rusqlite::install_write_guard_on_connection(&tx, &guard(event)).unwrap();
    }
    tx.commit().unwrap();
    // Backend conformance inputs model already-built SQLite callers, not a raw public API.
    for sql in [
        "INSERT INTO legacy VALUES (7)",
        "INSERT OR IGNORE INTO legacy VALUES (7)",
        "INSERT OR REPLACE INTO legacy VALUES (7)",
        "INSERT INTO legacy VALUES (8),(7)",
    ] {
        assert!(connection.execute(sql, []).is_err(), "{sql}");
    }
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM legacy", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    connection
        .execute("INSERT INTO legacy VALUES (8)", [])
        .unwrap();
    for sql in [
        "UPDATE legacy SET id=7",
        "UPDATE OR IGNORE legacy SET id=7",
        "UPDATE OR REPLACE legacy SET id=7",
        "INSERT INTO legacy VALUES (8) ON CONFLICT(id) DO UPDATE SET id=7",
        "INSERT OR IGNORE INTO legacy VALUES (8) ON CONFLICT(id) DO UPDATE SET id=7",
    ] {
        assert!(connection.execute(sql, []).is_err(), "{sql}");
    }
    assert_eq!(
        connection
            .query_row("SELECT id FROM legacy", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        8
    );
}
