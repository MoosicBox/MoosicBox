#![cfg(feature = "schema")]

//! # Savepoint Integration Tests
//!
//! This file contains savepoint tests for database backends that support nested transactions
//! using savepoints (SAVEPOINT/RELEASE/ROLLBACK TO syntax).
//!
//! ## Turso Backend Exclusion
//!
//! **Turso is currently excluded from these tests** because it does not yet support savepoints.
//! This is a known limitation tracked in the upstream issue:
//! <https://github.com/tursodatabase/turso/issues/1829>
//!
//! Once Turso adds savepoint support, a `turso_savepoint_tests` module should be added here
//! following the same pattern as the rusqlite and sqlite-sqlx implementations.

mod common;

use common::savepoint_tests::SavepointTestSuite;
use std::sync::Arc;

// ===== RUSQLITE BACKEND TESTS =====
#[cfg(feature = "sqlite-rusqlite")]
mod rusqlite_savepoint_tests {
    use super::*;
    use rusqlite::Connection;
    use switchy_async::sync::Mutex;
    use switchy_database::rusqlite::RusqliteDatabase;

    struct RusqliteSavepointTests;

    const CONNECTION_POOL_SIZE: u8 = 5;

    impl SavepointTestSuite for RusqliteSavepointTests {
        type DatabaseType = RusqliteDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            // Always available - in-memory database
            let test_id = std::thread::current().id();
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let db_url =
                format!("file:testdb_{test_id:?}_{timestamp}:?mode=memory&cache=shared&uri=true");

            let mut connections = Vec::new();

            for i in 0..CONNECTION_POOL_SIZE {
                let conn =
                    Connection::open(&db_url).expect("Failed to create shared memory database");

                // Configure SQLite for better concurrency on all connections
                conn.pragma_update(None, "journal_mode", "WAL")
                    .expect("Failed to set WAL mode");
                conn.pragma_update(None, "busy_timeout", 5000)
                    .expect("Failed to set busy timeout");
                conn.pragma_update(None, "synchronous", "NORMAL")
                    .expect("Failed to set synchronous mode");
                conn.pragma_update(None, "cache_size", -64000)
                    .expect("Failed to set cache size");
                conn.pragma_update(None, "temp_store", "MEMORY")
                    .expect("Failed to set temp store");

                // Only create table in first connection since shared memory shares schema
                if i == 0 {
                    conn.execute(
                    "CREATE TABLE test_table (id INTEGER PRIMARY KEY, name TEXT, value INTEGER)",
                    [],
                )
                .expect("Failed to create test table");
                }

                connections.push(Arc::new(Mutex::new(conn)));
            }
            let db = RusqliteDatabase::new(connections);
            Some(Arc::new(db))
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_nested_savepoints_three_levels() {
        let suite = RusqliteSavepointTests;
        suite.test_nested_savepoints_three_levels().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_rollback_to_middle_savepoint() {
        let suite = RusqliteSavepointTests;
        suite.test_rollback_to_middle_savepoint().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_release_savepoints_out_of_order() {
        let suite = RusqliteSavepointTests;
        suite.test_release_savepoints_out_of_order().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_savepoint_with_data_operations() {
        let suite = RusqliteSavepointTests;
        suite.test_savepoint_with_data_operations().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_commit_with_unreleased_savepoints() {
        let suite = RusqliteSavepointTests;
        suite.test_commit_with_unreleased_savepoints().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_savepoint_name_validation() {
        let suite = RusqliteSavepointTests;
        suite.test_savepoint_name_validation().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_sequential_savepoints_different_transactions() {
        let suite = RusqliteSavepointTests;
        suite
            .test_sequential_savepoints_different_transactions()
            .await;
    }

    #[test_log::test(switchy_async::test(real_time))]
    async fn test_rusqlite_concurrent_savepoints_with_isolation() {
        let suite = RusqliteSavepointTests;
        suite.test_concurrent_savepoints_with_isolation().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_savepoint_after_failed_operation() {
        let suite = RusqliteSavepointTests;
        suite.test_savepoint_after_failed_operation().await;
    }
}

// ===== SQLITE SQLX BACKEND TESTS =====
#[cfg(feature = "sqlite-sqlx")]
mod sqlite_sqlx_savepoint_tests {
    use super::*;
    use std::sync::Arc;
    use switchy_async::sync::Mutex;
    use switchy_database::sqlx::sqlite::SqliteSqlxDatabase;

    struct SqliteSqlxSavepointTests;

    impl SavepointTestSuite for SqliteSqlxSavepointTests {
        type DatabaseType = SqliteSqlxDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            use sqlx::sqlite::SqlitePoolOptions;

            // Always available - in-memory database with WAL mode and connection pool
            let database_url = "sqlite::memory:?cache=shared";
            let pool = SqlitePoolOptions::new()
                .max_connections(5) // Match Rusqlite pool size
                .min_connections(2)
                .connect(database_url)
                .await
                .ok()?;

            // Configure SQLite for better concurrency
            sqlx::query("PRAGMA journal_mode = WAL")
                .execute(&pool)
                .await
                .expect("Failed to set WAL mode");
            sqlx::query("PRAGMA busy_timeout = 5000")
                .execute(&pool)
                .await
                .expect("Failed to set busy timeout");
            sqlx::query("PRAGMA synchronous = NORMAL")
                .execute(&pool)
                .await
                .expect("Failed to set synchronous mode");
            sqlx::query("PRAGMA cache_size = -64000")
                .execute(&pool)
                .await
                .expect("Failed to set cache size");
            sqlx::query("PRAGMA temp_store = MEMORY")
                .execute(&pool)
                .await
                .expect("Failed to set temp store");

            let pool = Arc::new(Mutex::new(pool));
            Some(Arc::new(SqliteSqlxDatabase::new(pool)))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_nested_savepoints_three_levels() {
        let suite = SqliteSqlxSavepointTests;
        suite.test_nested_savepoints_three_levels().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_rollback_to_middle_savepoint() {
        let suite = SqliteSqlxSavepointTests;
        suite.test_rollback_to_middle_savepoint().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_release_savepoints_out_of_order() {
        let suite = SqliteSqlxSavepointTests;
        suite.test_release_savepoints_out_of_order().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_savepoint_with_data_operations() {
        let suite = SqliteSqlxSavepointTests;
        suite.test_savepoint_with_data_operations().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_commit_with_unreleased_savepoints() {
        let suite = SqliteSqlxSavepointTests;
        suite.test_commit_with_unreleased_savepoints().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_savepoint_name_validation() {
        let suite = SqliteSqlxSavepointTests;
        suite.test_savepoint_name_validation().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_sequential_savepoints_different_transactions() {
        let suite = SqliteSqlxSavepointTests;
        suite
            .test_sequential_savepoints_different_transactions()
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_concurrent_savepoints_with_isolation() {
        let suite = SqliteSqlxSavepointTests;
        suite.test_concurrent_savepoints_with_isolation().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_savepoint_after_failed_operation() {
        let suite = SqliteSqlxSavepointTests;
        suite.test_savepoint_after_failed_operation().await;
    }
}

// ===== POSTGRES RAW BACKEND TESTS =====
#[cfg(all(feature = "postgres-raw", not(feature = "postgres-sqlx")))]
mod postgres_savepoint_tests {
    use super::*;
    use switchy_database::postgres::postgres::PostgresDatabase;

    struct PostgresSavepointTests;

    impl SavepointTestSuite for PostgresSavepointTests {
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
    async fn test_postgres_nested_savepoints_three_levels() {
        let suite = PostgresSavepointTests;
        suite.test_nested_savepoints_three_levels().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_rollback_to_middle_savepoint() {
        let suite = PostgresSavepointTests;
        suite.test_rollback_to_middle_savepoint().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_release_savepoints_out_of_order() {
        let suite = PostgresSavepointTests;
        suite.test_release_savepoints_out_of_order().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_savepoint_with_data_operations() {
        let suite = PostgresSavepointTests;
        suite.test_savepoint_with_data_operations().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_commit_with_unreleased_savepoints() {
        let suite = PostgresSavepointTests;
        suite.test_commit_with_unreleased_savepoints().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_savepoint_name_validation() {
        let suite = PostgresSavepointTests;
        suite.test_savepoint_name_validation().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sequential_savepoints_different_transactions() {
        let suite = PostgresSavepointTests;
        suite
            .test_sequential_savepoints_different_transactions()
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_concurrent_savepoints_with_isolation() {
        let suite = PostgresSavepointTests;
        suite.test_concurrent_savepoints_with_isolation().await;
    }

    // NOTE: test_savepoint_after_failed_operation is intentionally excluded
    // from PostgreSQL test suites. PostgreSQL does not allow creating new
    // savepoints after an error occurs in a transaction - the transaction
    // enters an aborted state requiring ROLLBACK. See the test documentation
    // in common/savepoint_tests.rs for details and proper PostgreSQL patterns.
}

// ===== POSTGRES SQLX BACKEND TESTS =====
#[cfg(feature = "postgres-sqlx")]
mod postgres_sqlx_savepoint_tests {
    use super::*;
    use sqlx::PgPool;
    use std::sync::Arc;
    use switchy_async::sync::Mutex;
    use switchy_database::sqlx::postgres::PostgresSqlxDatabase;

    struct PostgresSqlxSavepointTests;

    impl SavepointTestSuite for PostgresSqlxSavepointTests {
        type DatabaseType = PostgresSqlxDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            let url = std::env::var("POSTGRES_TEST_URL").ok()?;
            let pool = PgPool::connect(&url).await.ok()?;
            let pool = Arc::new(Mutex::new(pool));
            Some(Arc::new(PostgresSqlxDatabase::new(pool)))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_nested_savepoints_three_levels() {
        let suite = PostgresSqlxSavepointTests;
        suite.test_nested_savepoints_three_levels().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_rollback_to_middle_savepoint() {
        let suite = PostgresSqlxSavepointTests;
        suite.test_rollback_to_middle_savepoint().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_release_savepoints_out_of_order() {
        let suite = PostgresSqlxSavepointTests;
        suite.test_release_savepoints_out_of_order().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_savepoint_with_data_operations() {
        let suite = PostgresSqlxSavepointTests;
        suite.test_savepoint_with_data_operations().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_commit_with_unreleased_savepoints() {
        let suite = PostgresSqlxSavepointTests;
        suite.test_commit_with_unreleased_savepoints().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_savepoint_name_validation() {
        let suite = PostgresSqlxSavepointTests;
        suite.test_savepoint_name_validation().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_sequential_savepoints_different_transactions() {
        let suite = PostgresSqlxSavepointTests;
        suite
            .test_sequential_savepoints_different_transactions()
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_concurrent_savepoints_with_isolation() {
        let suite = PostgresSqlxSavepointTests;
        suite.test_concurrent_savepoints_with_isolation().await;
    }

    // NOTE: test_savepoint_after_failed_operation is intentionally excluded
    // from PostgreSQL test suites. PostgreSQL does not allow creating new
    // savepoints after an error occurs in a transaction - the transaction
    // enters an aborted state requiring ROLLBACK. See the test documentation
    // in common/savepoint_tests.rs for details and proper PostgreSQL patterns.
}

// ===== MYSQL SQLX BACKEND TESTS =====
#[cfg(feature = "mysql-sqlx")]
mod mysql_sqlx_savepoint_tests {
    use super::*;
    use sqlx::MySqlPool;
    use std::sync::Arc;
    use switchy_async::sync::Mutex;
    use switchy_database::sqlx::mysql::MySqlSqlxDatabase;

    struct MysqlSqlxSavepointTests;

    impl SavepointTestSuite for MysqlSqlxSavepointTests {
        type DatabaseType = MySqlSqlxDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            let url = std::env::var("MYSQL_TEST_URL").ok()?;
            let pool = MySqlPool::connect(&url).await.ok()?;
            let pool = Arc::new(Mutex::new(pool));
            Some(Arc::new(MySqlSqlxDatabase::new(pool)))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_nested_savepoints_three_levels() {
        let suite = MysqlSqlxSavepointTests;
        suite.test_nested_savepoints_three_levels().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_rollback_to_middle_savepoint() {
        let suite = MysqlSqlxSavepointTests;
        suite.test_rollback_to_middle_savepoint().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_release_savepoints_out_of_order() {
        let suite = MysqlSqlxSavepointTests;
        suite.test_release_savepoints_out_of_order().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_savepoint_with_data_operations() {
        let suite = MysqlSqlxSavepointTests;
        suite.test_savepoint_with_data_operations().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_commit_with_unreleased_savepoints() {
        let suite = MysqlSqlxSavepointTests;
        suite.test_commit_with_unreleased_savepoints().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_savepoint_name_validation() {
        let suite = MysqlSqlxSavepointTests;
        suite.test_savepoint_name_validation().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_sequential_savepoints_different_transactions() {
        let suite = MysqlSqlxSavepointTests;
        suite
            .test_sequential_savepoints_different_transactions()
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_concurrent_savepoints_with_isolation() {
        let suite = MysqlSqlxSavepointTests;
        suite.test_concurrent_savepoints_with_isolation().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_savepoint_after_failed_operation() {
        let suite = MysqlSqlxSavepointTests;
        suite.test_savepoint_after_failed_operation().await;
    }
}
