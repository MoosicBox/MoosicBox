#![cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]

use std::sync::Arc;
use switchy_database::{
    Database, DatabaseError,
    rusqlite::{RusqliteDatabase, TransactionMode},
    schema::{Column, DataType},
};

#[tokio::test]
async fn leased_transaction_is_not_reused_and_drop_rolls_back() {
    let db = RusqliteDatabase::new(vec![Arc::new(switchy_async::sync::Mutex::new(
        rusqlite::Connection::open_in_memory().unwrap(),
    ))]);
    db.create_table("items")
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
    for mode in [
        TransactionMode::Deferred,
        TransactionMode::Immediate,
        TransactionMode::Exclusive,
    ] {
        let tx = db.transaction_with_mode(mode).await.unwrap();
        tx.insert("items")
            .value("id", 1)
            .execute(tx.as_ref())
            .await
            .unwrap();
        assert!(matches!(
            db.select("items").execute(&db).await,
            Err(DatabaseError::Busy)
        ));
        drop(tx);
        assert!(db.select("items").execute(&db).await.unwrap().is_empty());
    }
    let tx = db.begin_transaction().await.unwrap();
    tx.insert("items")
        .value("id", 2)
        .execute(tx.as_ref())
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(db.select("items").execute(&db).await.unwrap().len(), 1);
    let tx = db.begin_transaction().await.unwrap();
    assert!(matches!(db.trigger_close(), Err(DatabaseError::Busy)));
    assert_eq!(
        tx.exec_update_count(&tx.update("items").value("id", 3))
            .await
            .unwrap(),
        1
    );
    assert_eq!(tx.exec_delete_count(&tx.delete("items")).await.unwrap(), 1);
    assert_eq!(tx.exec_delete_count(&tx.delete("items")).await.unwrap(), 0);
    tx.rollback().await.unwrap();
    assert_eq!(db.select("items").execute(&db).await.unwrap().len(), 1);
    db.close().await.unwrap();
    assert!(matches!(
        db.select("items").execute(&db).await,
        Err(DatabaseError::ConnectionClosed)
    ));
}
