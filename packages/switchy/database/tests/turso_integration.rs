#![cfg(feature = "schema")]

use std::sync::Arc;

use switchy_database::{Database, DatabaseValue, schema::DataType};

#[cfg(feature = "turso")]
macro_rules! generate_tests {
    () => {
        #[test_log::test(switchy_async::test(no_simulator))]
        async fn test_insert() {
            let db = setup_db().await;
            let db = &**db;

            db.exec_raw_params(
                "INSERT INTO users (name) VALUES (?1)",
                &[DatabaseValue::String("Alice".to_string())]
            )
            .await
            .unwrap();

            let rows = db.query_raw("SELECT id, name FROM users WHERE name = 'Alice'").await.unwrap();

            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].get("id").unwrap().as_i64().unwrap(), 1);
            assert_eq!(rows[0].get("name").unwrap().as_str().unwrap(), "Alice");
        }

        #[test_log::test(switchy_async::test(no_simulator))]
        async fn test_update() {
            let db = setup_db().await;
            let db = &**db;

            db.exec_raw_params(
                "INSERT INTO users (name) VALUES (?1)",
                &[DatabaseValue::String("Bob".to_string())]
            )
            .await
            .unwrap();

            db.exec_raw_params(
                "UPDATE users SET name = ?1 WHERE name = ?2",
                &[
                    DatabaseValue::String("Charlie".to_string()),
                    DatabaseValue::String("Bob".to_string())
                ]
            )
            .await
            .unwrap();

            let rows = db.query_raw("SELECT name FROM users WHERE name = 'Charlie'").await.unwrap();

            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0].get("name").unwrap().as_str().unwrap(),
                "Charlie"
            );
        }

        #[test_log::test(switchy_async::test(no_simulator))]
        async fn test_delete() {
            let db = setup_db().await;
            let db = &**db;

            db.exec_raw_params(
                "INSERT INTO users (name) VALUES (?1)",
                &[DatabaseValue::String("Dave".to_string())]
            )
            .await
            .unwrap();

            db.exec_raw_params(
                "DELETE FROM users WHERE name = ?1",
                &[DatabaseValue::String("Dave".to_string())]
            )
            .await
            .unwrap();

            let rows = db.query_raw("SELECT * FROM users WHERE name = 'Dave'").await.unwrap();

            assert_eq!(rows.len(), 0);
        }

        #[test_log::test(switchy_async::test(no_simulator, real_time))]
        async fn test_transaction_commit() {
            let db = setup_db().await;
            let db = &**db;

            let tx = db.begin_transaction().await.unwrap();

            tx.exec_raw_params(
                "INSERT INTO users (name) VALUES (?1)",
                &[DatabaseValue::String("TransactionUser".to_string())]
            )
            .await
            .unwrap();

            tx.commit().await.unwrap();

            let rows = db.query_raw("SELECT * FROM users WHERE name = 'TransactionUser'").await.unwrap();

            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0].get("name").unwrap().as_str().unwrap(),
                "TransactionUser"
            );
        }

        #[test_log::test(switchy_async::test(no_simulator, real_time))]
        async fn test_transaction_rollback() {
            let db = setup_db().await;
            let db = &**db;

            let tx = db.begin_transaction().await.unwrap();

            tx.exec_raw_params(
                "INSERT INTO users (name) VALUES (?1)",
                &[DatabaseValue::String("RollbackUser".to_string())]
            )
            .await
            .unwrap();

            tx.rollback().await.unwrap();

            let rows = db.query_raw("SELECT * FROM users WHERE name = 'RollbackUser'").await.unwrap();

            assert_eq!(rows.len(), 0);
        }

        #[test_log::test(switchy_async::test(no_simulator))]
        async fn test_table_exists() {
            let db = setup_db().await;
            let db = &**db;

            let exists = db.table_exists("users").await.unwrap();
            assert!(exists, "users table should exist");

            let not_exists = db.table_exists("nonexistent_table").await.unwrap();
            assert!(!not_exists, "nonexistent_table should not exist");
        }

        #[test_log::test(switchy_async::test(no_simulator))]
        async fn test_get_table_columns() {
            let db = setup_db().await;
            let db = &**db;

            let columns = db.get_table_columns("users").await.unwrap();

            assert_eq!(columns.len(), 2);
            assert_eq!(columns[0].name, "id");
            assert_eq!(columns[0].data_type, DataType::BigInt);
            assert_eq!(columns[1].name, "name");
            assert_eq!(columns[1].data_type, DataType::Text);
        }

        #[test_log::test(switchy_async::test(no_simulator))]
        async fn test_complex_queries() {
            let db = setup_db().await;
            let db = &**db;

            db.exec_raw("CREATE TABLE IF NOT EXISTS products (id INTEGER PRIMARY KEY, name TEXT, price REAL, stock INTEGER)")
                .await
                .unwrap();

            db.exec_raw_params(
                "INSERT INTO products (name, price, stock) VALUES (?1, ?2, ?3)",
                &[
                    DatabaseValue::String("Widget".to_string()),
                    DatabaseValue::Real64(19.99),
                    DatabaseValue::Int64(100)
                ]
            )
            .await
            .unwrap();

            db.exec_raw_params(
                "INSERT INTO products (name, price, stock) VALUES (?1, ?2, ?3)",
                &[
                    DatabaseValue::String("Gadget".to_string()),
                    DatabaseValue::Real64(29.99),
                    DatabaseValue::Int64(50)
                ]
            )
            .await
            .unwrap();

            let rows = db.query_raw("SELECT * FROM products WHERE price > 15.0").await.unwrap();

            assert_eq!(rows.len(), 2);
        }

        #[test_log::test(switchy_async::test(no_simulator))]
        async fn test_parameterized_query() {
            let db = setup_db().await;
            let db = &**db;

            db.exec_raw_params(
                "INSERT INTO users (name) VALUES (?1)",
                &[DatabaseValue::String("Alice".to_string())]
            )
            .await
            .unwrap();

            db.exec_raw_params(
                "INSERT INTO users (name) VALUES (?1)",
                &[DatabaseValue::String("Bob".to_string())]
            )
            .await
            .unwrap();

            let rows = db.query_raw_params(
                "SELECT * FROM users WHERE name = ?1",
                &[DatabaseValue::String("Alice".to_string())]
            ).await.unwrap();

            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].get("name").unwrap().as_str().unwrap(), "Alice");
        }
    };
}

#[cfg(feature = "turso")]
mod turso {
    use pretty_assertions::assert_eq;

    use super::*;

    async fn setup_db() -> Arc<Box<dyn Database>> {
        let db = switchy_database_connection::init_turso_local(None)
            .await
            .unwrap();
        let db = Arc::new(db);

        db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .await
            .unwrap();
        db
    }

    generate_tests!();
}
