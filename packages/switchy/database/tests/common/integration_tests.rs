use std::sync::Arc;

use switchy_database::{Database, DatabaseValue, Row, query::FilterableQuery as _};

#[allow(unused)]
pub trait IntegrationTestSuite {
    async fn get_database(&self) -> Option<Arc<Box<dyn Database>>>;

    async fn test_insert(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db = &**db;

        db.insert("users")
            .value("name", "Alice")
            .execute(db)
            .await
            .unwrap();

        let rows = db
            .select("users")
            .where_eq("name", "Alice")
            .execute(db)
            .await
            .unwrap();

        assert_eq!(
            rows,
            vec![Row {
                columns: vec![("id".into(), 1i64.into()), ("name".into(), "Alice".into())]
            }]
        );
    }

    async fn test_update(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db = &**db;

        db.insert("users")
            .value("name", "Bob")
            .execute(db)
            .await
            .unwrap();

        db.update("users")
            .value("name", "Charlie")
            .where_eq("name", "Bob")
            .execute(db)
            .await
            .unwrap();

        let rows = db
            .select("users")
            .where_eq("name", "Charlie")
            .execute(db)
            .await
            .unwrap();

        assert_eq!(
            rows,
            vec![Row {
                columns: vec![
                    ("id".into(), 1i64.into()),
                    ("name".into(), "Charlie".into())
                ]
            }]
        );
    }

    async fn test_delete(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db = &**db;

        db.insert("users")
            .value("name", "Dave")
            .execute(db)
            .await
            .unwrap();

        let deleted = db
            .delete("users")
            .where_eq("name", "Dave")
            .execute(db)
            .await
            .unwrap();

        assert_eq!(
            deleted,
            vec![Row {
                columns: vec![("id".into(), 1i64.into()), ("name".into(), "Dave".into())]
            }]
        );

        let rows = db
            .select("users")
            .where_eq("name", "Dave")
            .execute(db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 0);
    }

    async fn test_delete_with_limit(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db = &**db;

        db.insert("users")
            .value("name", "Dave")
            .execute(db)
            .await
            .unwrap();

        let deleted = db
            .delete("users")
            .where_not_eq("name", "Bob")
            .where_eq("name", "Dave")
            .execute_first(db)
            .await
            .unwrap();

        assert_eq!(
            deleted,
            Some(Row {
                columns: vec![("id".into(), 1i64.into()), ("name".into(), "Dave".into())]
            })
        );

        let rows = db
            .select("users")
            .where_eq("name", "Dave")
            .execute(db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 0);
    }

    async fn test_transaction_commit(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db = &**db;

        let tx = db.begin_transaction().await.unwrap();

        tx.insert("users")
            .value("name", "TransactionUser")
            .execute(&*tx)
            .await
            .unwrap();

        tx.commit().await.unwrap();

        let rows = db
            .select("users")
            .where_eq("name", "TransactionUser")
            .execute(db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].get("name").unwrap().as_str().unwrap(),
            "TransactionUser"
        );
    }

    async fn test_transaction_rollback(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db = &**db;

        let tx = db.begin_transaction().await.unwrap();

        tx.insert("users")
            .value("name", "RollbackUser")
            .execute(&*tx)
            .await
            .unwrap();

        tx.rollback().await.unwrap();

        let rows = db
            .select("users")
            .where_eq("name", "RollbackUser")
            .execute(db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 0);
    }

    async fn test_transaction_isolation(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db = &**db;

        let tx = db.begin_transaction().await.unwrap();

        tx.insert("users")
            .value("name", "IsolatedUser")
            .execute(&*tx)
            .await
            .unwrap();

        let rows = db
            .select("users")
            .where_eq("name", "IsolatedUser")
            .execute(db)
            .await
            .unwrap_or_default();

        assert_eq!(
            rows.len(),
            0,
            "Uncommitted data should not be visible outside transaction"
        );

        tx.commit().await.unwrap();

        let rows = db
            .select("users")
            .where_eq("name", "IsolatedUser")
            .execute(db)
            .await
            .unwrap();

        assert_eq!(rows.len(), 1, "Committed data should be visible");
    }

    async fn test_nested_transaction_rejection(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db = &**db;

        let tx = db.begin_transaction().await.unwrap();

        let nested_result = tx.begin_transaction().await;
        assert!(
            nested_result.is_err(),
            "Nested transactions should be rejected"
        );

        tx.insert("users")
            .value("name", "NestedTestUser")
            .execute(&*tx)
            .await
            .unwrap();

        tx.commit().await.unwrap();
    }

    async fn test_concurrent_transactions(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db_clone = Arc::clone(&db);

        let tx1 = db.begin_transaction().await.unwrap();
        let tx2_result = db_clone.begin_transaction().await;

        if tx2_result.is_err() {
            tx1.insert("users")
                .value("name", "ConcurrentUser1")
                .execute(&*tx1)
                .await
                .unwrap();

            tx1.commit().await.unwrap();

            let rows = db
                .select("users")
                .columns(&["name"])
                .execute(&**db)
                .await
                .unwrap();

            assert_eq!(rows.len(), 1, "Single transaction should succeed");
            return;
        }

        let tx2 = tx2_result.unwrap();

        let result1 = tx1
            .insert("users")
            .value("name", "ConcurrentUser1")
            .execute(&*tx1)
            .await;

        let result2 = tx2
            .insert("users")
            .value("name", "ConcurrentUser2")
            .execute(&*tx2)
            .await;

        let tx1_success = result1.is_ok();
        let tx2_success = result2.is_ok();

        if tx1_success {
            tx1.commit().await.unwrap();
        } else {
            tx1.rollback().await.unwrap();
        }

        if tx2_success {
            tx2.commit().await.unwrap();
        } else {
            tx2.rollback().await.unwrap();
        }

        let rows = db
            .select("users")
            .columns(&["name"])
            .execute(&**db)
            .await
            .unwrap();

        let names: Vec<String> = rows
            .iter()
            .map(|r| r.get("name").unwrap().as_str().unwrap().to_string())
            .collect();

        let has_user1 = names.contains(&"ConcurrentUser1".to_string());
        let has_user2 = names.contains(&"ConcurrentUser2".to_string());

        assert!(
            has_user1 || has_user2,
            "At least one concurrent transaction should succeed"
        );
    }

    async fn test_transaction_crud_operations(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db = &**db;

        db.insert("users")
            .value("name", "InitialUser")
            .execute(db)
            .await
            .unwrap();

        let tx = db.begin_transaction().await.unwrap();

        let insert_result = tx
            .insert("users")
            .value("name", "TxInsertUser")
            .execute(&*tx)
            .await
            .unwrap();

        assert_eq!(
            insert_result.get("name").unwrap().as_str().unwrap(),
            "TxInsertUser"
        );

        let update_result = tx
            .update("users")
            .value("name", "UpdatedInitialUser")
            .where_eq("name", "InitialUser")
            .execute(&*tx)
            .await
            .unwrap();

        assert_eq!(update_result.len(), 1);
        assert_eq!(
            update_result[0].get("name").unwrap().as_str().unwrap(),
            "UpdatedInitialUser"
        );

        let upsert_result = tx
            .upsert("users")
            .value("name", "UpsertUser")
            .where_eq("name", "UpdatedInitialUser")
            .execute(&*tx)
            .await
            .unwrap();

        assert_eq!(upsert_result.len(), 1);
        assert_eq!(
            upsert_result[0].get("name").unwrap().as_str().unwrap(),
            "UpsertUser"
        );

        let delete_result = tx
            .delete("users")
            .where_eq("name", "TxInsertUser")
            .execute(&*tx)
            .await
            .unwrap();

        assert_eq!(delete_result.len(), 1);
        assert_eq!(
            delete_result[0].get("name").unwrap().as_str().unwrap(),
            "TxInsertUser"
        );

        let tx_rows = tx.select("users").execute(&*tx).await.unwrap();

        let tx_names: Vec<String> = tx_rows
            .iter()
            .map(|r| r.get("name").unwrap().as_str().unwrap().to_string())
            .collect();

        assert!(tx_names.contains(&"UpsertUser".to_string()));
        assert!(!tx_names.contains(&"UpdatedInitialUser".to_string()));
        assert!(!tx_names.contains(&"TxInsertUser".to_string()));
        assert!(!tx_names.contains(&"InitialUser".to_string()));

        tx.commit().await.unwrap();

        let final_rows = db.select("users").execute(db).await.unwrap();

        let final_names: Vec<String> = final_rows
            .iter()
            .map(|r| r.get("name").unwrap().as_str().unwrap().to_string())
            .collect();

        assert!(final_names.contains(&"UpsertUser".to_string()));
        assert!(!final_names.contains(&"UpdatedInitialUser".to_string()));
        assert!(!final_names.contains(&"TxInsertUser".to_string()));
        assert!(!final_names.contains(&"InitialUser".to_string()));
    }

    async fn test_query_raw_with_valid_sql(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db = &**db;

        db.insert("users")
            .value("name", "QueryTest")
            .execute(db)
            .await
            .unwrap();

        let rows = db
            .query_raw("SELECT name FROM users WHERE name = 'QueryTest'")
            .await
            .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].columns.len(), 1);
        assert_eq!(rows[0].columns[0].0, "name");
        match &rows[0].columns[0].1 {
            DatabaseValue::String(s) => assert_eq!(s, "QueryTest"),
            DatabaseValue::StringOpt(Some(s)) => assert_eq!(s, "QueryTest"),
            other => panic!("Unexpected value type: {:?}", other),
        }
    }

    async fn test_query_raw_with_empty_result(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db = &**db;

        let rows = db
            .query_raw("SELECT name FROM users WHERE name = 'NonExistent'")
            .await
            .unwrap();

        assert_eq!(rows.len(), 0);
    }

    async fn test_query_raw_with_invalid_sql(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db = &**db;

        let result = db.query_raw("INVALID SQL STATEMENT").await;

        assert!(result.is_err());
        match result.unwrap_err() {
            switchy_database::DatabaseError::QueryFailed(_) => {}
            other => panic!("Expected QueryFailed error, got: {:?}", other),
        }
    }

    async fn test_query_raw_with_ddl_statement(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db = &**db;

        let result = db
            .query_raw(
                "CREATE TABLE test_query_raw (id INTEGER PRIMARY KEY AUTO_INCREMENT, name TEXT)",
            )
            .await;

        if let Ok(rows) = result {
            assert_eq!(
                rows.len(),
                0,
                "DDL statements should return empty result set"
            );
        }
    }

    async fn test_query_raw_in_transaction(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };
        let db = &**db;

        let tx = db.begin_transaction().await.unwrap();

        tx.insert("users")
            .value("name", "TxQueryTest")
            .execute(&*tx)
            .await
            .unwrap();

        let rows = tx
            .query_raw("SELECT name FROM users WHERE name = 'TxQueryTest'")
            .await
            .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].columns[0].0, "name");

        tx.commit().await.unwrap();

        let rows = db
            .query_raw("SELECT name FROM users WHERE name = 'TxQueryTest'")
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }
}
