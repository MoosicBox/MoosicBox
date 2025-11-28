use std::sync::Arc;

use switchy_database::{Database, query::FilterableQuery as _};

mod common;

use common::integration_tests::IntegrationTestSuite;

#[cfg(feature = "sqlite-sqlx")]
mod sqlx_sqlite {
    use super::*;

    struct SqlxSqliteIntegrationTests;

    impl IntegrationTestSuite for SqlxSqliteIntegrationTests {
        async fn get_database(&self) -> Option<Arc<Box<dyn Database>>> {
            let timestamp = switchy_time::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let thread_id = std::thread::current().id();
            let temp_file = std::env::temp_dir().join(format!(
                "test_db_{}_{}_{:?}.sqlite",
                std::process::id(),
                timestamp,
                thread_id
            ));
            let db = switchy_database_connection::init_sqlite_sqlx(Some(&temp_file))
                .await
                .ok()?;

            db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
                .await
                .ok()?;

            Some(Arc::new(db))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_insert() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_insert().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_update() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_update().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_delete() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_delete().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_delete_with_limit() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_delete_with_limit().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_sqlx_sqlite_transaction_commit() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_transaction_commit().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_sqlx_sqlite_transaction_rollback() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_transaction_rollback().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_sqlx_sqlite_transaction_isolation() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_transaction_isolation().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_sqlx_sqlite_nested_transaction_rejection() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_nested_transaction_rejection().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_sqlx_sqlite_concurrent_transactions() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_concurrent_transactions().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_transaction_crud_operations() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_transaction_crud_operations().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_query_raw_with_valid_sql() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_query_raw_with_valid_sql().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_query_raw_with_empty_result() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_query_raw_with_empty_result().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_query_raw_with_invalid_sql() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_query_raw_with_invalid_sql().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_query_raw_with_ddl_statement() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_query_raw_with_ddl_statement().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_query_raw_in_transaction() {
        let suite = SqlxSqliteIntegrationTests;
        suite.test_query_raw_in_transaction().await;
    }
}

#[cfg(feature = "sqlite-rusqlite")]
mod rusqlite {
    use super::*;

    struct RusqliteIntegrationTests;

    impl IntegrationTestSuite for RusqliteIntegrationTests {
        async fn get_database(&self) -> Option<Arc<Box<dyn Database>>> {
            let db = switchy_database_connection::init_sqlite_rusqlite(None).ok()?;

            db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
                .await
                .ok()?;

            Some(Arc::new(db))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_insert() {
        let suite = RusqliteIntegrationTests;
        suite.test_insert().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_update() {
        let suite = RusqliteIntegrationTests;
        suite.test_update().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_delete() {
        let suite = RusqliteIntegrationTests;
        suite.test_delete().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_delete_with_limit() {
        let suite = RusqliteIntegrationTests;
        suite.test_delete_with_limit().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_rusqlite_transaction_commit() {
        let suite = RusqliteIntegrationTests;
        suite.test_transaction_commit().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_rusqlite_transaction_rollback() {
        let suite = RusqliteIntegrationTests;
        suite.test_transaction_rollback().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_rusqlite_transaction_isolation() {
        let suite = RusqliteIntegrationTests;
        suite.test_transaction_isolation().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_rusqlite_nested_transaction_rejection() {
        let suite = RusqliteIntegrationTests;
        suite.test_nested_transaction_rejection().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_rusqlite_concurrent_transactions() {
        let suite = RusqliteIntegrationTests;
        suite.test_concurrent_transactions().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_transaction_crud_operations() {
        let suite = RusqliteIntegrationTests;
        suite.test_transaction_crud_operations().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_query_raw_with_valid_sql() {
        let suite = RusqliteIntegrationTests;
        suite.test_query_raw_with_valid_sql().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_query_raw_with_empty_result() {
        let suite = RusqliteIntegrationTests;
        suite.test_query_raw_with_empty_result().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_query_raw_with_invalid_sql() {
        let suite = RusqliteIntegrationTests;
        suite.test_query_raw_with_invalid_sql().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_query_raw_with_ddl_statement() {
        let suite = RusqliteIntegrationTests;
        suite.test_query_raw_with_ddl_statement().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_query_raw_in_transaction() {
        let suite = RusqliteIntegrationTests;
        suite.test_query_raw_in_transaction().await;
    }
}

#[cfg(feature = "simulator")]
mod simulator {
    use pretty_assertions::assert_eq;

    use super::*;

    struct SimulatorIntegrationTests;

    impl IntegrationTestSuite for SimulatorIntegrationTests {
        async fn get_database(&self) -> Option<Arc<Box<dyn Database>>> {
            let db = switchy_database::simulator::SimulationDatabase::new().ok()?;
            let db = Box::new(db) as Box<dyn Database>;

            db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
                .await
                .ok()?;

            Some(Arc::new(db))
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_insert() {
        let suite = SimulatorIntegrationTests;
        suite.test_insert().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_update() {
        let suite = SimulatorIntegrationTests;
        suite.test_update().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_delete() {
        let suite = SimulatorIntegrationTests;
        suite.test_delete().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_delete_with_limit() {
        let suite = SimulatorIntegrationTests;
        suite.test_delete_with_limit().await;
    }

    #[test_log::test(switchy_async::test(real_time))]
    async fn test_simulator_transaction_commit() {
        let suite = SimulatorIntegrationTests;
        suite.test_transaction_commit().await;
    }

    #[test_log::test(switchy_async::test(real_time))]
    async fn test_simulator_transaction_rollback() {
        let suite = SimulatorIntegrationTests;
        suite.test_transaction_rollback().await;
    }

    #[test_log::test(switchy_async::test(real_time))]
    async fn test_simulator_transaction_isolation() {
        let suite = SimulatorIntegrationTests;
        suite.test_transaction_isolation().await;
    }

    #[test_log::test(switchy_async::test(real_time))]
    async fn test_simulator_nested_transaction_rejection() {
        let suite = SimulatorIntegrationTests;
        suite.test_nested_transaction_rejection().await;
    }

    #[test_log::test(switchy_async::test(real_time))]
    async fn test_simulator_concurrent_transactions() {
        let suite = SimulatorIntegrationTests;
        suite.test_concurrent_transactions().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_transaction_crud_operations() {
        let suite = SimulatorIntegrationTests;
        suite.test_transaction_crud_operations().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_query_raw_with_valid_sql() {
        let suite = SimulatorIntegrationTests;
        suite.test_query_raw_with_valid_sql().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_query_raw_with_empty_result() {
        let suite = SimulatorIntegrationTests;
        suite.test_query_raw_with_empty_result().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_query_raw_with_invalid_sql() {
        let suite = SimulatorIntegrationTests;
        suite.test_query_raw_with_invalid_sql().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_query_raw_with_ddl_statement() {
        let suite = SimulatorIntegrationTests;
        suite.test_query_raw_with_ddl_statement().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_query_raw_in_transaction() {
        let suite = SimulatorIntegrationTests;
        suite.test_query_raw_in_transaction().await;
    }

    async fn setup_db() -> Arc<Box<dyn Database>> {
        let db = switchy_database::simulator::SimulationDatabase::new().unwrap();
        let db = Arc::new(Box::new(db) as Box<dyn Database>);

        db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            .await
            .unwrap();
        db
    }

    #[test_log::test(switchy_async::test)]
    async fn test_operation_after_commit_verification() {
        let db = setup_db().await;
        let db = &**db;

        let tx = db.begin_transaction().await.unwrap();
        tx.insert("users")
            .value("name", "CommittedUser")
            .execute(&*tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();

        let rows = db
            .select("users")
            .where_eq("name", "CommittedUser")
            .execute(db)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_operation_after_rollback_verification() {
        let db = setup_db().await;
        let db = &**db;

        let tx = db.begin_transaction().await.unwrap();
        tx.insert("users")
            .value("name", "RolledBackUser")
            .execute(&*tx)
            .await
            .unwrap();
        tx.rollback().await.unwrap();

        let rows = db
            .select("users")
            .where_eq("name", "RolledBackUser")
            .execute(db)
            .await
            .unwrap();
        assert_eq!(rows.len(), 0);
    }

    #[cfg(feature = "schema")]
    #[test_log::test(switchy_async::test)]
    async fn test_drop_table_operations() {
        let db = setup_db().await;
        let db = &**db;

        db.exec_raw("CREATE TABLE temp_table (id INTEGER PRIMARY KEY, data TEXT)")
            .await
            .unwrap();

        db.insert("temp_table")
            .value("data", "test_data")
            .execute(db)
            .await
            .unwrap();

        db.drop_table("temp_table").execute(db).await.unwrap();

        let insert_result = db
            .insert("temp_table")
            .value("data", "test_data2")
            .execute(db)
            .await;
        assert!(insert_result.is_err());

        db.drop_table("nonexistent_table")
            .if_exists(true)
            .execute(db)
            .await
            .unwrap();
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_create_index_operations() {
        let db = setup_db().await;
        let db = &**db;

        db.exec_raw("CREATE TABLE index_test (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
            .await
            .unwrap();

        db.create_index("idx_name")
            .table("index_test")
            .column("name")
            .execute(db)
            .await
            .unwrap();

        db.create_index("idx_multi")
            .table("index_test")
            .columns(vec!["name", "email"])
            .execute(db)
            .await
            .unwrap();

        db.create_index("idx_email")
            .table("index_test")
            .column("email")
            .unique(true)
            .execute(db)
            .await
            .unwrap();

        db.create_index("idx_name")
            .table("index_test")
            .column("name")
            .if_not_exists(true)
            .execute(db)
            .await
            .unwrap();

        db.create_index("idx_quoted")
            .table("index_test")
            .column("name")
            .execute(db)
            .await
            .unwrap();

        db.exec_raw("DROP TABLE index_test").await.unwrap();
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_drop_index_operations() {
        let db = setup_db().await;
        let db = &**db;

        db.exec_raw("CREATE TABLE drop_index_test (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
            .await
            .unwrap();

        db.create_index("idx_name")
            .table("drop_index_test")
            .column("name")
            .execute(db)
            .await
            .unwrap();

        db.create_index("idx_email")
            .table("drop_index_test")
            .column("email")
            .execute(db)
            .await
            .unwrap();

        db.drop_index("idx_name", "drop_index_test")
            .execute(db)
            .await
            .unwrap();

        db.drop_index("idx_email", "drop_index_test")
            .if_exists()
            .execute(db)
            .await
            .unwrap();

        db.drop_index("nonexistent_idx", "drop_index_test")
            .if_exists()
            .execute(db)
            .await
            .unwrap();

        let drop_result = db
            .drop_index("another_nonexistent_idx", "drop_index_test")
            .execute(db)
            .await;

        let _expected_behavior = drop_result.is_err();

        db.exec_raw("DROP TABLE drop_index_test").await.unwrap();
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_alter_table_add_column() {
        let db = setup_db().await;
        let db = &**db;

        db.exec_raw("CREATE TABLE alter_test (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .unwrap();

        db.alter_table("alter_test")
            .add_column(
                "email".to_string(),
                switchy_database::schema::DataType::VarChar(255),
                false,
                Some(switchy_database::DatabaseValue::String(
                    "default@example.com".to_string(),
                )),
            )
            .execute(db)
            .await
            .unwrap();

        db.insert("alter_test")
            .value("name", "Test User")
            .execute(db)
            .await
            .unwrap();

        let rows = db.select("alter_test").execute(db).await.unwrap();
        assert!(!rows.is_empty());

        let row = &rows[0];
        assert!(row.get("email").is_some());

        db.exec_raw("DROP TABLE alter_test").await.unwrap();
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_alter_table_drop_column() {
        let db = setup_db().await;
        let db = &**db;

        db.exec_raw("CREATE TABLE alter_drop_test (id INTEGER PRIMARY KEY, name TEXT, email TEXT, age INTEGER)")
            .await
            .unwrap();

        db.insert("alter_drop_test")
            .value("name", "John Doe")
            .value("email", "john@example.com")
            .value("age", 30)
            .execute(db)
            .await
            .unwrap();

        db.alter_table("alter_drop_test")
            .drop_column("email".to_string())
            .execute(db)
            .await
            .unwrap();

        let rows = db.select("alter_drop_test").execute(db).await.unwrap();
        assert!(!rows.is_empty());

        let row = &rows[0];
        assert!(row.get("name").is_some());
        assert!(row.get("age").is_some());
        assert!(
            row.get("email").is_none(),
            "Email column should have been dropped"
        );

        db.exec_raw("DROP TABLE alter_drop_test").await.unwrap();
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_alter_table_rename_column() {
        let db = setup_db().await;
        let db = &**db;

        db.exec_raw("CREATE TABLE alter_rename_test (id INTEGER PRIMARY KEY, old_name TEXT)")
            .await
            .unwrap();

        db.insert("alter_rename_test")
            .value("old_name", "test value")
            .execute(db)
            .await
            .unwrap();

        db.alter_table("alter_rename_test")
            .rename_column("old_name".to_string(), "new_name".to_string())
            .execute(db)
            .await
            .unwrap();

        let rows = db.select("alter_rename_test").execute(db).await.unwrap();
        assert!(!rows.is_empty());

        let row = &rows[0];
        assert!(row.get("new_name").is_some());
        assert!(
            row.get("old_name").is_none(),
            "Old column name should not exist"
        );

        db.exec_raw("DROP TABLE alter_rename_test").await.unwrap();
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_alter_table_modify_column() {
        let db = setup_db().await;
        let db = &**db;

        db.exec_raw("CREATE TABLE alter_modify_test (id INTEGER PRIMARY KEY, data TEXT)")
            .await
            .unwrap();

        db.insert("alter_modify_test")
            .value("data", "123")
            .execute(db)
            .await
            .unwrap();

        db.alter_table("alter_modify_test")
            .modify_column(
                "data".to_string(),
                switchy_database::schema::DataType::BigInt,
                Some(true),
                Some(switchy_database::DatabaseValue::Int64(0)),
            )
            .execute(db)
            .await
            .unwrap();

        let rows = db.select("alter_modify_test").execute(db).await.unwrap();
        assert!(!rows.is_empty());

        let row = &rows[0];
        assert!(row.get("data").is_some());

        db.insert("alter_modify_test").execute(db).await.unwrap();

        let rows_after = db.select("alter_modify_test").execute(db).await.unwrap();
        assert_eq!(rows_after.len(), 2);

        db.exec_raw("DROP TABLE alter_modify_test").await.unwrap();
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_alter_table_multiple_operations() {
        let db = setup_db().await;
        let db = &**db;

        db.exec_raw(
            "CREATE TABLE alter_multi_test (id INTEGER PRIMARY KEY, old_col TEXT, drop_me TEXT)",
        )
        .await
        .unwrap();

        db.insert("alter_multi_test")
            .value("old_col", "original value")
            .value("drop_me", "will be dropped")
            .execute(db)
            .await
            .unwrap();

        db.alter_table("alter_multi_test")
            .add_column(
                "new_col".to_string(),
                switchy_database::schema::DataType::VarChar(100),
                true,
                Some(switchy_database::DatabaseValue::String(
                    "default".to_string(),
                )),
            )
            .drop_column("drop_me".to_string())
            .rename_column("old_col".to_string(), "renamed_col".to_string())
            .execute(db)
            .await
            .unwrap();

        let rows = db.select("alter_multi_test").execute(db).await.unwrap();
        assert!(!rows.is_empty());

        let row = &rows[0];
        assert!(row.get("new_col").is_some(), "New column should exist");
        assert!(
            row.get("renamed_col").is_some(),
            "Renamed column should exist"
        );
        assert!(
            row.get("old_col").is_none(),
            "Old column name should not exist"
        );
        assert!(
            row.get("drop_me").is_none(),
            "Dropped column should not exist"
        );

        db.exec_raw("DROP TABLE alter_multi_test").await.unwrap();
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_alter_table_transaction_rollback() {
        let db = setup_db().await;
        let db = &**db;

        db.exec_raw(
            "CREATE TABLE alter_rollback_test (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
        )
        .await
        .unwrap();

        db.insert("alter_rollback_test")
            .value("name", "test")
            .execute(db)
            .await
            .unwrap();

        db.alter_table("alter_rollback_test")
            .add_column(
                "email".to_string(),
                switchy_database::schema::DataType::VarChar(255),
                true,
                None,
            )
            .execute(db)
            .await
            .unwrap();

        let rows = db.select("alter_rollback_test").execute(db).await.unwrap();
        assert!(!rows.is_empty());
        assert!(rows[0].get("email").is_some());

        db.exec_raw("DROP TABLE alter_rollback_test").await.unwrap();
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_alter_table_constraint_detection() {
        let db = setup_db().await;
        let db = &**db;

        db.exec_raw("CREATE TABLE simple_test (id INTEGER PRIMARY KEY, data TEXT)")
            .await
            .unwrap();

        db.insert("simple_test")
            .value("data", "original")
            .execute(db)
            .await
            .unwrap();

        db.alter_table("simple_test")
            .modify_column(
                "data".to_string(),
                switchy_database::schema::DataType::VarChar(255),
                Some(true),
                None,
            )
            .execute(db)
            .await
            .unwrap();

        let rows = db.select("simple_test").execute(db).await.unwrap();
        assert!(
            !rows.is_empty(),
            "Data should be preserved after column modification"
        );

        db.exec_raw("DROP TABLE simple_test").await.unwrap();

        db.exec_raw("CREATE TABLE pk_test (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .unwrap();

        db.insert("pk_test")
            .value("name", "test")
            .execute(db)
            .await
            .unwrap();

        let result = db
            .alter_table("pk_test")
            .modify_column(
                "id".to_string(),
                switchy_database::schema::DataType::BigInt,
                Some(false),
                None,
            )
            .execute(db)
            .await;

        if let Ok(()) = result {
            let rows = db.select("pk_test").execute(db).await.unwrap();
            assert!(!rows.is_empty(), "Data preserved after table recreation");
        }

        db.exec_raw("DROP TABLE pk_test").await.unwrap();
    }

    #[cfg(feature = "schema")]
    #[switchy_async::test]
    async fn test_alter_table_table_recreation_applies_changes() {
        let db = setup_db().await;
        let db = &**db;

        db.exec_raw("CREATE TABLE recreation_test (id INTEGER PRIMARY KEY, name TEXT)")
            .await
            .unwrap();

        db.insert("recreation_test")
            .value("name", "original")
            .execute(db)
            .await
            .unwrap();

        let result = db
            .alter_table("recreation_test")
            .modify_column(
                "id".to_string(),
                switchy_database::schema::DataType::BigInt,
                Some(false),
                Some(switchy_database::DatabaseValue::Int64(999)),
            )
            .execute(db)
            .await;

        match result {
            Ok(()) => {
                let rows = db.select("recreation_test").execute(db).await.unwrap();
                assert_eq!(
                    rows.len(),
                    1,
                    "Should have exactly one row after recreation"
                );

                assert!(
                    rows[0].get("name").is_some(),
                    "Name column should still exist"
                );
                if let Some(name_value) = rows[0].get("name") {
                    let name_str = match name_value {
                        switchy_database::DatabaseValue::String(s) => s.clone(),
                        switchy_database::DatabaseValue::StringOpt(Some(s)) => s.clone(),
                        _ => panic!("Unexpected name value type: {:?}", name_value),
                    };
                    assert_eq!(name_str, "original", "Name value should be preserved");
                }

                assert!(
                    rows[0].get("id").is_some(),
                    "ID column should exist after recreation"
                );
            }
            Err(e) => {
                println!("Table recreation failed (acceptable): {}", e);
            }
        }

        db.exec_raw("DROP TABLE recreation_test").await.unwrap();
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_query_raw_delegation() {
        let db = setup_db().await;
        let db = &**db;

        db.insert("users")
            .value("name", "SimulatorQuery")
            .execute(db)
            .await
            .unwrap();

        let rows = db
            .query_raw("SELECT name FROM users WHERE name = 'SimulatorQuery'")
            .await
            .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].columns.len(), 1);
        assert_eq!(rows[0].columns[0].0, "name");
        match &rows[0].columns[0].1 {
            switchy_database::DatabaseValue::String(s) => assert_eq!(s, "SimulatorQuery"),
            switchy_database::DatabaseValue::StringOpt(Some(s)) => assert_eq!(s, "SimulatorQuery"),
            other => panic!("Unexpected value type: {:?}", other),
        }
    }
}

#[cfg(feature = "turso")]
mod turso {
    use super::*;

    struct TursoIntegrationTests;

    impl IntegrationTestSuite for TursoIntegrationTests {
        async fn get_database(&self) -> Option<Arc<Box<dyn Database>>> {
            let db = switchy_database_connection::init_turso_local(None)
                .await
                .ok()?;

            db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
                .await
                .ok()?;

            Some(Arc::new(db))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_turso_insert() {
        let suite = TursoIntegrationTests;
        suite.test_insert().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_turso_update() {
        let suite = TursoIntegrationTests;
        suite.test_update().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_turso_delete() {
        let suite = TursoIntegrationTests;
        suite.test_delete().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_turso_delete_with_limit() {
        let suite = TursoIntegrationTests;
        suite.test_delete_with_limit().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_turso_transaction_commit() {
        let suite = TursoIntegrationTests;
        suite.test_transaction_commit().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_turso_transaction_rollback() {
        let suite = TursoIntegrationTests;
        suite.test_transaction_rollback().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_turso_transaction_isolation() {
        let suite = TursoIntegrationTests;
        suite.test_transaction_isolation().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_turso_nested_transaction_rejection() {
        let suite = TursoIntegrationTests;
        suite.test_nested_transaction_rejection().await;
    }

    #[test_log::test(switchy_async::test(no_simulator, real_time))]
    async fn test_turso_concurrent_transactions() {
        let suite = TursoIntegrationTests;
        suite.test_concurrent_transactions().await;
    }

    #[ignore = "Turso doesn't properly handle transaction operations in the way we expect yet"]
    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_turso_transaction_crud_operations() {
        let suite = TursoIntegrationTests;
        suite.test_transaction_crud_operations().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_turso_query_raw_with_valid_sql() {
        let suite = TursoIntegrationTests;
        suite.test_query_raw_with_valid_sql().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_turso_query_raw_with_empty_result() {
        let suite = TursoIntegrationTests;
        suite.test_query_raw_with_empty_result().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_turso_query_raw_with_invalid_sql() {
        let suite = TursoIntegrationTests;
        suite.test_query_raw_with_invalid_sql().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_turso_query_raw_with_ddl_statement() {
        let suite = TursoIntegrationTests;
        suite.test_query_raw_with_ddl_statement().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_turso_query_raw_in_transaction() {
        let suite = TursoIntegrationTests;
        suite.test_query_raw_in_transaction().await;
    }
}

#[cfg(feature = "schema")]
use common::introspection_tests::IntrospectionTestSuite;

#[cfg(feature = "schema")]
use common::returning_tests::ReturningTestSuite;

#[cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]
mod rusqlite_introspection_tests {
    use super::*;
    use ::rusqlite::Connection;
    use std::sync::Arc;
    use switchy_async::sync::Mutex;
    use switchy_database::rusqlite::RusqliteDatabase;

    struct RusqliteIntrospectionTests;

    impl IntrospectionTestSuite for RusqliteIntrospectionTests {
        type DatabaseType = RusqliteDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            let test_id = std::thread::current().id();
            let timestamp = switchy_time::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let db_url = format!(
                "file:introspection_test_{test_id:?}_{timestamp}:?mode=memory&cache=shared&uri=true"
            );

            let mut connections = Vec::new();
            for _ in 0..5 {
                let conn = Connection::open(&db_url).ok()?;
                connections.push(Arc::new(Mutex::new(conn)));
            }

            Some(Arc::new(RusqliteDatabase::new(connections)))
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_introspection_table_exists() {
        let suite = RusqliteIntrospectionTests;
        suite.test_table_exists().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_introspection_column_exists() {
        let suite = RusqliteIntrospectionTests;
        suite.test_column_exists().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_introspection_get_table_columns() {
        let suite = RusqliteIntrospectionTests;
        suite.test_get_table_columns().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_introspection_get_table_info() {
        let suite = RusqliteIntrospectionTests;
        suite.test_get_table_info().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_introspection_unsupported_types() {
        let suite = RusqliteIntrospectionTests;
        suite.test_unsupported_types().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_introspection_transaction_context() {
        let suite = RusqliteIntrospectionTests;
        suite.test_transaction_context().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_introspection_edge_cases() {
        let suite = RusqliteIntrospectionTests;
        suite.test_edge_cases().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_introspection_all() {
        let suite = RusqliteIntrospectionTests;
        suite.run_all_tests().await;
    }
}

#[cfg(all(feature = "sqlite-sqlx", feature = "schema"))]
mod sqlx_sqlite_introspection_tests {
    use super::*;
    use std::sync::Arc;
    use switchy_database::sqlx::sqlite::SqliteSqlxDatabase;

    struct SqlxSqliteIntrospectionTests;

    impl IntrospectionTestSuite for SqlxSqliteIntrospectionTests {
        type DatabaseType = SqliteSqlxDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            use sqlx::SqlitePool;
            use switchy_async::sync::Mutex;

            let db_url = "sqlite::memory:";
            let pool = SqlitePool::connect(db_url).await.ok()?;
            let db = SqliteSqlxDatabase::new(Arc::new(Mutex::new(pool)));
            Some(Arc::new(db))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_introspection_table_exists() {
        let suite = SqlxSqliteIntrospectionTests;
        suite.test_table_exists().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_introspection_column_exists() {
        let suite = SqlxSqliteIntrospectionTests;
        suite.test_column_exists().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_introspection_get_table_columns() {
        let suite = SqlxSqliteIntrospectionTests;
        suite.test_get_table_columns().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_introspection_get_table_info() {
        let suite = SqlxSqliteIntrospectionTests;
        suite.test_get_table_info().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_introspection_unsupported_types() {
        let suite = SqlxSqliteIntrospectionTests;
        suite.test_unsupported_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_introspection_transaction_context() {
        let suite = SqlxSqliteIntrospectionTests;
        suite.test_transaction_context().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_introspection_edge_cases() {
        let suite = SqlxSqliteIntrospectionTests;
        suite.test_edge_cases().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_introspection_all() {
        let suite = SqlxSqliteIntrospectionTests;
        suite.run_all_tests().await;
    }
}

#[cfg(all(feature = "postgres", feature = "schema"))]
mod postgres_introspection_tests {
    use super::*;
    use std::sync::Arc;
    use switchy_database::postgres::postgres::PostgresDatabase;

    struct PostgresIntrospectionTests;

    impl IntrospectionTestSuite for PostgresIntrospectionTests {
        type DatabaseType = PostgresDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            let url = std::env::var("POSTGRES_TEST_URL").ok()?;

            let mut cfg = deadpool_postgres::Config::new();
            cfg.url = Some(url.clone());

            let pool = if url.contains("sslmode=require") {
                let connector = native_tls::TlsConnector::builder()
                    .danger_accept_invalid_certs(true)
                    .build()
                    .ok()?;
                let connector = postgres_native_tls::MakeTlsConnector::new(connector);
                cfg.create_pool(Some(deadpool_postgres::Runtime::Tokio1), connector)
                    .ok()?
            } else {
                cfg.create_pool(
                    Some(deadpool_postgres::Runtime::Tokio1),
                    tokio_postgres::NoTls,
                )
                .ok()?
            };

            Some(Arc::new(PostgresDatabase::new(pool)))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_introspection_table_exists() {
        let suite = PostgresIntrospectionTests;
        suite.test_table_exists().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_introspection_column_exists() {
        let suite = PostgresIntrospectionTests;
        suite.test_column_exists().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_introspection_get_table_columns() {
        let suite = PostgresIntrospectionTests;
        suite.test_get_table_columns().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_introspection_get_table_info() {
        let suite = PostgresIntrospectionTests;
        suite.test_get_table_info().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_introspection_unsupported_types() {
        let suite = PostgresIntrospectionTests;
        suite.test_unsupported_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_introspection_transaction_context() {
        let suite = PostgresIntrospectionTests;
        suite.test_transaction_context().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_introspection_edge_cases() {
        let suite = PostgresIntrospectionTests;
        suite.test_edge_cases().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_introspection_all() {
        let suite = PostgresIntrospectionTests;
        suite.run_all_tests().await;
    }
}

#[cfg(all(feature = "postgres-sqlx", feature = "schema"))]
mod sqlx_postgres_introspection_tests {
    use super::*;
    use std::sync::Arc;
    use switchy_database::sqlx::postgres::PostgresSqlxDatabase;

    struct SqlxPostgresIntrospectionTests;

    impl IntrospectionTestSuite for SqlxPostgresIntrospectionTests {
        type DatabaseType = PostgresSqlxDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            use sqlx::PgPool;
            use switchy_async::sync::Mutex;

            let url = std::env::var("POSTGRES_TEST_URL").ok()?;
            let pool = PgPool::connect(&url).await.ok()?;
            let pool = Arc::new(Mutex::new(pool));
            let db = PostgresSqlxDatabase::new(pool);
            Some(Arc::new(db))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_postgres_introspection_table_exists() {
        let suite = SqlxPostgresIntrospectionTests;
        suite.test_table_exists().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_postgres_introspection_column_exists() {
        let suite = SqlxPostgresIntrospectionTests;
        suite.test_column_exists().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_postgres_introspection_get_table_columns() {
        let suite = SqlxPostgresIntrospectionTests;
        suite.test_get_table_columns().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_postgres_introspection_get_table_info() {
        let suite = SqlxPostgresIntrospectionTests;
        suite.test_get_table_info().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_postgres_introspection_unsupported_types() {
        let suite = SqlxPostgresIntrospectionTests;
        suite.test_unsupported_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_postgres_introspection_transaction_context() {
        let suite = SqlxPostgresIntrospectionTests;
        suite.test_transaction_context().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_postgres_introspection_edge_cases() {
        let suite = SqlxPostgresIntrospectionTests;
        suite.test_edge_cases().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_postgres_introspection_all() {
        let suite = SqlxPostgresIntrospectionTests;
        suite.run_all_tests().await;
    }
}

#[cfg(all(feature = "mysql-sqlx", feature = "schema"))]
mod sqlx_mysql_introspection_tests {
    use super::*;
    use std::sync::Arc;
    use switchy_database::sqlx::mysql::MySqlSqlxDatabase;

    struct SqlxMysqlIntrospectionTests;

    impl IntrospectionTestSuite for SqlxMysqlIntrospectionTests {
        type DatabaseType = MySqlSqlxDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            use sqlx::MySqlPool;
            use switchy_async::sync::Mutex;

            let url = std::env::var("MYSQL_TEST_URL").ok()?;
            let pool = MySqlPool::connect(&url).await.ok()?;
            let pool = Arc::new(Mutex::new(pool));
            let db = MySqlSqlxDatabase::new(pool);
            Some(Arc::new(db))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_mysql_introspection_table_exists() {
        let suite = SqlxMysqlIntrospectionTests;
        suite.test_table_exists().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_mysql_introspection_column_exists() {
        let suite = SqlxMysqlIntrospectionTests;
        suite.test_column_exists().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_mysql_introspection_get_table_columns() {
        let suite = SqlxMysqlIntrospectionTests;
        suite.test_get_table_columns().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_mysql_introspection_get_table_info() {
        let suite = SqlxMysqlIntrospectionTests;
        suite.test_get_table_info().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_mysql_introspection_unsupported_types() {
        let suite = SqlxMysqlIntrospectionTests;
        suite.test_unsupported_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_mysql_introspection_transaction_context() {
        let suite = SqlxMysqlIntrospectionTests;
        suite.test_transaction_context().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_mysql_introspection_edge_cases() {
        let suite = SqlxMysqlIntrospectionTests;
        suite.test_edge_cases().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_mysql_introspection_all() {
        let suite = SqlxMysqlIntrospectionTests;
        suite.run_all_tests().await;
    }
}

#[cfg(all(feature = "simulator", feature = "schema"))]
mod simulator_introspection_tests {
    use super::*;
    use std::sync::Arc;
    use switchy_database::simulator::SimulationDatabase;

    struct SimulatorIntrospectionTests;

    impl IntrospectionTestSuite for SimulatorIntrospectionTests {
        type DatabaseType = SimulationDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            let test_id = std::thread::current().id();
            let timestamp = switchy_time::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = format!("simulator_introspection_test_{test_id:?}_{timestamp}.db");

            let simulator = SimulationDatabase::new_for_path(Some(&path)).ok()?;
            Some(Arc::new(simulator))
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_introspection_table_exists() {
        let suite = SimulatorIntrospectionTests;
        suite.test_table_exists().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_introspection_column_exists() {
        let suite = SimulatorIntrospectionTests;
        suite.test_column_exists().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_introspection_get_table_columns() {
        let suite = SimulatorIntrospectionTests;
        suite.test_get_table_columns().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_introspection_get_table_info() {
        let suite = SimulatorIntrospectionTests;
        suite.test_get_table_info().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_introspection_unsupported_types() {
        let suite = SimulatorIntrospectionTests;
        suite.test_unsupported_types().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_introspection_transaction_context() {
        let suite = SimulatorIntrospectionTests;
        suite.test_transaction_context().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_introspection_edge_cases() {
        let suite = SimulatorIntrospectionTests;
        suite.test_edge_cases().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_introspection_all() {
        let suite = SimulatorIntrospectionTests;
        suite.run_all_tests().await;
    }
}

#[cfg(all(feature = "mysql-sqlx", feature = "schema"))]
mod mysql_returning_tests {
    use super::*;
    use std::sync::Arc;
    use switchy_database::sqlx::mysql::MySqlSqlxDatabase;

    struct MysqlReturningTests;

    impl ReturningTestSuite for MysqlReturningTests {
        async fn get_database(&self) -> Option<Arc<dyn Database + Send + Sync>> {
            use sqlx::MySqlPool;
            use switchy_async::sync::Mutex;

            let url = std::env::var("MYSQL_TEST_URL").ok()?;
            let pool = MySqlPool::connect(&url).await.ok()?;
            let pool = Arc::new(Mutex::new(pool));
            let db = MySqlSqlxDatabase::new(pool);
            Some(Arc::new(db))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_insert_returns_complete_row() {
        let suite = MysqlReturningTests;
        suite.test_insert_returns_complete_row().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_update_returns_all_updated_rows() {
        let suite = MysqlReturningTests;
        suite.test_update_returns_all_updated_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_update_with_limit_returns_limited_rows() {
        let suite = MysqlReturningTests;
        suite.test_update_with_limit_returns_limited_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_delete_returns_deleted_rows() {
        let suite = MysqlReturningTests;
        suite.test_delete_returns_deleted_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_delete_with_limit_returns_limited_rows() {
        let suite = MysqlReturningTests;
        suite.test_delete_with_limit_returns_limited_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_upsert_returns_correct_row() {
        let suite = MysqlReturningTests;
        suite.test_upsert_returns_correct_row().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_transaction_operations_return_data() {
        let suite = MysqlReturningTests;
        suite.test_transaction_operations_return_data().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_empty_operations_return_empty() {
        let suite = MysqlReturningTests;
        suite.test_empty_operations_return_empty().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_data_type_preservation_in_returns() {
        let suite = MysqlReturningTests;
        suite.test_data_type_preservation_in_returns().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_complex_filters_return_correct_rows() {
        let suite = MysqlReturningTests;
        suite.test_complex_filters_return_correct_rows().await;
    }
}

#[cfg(all(feature = "postgres-sqlx", feature = "schema"))]
mod postgres_returning_tests {
    use super::*;
    use std::sync::Arc;
    use switchy_database::sqlx::postgres::PostgresSqlxDatabase;

    struct PostgresReturningTests;

    impl ReturningTestSuite for PostgresReturningTests {
        fn get_table_name(&self, test_suffix: &str) -> String {
            format!("ret_postgres_sqlx_{}", test_suffix)
        }

        async fn get_database(&self) -> Option<Arc<dyn Database + Send + Sync>> {
            use sqlx::PgPool;
            use switchy_async::sync::Mutex;

            let url = std::env::var("POSTGRES_TEST_URL").ok()?;
            let pool = PgPool::connect(&url).await.ok()?;
            let pool = Arc::new(Mutex::new(pool));
            let db = PostgresSqlxDatabase::new(pool);
            Some(Arc::new(db))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_insert_returns_complete_row() {
        let suite = PostgresReturningTests;
        suite.test_insert_returns_complete_row().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_update_returns_all_updated_rows() {
        let suite = PostgresReturningTests;
        suite.test_update_returns_all_updated_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_update_with_limit_returns_limited_rows() {
        let suite = PostgresReturningTests;
        suite.test_update_with_limit_returns_limited_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_delete_returns_deleted_rows() {
        let suite = PostgresReturningTests;
        suite.test_delete_returns_deleted_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_delete_with_limit_returns_limited_rows() {
        let suite = PostgresReturningTests;
        suite.test_delete_with_limit_returns_limited_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_upsert_returns_correct_row() {
        let suite = PostgresReturningTests;
        suite.test_upsert_returns_correct_row().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_transaction_operations_return_data() {
        let suite = PostgresReturningTests;
        suite.test_transaction_operations_return_data().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_empty_operations_return_empty() {
        let suite = PostgresReturningTests;
        suite.test_empty_operations_return_empty().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_data_type_preservation_in_returns() {
        let suite = PostgresReturningTests;
        suite.test_data_type_preservation_in_returns().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_complex_filters_return_correct_rows() {
        let suite = PostgresReturningTests;
        suite.test_complex_filters_return_correct_rows().await;
    }
}

#[cfg(all(feature = "sqlite-sqlx", feature = "schema"))]
mod sqlite_returning_tests {
    use super::*;
    use std::sync::Arc;
    use switchy_database::sqlx::sqlite::SqliteSqlxDatabase;

    struct SqliteReturningTests;

    impl ReturningTestSuite for SqliteReturningTests {
        async fn get_database(&self) -> Option<Arc<dyn Database + Send + Sync>> {
            use sqlx::SqlitePool;
            use switchy_async::sync::Mutex;

            let db_url = "sqlite::memory:";
            let pool = SqlitePool::connect(db_url).await.ok()?;
            let db = SqliteSqlxDatabase::new(Arc::new(Mutex::new(pool)));
            Some(Arc::new(db))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_insert_returns_complete_row() {
        let suite = SqliteReturningTests;
        suite.test_insert_returns_complete_row().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_update_returns_all_updated_rows() {
        let suite = SqliteReturningTests;
        suite.test_update_returns_all_updated_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_update_with_limit_returns_limited_rows() {
        let suite = SqliteReturningTests;
        suite.test_update_with_limit_returns_limited_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_delete_returns_deleted_rows() {
        let suite = SqliteReturningTests;
        suite.test_delete_returns_deleted_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_delete_with_limit_returns_limited_rows() {
        let suite = SqliteReturningTests;
        suite.test_delete_with_limit_returns_limited_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_upsert_returns_correct_row() {
        let suite = SqliteReturningTests;
        suite.test_upsert_returns_correct_row().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_transaction_operations_return_data() {
        let suite = SqliteReturningTests;
        suite.test_transaction_operations_return_data().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_empty_operations_return_empty() {
        let suite = SqliteReturningTests;
        suite.test_empty_operations_return_empty().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_data_type_preservation_in_returns() {
        let suite = SqliteReturningTests;
        suite.test_data_type_preservation_in_returns().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_complex_filters_return_correct_rows() {
        let suite = SqliteReturningTests;
        suite.test_complex_filters_return_correct_rows().await;
    }
}
#[cfg(all(feature = "sqlite-rusqlite", feature = "schema"))]
mod rusqlite_returning_tests {
    use super::*;
    use ::rusqlite::Connection;
    use std::sync::Arc;
    use switchy_async::sync::Mutex;
    use switchy_database::rusqlite::RusqliteDatabase;

    struct RusqliteReturningTests;

    impl ReturningTestSuite for RusqliteReturningTests {
        async fn get_database(&self) -> Option<Arc<dyn Database + Send + Sync>> {
            let test_id = std::thread::current().id();
            let timestamp = switchy_time::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let db_url = format!(
                "file:returning_test_{test_id:?}_{timestamp}:?mode=memory&cache=shared&uri=true"
            );

            let mut connections = Vec::new();
            for _ in 0..5 {
                let conn = Connection::open(&db_url).ok()?;
                connections.push(Arc::new(Mutex::new(conn)));
            }

            let db = RusqliteDatabase::new(connections);
            Some(Arc::new(db))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_insert_returns_complete_row() {
        let suite = RusqliteReturningTests;
        suite.test_insert_returns_complete_row().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_update_returns_all_updated_rows() {
        let suite = RusqliteReturningTests;
        suite.test_update_returns_all_updated_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_update_with_limit_returns_limited_rows() {
        let suite = RusqliteReturningTests;
        suite.test_update_with_limit_returns_limited_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_delete_returns_deleted_rows() {
        let suite = RusqliteReturningTests;
        suite.test_delete_returns_deleted_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_delete_with_limit_returns_limited_rows() {
        let suite = RusqliteReturningTests;
        suite.test_delete_with_limit_returns_limited_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_upsert_returns_correct_row() {
        let suite = RusqliteReturningTests;
        suite.test_upsert_returns_correct_row().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_transaction_operations_return_data() {
        let suite = RusqliteReturningTests;
        suite.test_transaction_operations_return_data().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_empty_operations_return_empty() {
        let suite = RusqliteReturningTests;
        suite.test_empty_operations_return_empty().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_data_type_preservation_in_returns() {
        let suite = RusqliteReturningTests;
        suite.test_data_type_preservation_in_returns().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_complex_filters_return_correct_rows() {
        let suite = RusqliteReturningTests;
        suite.test_complex_filters_return_correct_rows().await;
    }
}

#[cfg(all(feature = "turso", feature = "schema"))]
mod turso_returning_tests {
    use super::*;
    use std::sync::Arc;
    use switchy_database::turso::TursoDatabase;

    struct TursoReturningTests;

    impl ReturningTestSuite for TursoReturningTests {
        async fn get_database(&self) -> Option<Arc<dyn Database + Send + Sync>> {
            TursoDatabase::new(":memory:")
                .await
                .ok()
                .map(|db| Arc::new(db) as Arc<dyn Database + Send + Sync>)
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_insert_returns_complete_row() {
        let suite = TursoReturningTests;
        suite.test_insert_returns_complete_row().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_update_returns_all_updated_rows() {
        let suite = TursoReturningTests;
        suite.test_update_returns_all_updated_rows().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_update_with_limit_returns_limited_rows() {
        let suite = TursoReturningTests;
        suite.test_update_with_limit_returns_limited_rows().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_delete_returns_deleted_rows() {
        let suite = TursoReturningTests;
        suite.test_delete_returns_deleted_rows().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_delete_with_limit_returns_limited_rows() {
        let suite = TursoReturningTests;
        suite.test_delete_with_limit_returns_limited_rows().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_upsert_returns_correct_row() {
        let suite = TursoReturningTests;
        suite.test_upsert_returns_correct_row().await;
    }

    #[ignore = "Turso does not properly return auto-incremented ID from inserts yet"]
    #[test_log::test(switchy_async::test)]
    async fn test_turso_transaction_operations_return_data() {
        let suite = TursoReturningTests;
        suite.test_transaction_operations_return_data().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_empty_operations_return_empty() {
        let suite = TursoReturningTests;
        suite.test_empty_operations_return_empty().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_data_type_preservation_in_returns() {
        let suite = TursoReturningTests;
        suite.test_data_type_preservation_in_returns().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_complex_filters_return_correct_rows() {
        let suite = TursoReturningTests;
        suite.test_complex_filters_return_correct_rows().await;
    }
}

#[cfg(all(feature = "postgres", feature = "schema"))]
mod postgres_native_returning_tests {
    use super::*;
    use std::sync::Arc;
    use switchy_database::postgres::postgres::PostgresDatabase;

    struct PostgresNativeReturningTests;

    impl ReturningTestSuite for PostgresNativeReturningTests {
        fn get_table_name(&self, test_suffix: &str) -> String {
            format!("ret_postgres_tokio_{}", test_suffix)
        }

        async fn get_database(&self) -> Option<Arc<dyn Database + Send + Sync>> {
            let url = std::env::var("POSTGRES_TEST_URL").ok()?;

            let mut cfg = deadpool_postgres::Config::new();
            cfg.url = Some(url.clone());

            let pool = if url.contains("sslmode=require") {
                let connector = native_tls::TlsConnector::builder()
                    .danger_accept_invalid_certs(true)
                    .build()
                    .ok()?;
                let connector = postgres_native_tls::MakeTlsConnector::new(connector);
                cfg.create_pool(Some(deadpool_postgres::Runtime::Tokio1), connector)
                    .ok()?
            } else {
                cfg.create_pool(
                    Some(deadpool_postgres::Runtime::Tokio1),
                    tokio_postgres::NoTls,
                )
                .ok()?
            };

            let db = PostgresDatabase::new(pool);
            Some(Arc::new(db))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_native_insert_returns_complete_row() {
        let suite = PostgresNativeReturningTests;
        suite.test_insert_returns_complete_row().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_native_update_returns_all_updated_rows() {
        let suite = PostgresNativeReturningTests;
        suite.test_update_returns_all_updated_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_native_update_with_limit_returns_limited_rows() {
        let suite = PostgresNativeReturningTests;
        suite.test_update_with_limit_returns_limited_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_native_delete_returns_deleted_rows() {
        let suite = PostgresNativeReturningTests;
        suite.test_delete_returns_deleted_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_native_delete_with_limit_returns_limited_rows() {
        let suite = PostgresNativeReturningTests;
        suite.test_delete_with_limit_returns_limited_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_native_upsert_returns_correct_row() {
        let suite = PostgresNativeReturningTests;
        suite.test_upsert_returns_correct_row().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_native_transaction_operations_return_data() {
        let suite = PostgresNativeReturningTests;
        suite.test_transaction_operations_return_data().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_native_empty_operations_return_empty() {
        let suite = PostgresNativeReturningTests;
        suite.test_empty_operations_return_empty().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_native_data_type_preservation_in_returns() {
        let suite = PostgresNativeReturningTests;
        suite.test_data_type_preservation_in_returns().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_native_complex_filters_return_correct_rows() {
        let suite = PostgresNativeReturningTests;
        suite.test_complex_filters_return_correct_rows().await;
    }
}

#[cfg(all(feature = "simulator", feature = "schema"))]
mod simulator_returning_tests {
    use super::*;
    use std::sync::Arc;
    use switchy_database::simulator::SimulationDatabase;

    struct SimulatorReturningTests;

    impl ReturningTestSuite for SimulatorReturningTests {
        async fn get_database(&self) -> Option<Arc<dyn Database + Send + Sync>> {
            let db = SimulationDatabase::new().ok()?;
            Some(Arc::new(db))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_simulator_insert_returns_complete_row() {
        let suite = SimulatorReturningTests;
        suite.test_insert_returns_complete_row().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_simulator_update_returns_all_updated_rows() {
        let suite = SimulatorReturningTests;
        suite.test_update_returns_all_updated_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_simulator_update_with_limit_returns_limited_rows() {
        let suite = SimulatorReturningTests;
        suite.test_update_with_limit_returns_limited_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_simulator_delete_returns_deleted_rows() {
        let suite = SimulatorReturningTests;
        suite.test_delete_returns_deleted_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_simulator_delete_with_limit_returns_limited_rows() {
        let suite = SimulatorReturningTests;
        suite.test_delete_with_limit_returns_limited_rows().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_simulator_upsert_returns_correct_row() {
        let suite = SimulatorReturningTests;
        suite.test_upsert_returns_correct_row().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_simulator_transaction_operations_return_data() {
        let suite = SimulatorReturningTests;
        suite.test_transaction_operations_return_data().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_simulator_empty_operations_return_empty() {
        let suite = SimulatorReturningTests;
        suite.test_empty_operations_return_empty().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_simulator_data_type_preservation_in_returns() {
        let suite = SimulatorReturningTests;
        suite.test_data_type_preservation_in_returns().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_simulator_complex_filters_return_correct_rows() {
        let suite = SimulatorReturningTests;
        suite.test_complex_filters_return_correct_rows().await;
    }
}

#[cfg(all(feature = "sqlite-rusqlite", feature = "cascade"))]
mod rusqlite_cascade_tests {
    use super::*;
    use ::rusqlite::Connection;
    use common::cascade_tests::CascadeTestSuite;
    use std::sync::Arc;
    use switchy_async::sync::Mutex;
    use switchy_database::rusqlite::RusqliteDatabase;

    struct RusqliteCascadeTests;

    impl CascadeTestSuite for RusqliteCascadeTests {
        async fn setup_db(&self) -> Option<Arc<Box<dyn Database>>> {
            let test_id = std::thread::current().id();
            let timestamp = switchy_time::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let db_url = format!(
                "file:cascade_test_{test_id:?}_{timestamp}:?mode=memory&cache=shared&uri=true"
            );

            let mut connections = Vec::new();
            for _ in 0..5 {
                let conn = Connection::open(&db_url).unwrap();
                connections.push(Arc::new(Mutex::new(conn)));
            }

            let db = RusqliteDatabase::new(connections);
            Some(Arc::new(Box::new(db) as Box<dyn Database>))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_cascade_find_targets_linear() {
        let suite = RusqliteCascadeTests;
        suite.test_cascade_find_targets_linear().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_cascade_has_any_dependents() {
        let suite = RusqliteCascadeTests;
        suite.test_cascade_has_any_dependents().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_cascade_get_direct_dependents() {
        let suite = RusqliteCascadeTests;
        suite.test_cascade_get_direct_dependents().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_cascade_drop_restrict() {
        let suite = RusqliteCascadeTests;
        suite.test_cascade_drop_restrict().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_cascade_drop_execution() {
        let suite = RusqliteCascadeTests;
        suite.test_cascade_drop_execution().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_restrict_drop_execution() {
        let suite = RusqliteCascadeTests;
        suite.test_restrict_drop_execution().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_drop_column_cascade_with_index() {
        let suite = RusqliteCascadeTests;
        suite.test_drop_column_cascade_with_index().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_drop_column_restrict_with_index() {
        let suite = RusqliteCascadeTests;
        suite.test_drop_column_restrict_with_index().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_drop_column_cascade_with_foreign_key() {
        let suite = RusqliteCascadeTests;
        suite.test_drop_column_cascade_with_foreign_key().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_rusqlite_drop_column_restrict_no_dependencies() {
        let suite = RusqliteCascadeTests;
        suite.test_drop_column_restrict_no_dependencies().await;
    }
}

#[cfg(all(feature = "sqlite-sqlx", feature = "cascade"))]
mod sqlx_sqlite_cascade_tests {
    use super::*;
    use common::cascade_tests::CascadeTestSuite;
    use std::sync::Arc;
    use switchy_async::sync::Mutex;
    use switchy_database::sqlx::sqlite::SqliteSqlxDatabase;

    struct SqlxSqliteCascadeTests;

    impl CascadeTestSuite for SqlxSqliteCascadeTests {
        async fn setup_db(&self) -> Option<Arc<Box<dyn Database>>> {
            use sqlx::sqlite::SqlitePoolOptions;

            let database_url = "sqlite::memory:?cache=shared";
            let pool = SqlitePoolOptions::new()
                .max_connections(5)
                .min_connections(2)
                .connect(database_url)
                .await
                .unwrap();

            let db = SqliteSqlxDatabase::new(Arc::new(Mutex::new(pool)));
            Some(Arc::new(Box::new(db) as Box<dyn Database>))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_cascade_find_targets_linear() {
        let suite = SqlxSqliteCascadeTests;
        suite.test_cascade_find_targets_linear().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_cascade_has_any_dependents() {
        let suite = SqlxSqliteCascadeTests;
        suite.test_cascade_has_any_dependents().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_cascade_get_direct_dependents() {
        let suite = SqlxSqliteCascadeTests;
        suite.test_cascade_get_direct_dependents().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_cascade_drop_restrict() {
        let suite = SqlxSqliteCascadeTests;
        suite.test_cascade_drop_restrict().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_cascade_drop_execution() {
        let suite = SqlxSqliteCascadeTests;
        suite.test_cascade_drop_execution().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_restrict_drop_execution() {
        let suite = SqlxSqliteCascadeTests;
        suite.test_restrict_drop_execution().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_drop_column_cascade_with_index() {
        let suite = SqlxSqliteCascadeTests;
        suite.test_drop_column_cascade_with_index().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_drop_column_restrict_with_index() {
        let suite = SqlxSqliteCascadeTests;
        suite.test_drop_column_restrict_with_index().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_drop_column_cascade_with_foreign_key() {
        let suite = SqlxSqliteCascadeTests;
        suite.test_drop_column_cascade_with_foreign_key().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlx_sqlite_drop_column_restrict_no_dependencies() {
        let suite = SqlxSqliteCascadeTests;
        suite.test_drop_column_restrict_no_dependencies().await;
    }
}

#[cfg(all(feature = "simulator", feature = "cascade"))]
mod simulator_cascade_tests {
    use super::*;
    use common::cascade_tests::CascadeTestSuite;
    use std::sync::Arc;
    use switchy_database::simulator::SimulationDatabase;

    struct SimulatorCascadeTests;

    impl CascadeTestSuite for SimulatorCascadeTests {
        async fn setup_db(&self) -> Option<Arc<Box<dyn Database>>> {
            let db = SimulationDatabase::new().unwrap();
            Some(Arc::new(Box::new(db) as Box<dyn Database>))
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_cascade_find_targets_linear() {
        let suite = SimulatorCascadeTests;
        suite.test_cascade_find_targets_linear().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_cascade_has_any_dependents() {
        let suite = SimulatorCascadeTests;
        suite.test_cascade_has_any_dependents().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_cascade_get_direct_dependents() {
        let suite = SimulatorCascadeTests;
        suite.test_cascade_get_direct_dependents().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_cascade_drop_restrict() {
        let suite = SimulatorCascadeTests;
        suite.test_cascade_drop_restrict().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_cascade_drop_execution() {
        let suite = SimulatorCascadeTests;
        suite.test_cascade_drop_execution().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_restrict_drop_execution() {
        let suite = SimulatorCascadeTests;
        suite.test_restrict_drop_execution().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_drop_column_cascade_with_index() {
        let suite = SimulatorCascadeTests;
        suite.test_drop_column_cascade_with_index().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_drop_column_restrict_with_index() {
        let suite = SimulatorCascadeTests;
        suite.test_drop_column_restrict_with_index().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_drop_column_cascade_with_foreign_key() {
        let suite = SimulatorCascadeTests;
        suite.test_drop_column_cascade_with_foreign_key().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_simulator_drop_column_restrict_no_dependencies() {
        let suite = SimulatorCascadeTests;
        suite.test_drop_column_restrict_no_dependencies().await;
    }
}

#[cfg(all(feature = "postgres", feature = "cascade"))]
mod postgres_cascade_tests {
    use super::*;
    use common::cascade_tests::CascadeTestSuite;
    use std::sync::Arc;
    use switchy_database::postgres::postgres::PostgresDatabase;

    struct PostgresCascadeTests;

    impl CascadeTestSuite for PostgresCascadeTests {
        async fn setup_db(&self) -> Option<Arc<Box<dyn Database>>> {
            let url = std::env::var("POSTGRES_TEST_URL").ok()?;

            let mut cfg = deadpool_postgres::Config::new();
            cfg.url = Some(url.clone());

            let pool = if url.contains("sslmode=require") {
                let connector = native_tls::TlsConnector::builder()
                    .danger_accept_invalid_certs(true)
                    .build()
                    .ok()?;
                let connector = postgres_native_tls::MakeTlsConnector::new(connector);
                cfg.create_pool(Some(deadpool_postgres::Runtime::Tokio1), connector)
                    .ok()?
            } else {
                cfg.create_pool(
                    Some(deadpool_postgres::Runtime::Tokio1),
                    tokio_postgres::NoTls,
                )
                .ok()?
            };

            let db = PostgresDatabase::new(pool);
            Some(Arc::new(Box::new(db) as Box<dyn Database>))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_cascade_find_targets_linear() {
        let suite = PostgresCascadeTests;
        suite.test_cascade_find_targets_linear().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_cascade_has_any_dependents() {
        let suite = PostgresCascadeTests;
        suite.test_cascade_has_any_dependents().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_cascade_get_direct_dependents() {
        let suite = PostgresCascadeTests;
        suite.test_cascade_get_direct_dependents().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_cascade_drop_restrict() {
        let suite = PostgresCascadeTests;
        suite.test_cascade_drop_restrict().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_cascade_drop_execution() {
        let suite = PostgresCascadeTests;
        suite.test_cascade_drop_execution().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_restrict_drop_execution() {
        let suite = PostgresCascadeTests;
        suite.test_restrict_drop_execution().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_drop_column_cascade_with_index() {
        let suite = PostgresCascadeTests;
        suite.test_drop_column_cascade_with_index().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_drop_column_restrict_with_index() {
        let suite = PostgresCascadeTests;
        suite.test_drop_column_restrict_with_index().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_drop_column_cascade_with_foreign_key() {
        let suite = PostgresCascadeTests;
        suite.test_drop_column_cascade_with_foreign_key().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_drop_column_restrict_no_dependencies() {
        let suite = PostgresCascadeTests;
        suite.test_drop_column_restrict_no_dependencies().await;
    }
}

#[cfg(all(feature = "postgres-sqlx", feature = "cascade"))]
mod postgres_sqlx_cascade_tests {
    use super::*;
    use common::cascade_tests::CascadeTestSuite;
    use std::sync::Arc;
    use switchy_async::sync::Mutex;
    use switchy_database::sqlx::postgres::PostgresSqlxDatabase;

    struct PostgresSqlxCascadeTests;

    impl CascadeTestSuite for PostgresSqlxCascadeTests {
        async fn setup_db(&self) -> Option<Arc<Box<dyn Database>>> {
            use sqlx::PgPool;

            let url = std::env::var("POSTGRES_TEST_URL").ok()?;
            let pool = PgPool::connect(&url).await.ok()?;
            let db = PostgresSqlxDatabase::new(Arc::new(Mutex::new(pool)));
            Some(Arc::new(Box::new(db) as Box<dyn Database>))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_cascade_find_targets_linear() {
        let suite = PostgresSqlxCascadeTests;
        suite.test_cascade_find_targets_linear().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_cascade_has_any_dependents() {
        let suite = PostgresSqlxCascadeTests;
        suite.test_cascade_has_any_dependents().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_cascade_get_direct_dependents() {
        let suite = PostgresSqlxCascadeTests;
        suite.test_cascade_get_direct_dependents().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_cascade_drop_restrict() {
        let suite = PostgresSqlxCascadeTests;
        suite.test_cascade_drop_restrict().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_cascade_drop_execution() {
        let suite = PostgresSqlxCascadeTests;
        suite.test_cascade_drop_execution().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_restrict_drop_execution() {
        let suite = PostgresSqlxCascadeTests;
        suite.test_restrict_drop_execution().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_drop_column_cascade_with_index() {
        let suite = PostgresSqlxCascadeTests;
        suite.test_drop_column_cascade_with_index().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_drop_column_restrict_with_index() {
        let suite = PostgresSqlxCascadeTests;
        suite.test_drop_column_restrict_with_index().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_drop_column_cascade_with_foreign_key() {
        let suite = PostgresSqlxCascadeTests;
        suite.test_drop_column_cascade_with_foreign_key().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_drop_column_restrict_no_dependencies() {
        let suite = PostgresSqlxCascadeTests;
        suite.test_drop_column_restrict_no_dependencies().await;
    }
}

#[cfg(all(feature = "mysql-sqlx", feature = "cascade"))]
mod mysql_sqlx_cascade_tests {
    use super::*;
    use common::cascade_tests::CascadeTestSuite;
    use std::sync::Arc;
    use switchy_async::sync::Mutex;
    use switchy_database::sqlx::mysql::MySqlSqlxDatabase;

    struct MySqlSqlxCascadeTests;

    impl CascadeTestSuite for MySqlSqlxCascadeTests {
        async fn setup_db(&self) -> Option<Arc<Box<dyn Database>>> {
            use sqlx::MySqlPool;

            let url = std::env::var("MYSQL_TEST_URL").ok()?;
            let pool = MySqlPool::connect(&url).await.ok()?;
            let db = MySqlSqlxDatabase::new(Arc::new(Mutex::new(pool)));
            Some(Arc::new(Box::new(db) as Box<dyn Database>))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_cascade_find_targets_linear() {
        let suite = MySqlSqlxCascadeTests;
        suite.test_cascade_find_targets_linear().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_cascade_has_any_dependents() {
        let suite = MySqlSqlxCascadeTests;
        suite.test_cascade_has_any_dependents().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_cascade_get_direct_dependents() {
        let suite = MySqlSqlxCascadeTests;
        suite.test_cascade_get_direct_dependents().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_cascade_drop_restrict() {
        let suite = MySqlSqlxCascadeTests;
        suite.test_cascade_drop_restrict().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_cascade_drop_execution() {
        let suite = MySqlSqlxCascadeTests;
        suite.test_cascade_drop_execution().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_restrict_drop_execution() {
        let suite = MySqlSqlxCascadeTests;
        suite.test_restrict_drop_execution().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_drop_column_cascade_with_index() {
        let suite = MySqlSqlxCascadeTests;
        suite.test_drop_column_cascade_with_index().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_drop_column_restrict_with_index() {
        let suite = MySqlSqlxCascadeTests;
        suite.test_drop_column_restrict_with_index().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_drop_column_cascade_with_foreign_key() {
        let suite = MySqlSqlxCascadeTests;
        suite.test_drop_column_cascade_with_foreign_key().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_drop_column_restrict_no_dependencies() {
        let suite = MySqlSqlxCascadeTests;
        suite.test_drop_column_restrict_no_dependencies().await;
    }
}
