//! Workflow-store prerequisites exercised without raw SQL or direct-driver fixtures.
#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::{num::NonZeroUsize, time::Duration};
use switchy_database::{
    Database, DatabaseError, TransactionMode,
    query::FilterableQuery,
    schema::{Column, DataType},
};
use switchy_database_connection::{SqliteAccess, SqliteOpenOptions, open_sqlite_rusqlite};

#[tokio::test]
async fn immediate_read_modify_write_is_affine_and_releases_on_every_exit() {
    let path = std::env::temp_dir().join(format!(
        "switchy-workflow-affinity-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let options = SqliteOpenOptions {
        access: SqliteAccess::ReadWriteCreate,
        allow_uri: false,
        busy_timeout: Duration::ZERO,
        foreign_keys: true,
        connections: NonZeroUsize::new(1).unwrap(),
    };
    let owner = open_sqlite_rusqlite(&path, &options).unwrap();
    let contender = open_sqlite_rusqlite(&path, &options).unwrap();
    owner
        .create_table("epochs")
        .column(Column {
            name: "generation".into(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .execute(&owner)
        .await
        .unwrap();
    owner
        .insert("epochs")
        .value("generation", 1)
        .execute(&owner)
        .await
        .unwrap();

    owner
        .sqlite_journal_mode(switchy_database::SqliteJournalMode::Delete)
        .await
        .unwrap();
    let transaction = owner.begin_transaction().await.unwrap();
    let integrity = transaction.sqlite_integrity_check().await.unwrap();
    assert_eq!(integrity.len(), 1);
    assert_eq!(
        integrity[0].columns[0].1,
        switchy_database::DatabaseValue::String("ok".into())
    );
    assert!(
        transaction
            .sqlite_foreign_key_check()
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        transaction
            .sqlite_journal_mode(switchy_database::SqliteJournalMode::Delete)
            .await
            .is_err()
    );
    transaction.rollback().await.unwrap();
    for commit in [false, true] {
        let tx = owner
            .begin_transaction_with_mode(TransactionMode::Immediate)
            .await
            .unwrap();
        assert_eq!(
            tx.select("epochs")
                .execute(tx.as_ref())
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            tx.exec_update_count(
                &tx.update("epochs")
                    .value("generation", 2)
                    .where_eq("generation", 1)
            )
            .await
            .unwrap(),
            1
        );
        assert_eq!(
            tx.select("epochs")
                .where_eq("generation", 2)
                .execute(tx.as_ref())
                .await
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            tx.exec_update_count(
                &tx.update("epochs")
                    .value("generation", 3)
                    .where_eq("generation", 1)
            )
            .await
            .unwrap(),
            0
        );
        // The owner's sole connection remains leased, not silently reused.
        assert!(matches!(
            owner.select("epochs").execute(&owner).await,
            Err(DatabaseError::Busy)
        ));
        assert!(matches!(owner.trigger_close(), Err(DatabaseError::Busy)));
        assert!(matches!(
            contender
                .begin_transaction_with_mode(TransactionMode::Immediate)
                .await,
            Err(DatabaseError::Busy | DatabaseError::Locked)
        ));
        if commit {
            tx.commit().await.unwrap();
        } else {
            drop(tx);
        }
        let expected = if commit { 2 } else { 1 };
        let tx = contender
            .begin_transaction_with_mode(TransactionMode::Immediate)
            .await
            .unwrap();
        assert_eq!(
            tx.select("epochs")
                .where_eq("generation", expected)
                .execute(tx.as_ref())
                .await
                .unwrap()
                .len(),
            1
        );
        tx.exec_update_count(&tx.update("epochs").value("generation", 99))
            .await
            .unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(
            owner
                .select("epochs")
                .where_eq("generation", expected)
                .execute(&owner)
                .await
                .unwrap()
                .len(),
            1
        );
    }
    owner.close().await.unwrap();
    contender.close().await.unwrap();
    assert!(matches!(
        owner.select("epochs").execute(&owner).await,
        Err(DatabaseError::ConnectionClosed)
    ));
    let reopened = open_sqlite_rusqlite(&path, &options).unwrap();
    let tx = reopened
        .begin_transaction_with_mode(TransactionMode::Immediate)
        .await
        .unwrap();
    assert_eq!(
        tx.select("epochs")
            .where_eq("generation", 2)
            .execute(tx.as_ref())
            .await
            .unwrap()
            .len(),
        1
    );
    tx.rollback().await.unwrap();
    reopened.close().await.unwrap();
    std::fs::remove_file(path).unwrap();
}
