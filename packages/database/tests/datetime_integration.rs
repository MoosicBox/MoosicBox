mod common;

use chrono::NaiveDateTime;
use common::datetime_tests::DateTimeTestSuite;
use std::sync::Arc;
use switchy_database::{Database, DatabaseValue};

// ===== RUSQLITE BACKEND TESTS =====
#[cfg(feature = "sqlite-rusqlite")]
mod rusqlite_datetime_tests {
    use super::*;
    use moosicbox_json_utils::database::ToValue as _;
    use rusqlite::Connection;
    use switchy_async::sync::Mutex;
    use switchy_database::rusqlite::RusqliteDatabase;

    struct RusqliteDateTimeTests;

    const CONNECTION_POOL_SIZE: u8 = 3;
    impl DateTimeTestSuite<&'static str> for RusqliteDateTimeTests {
        type DatabaseType = RusqliteDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            // Always available - in-memory database
            let test_id = std::thread::current().id();
            let timestamp = switchy_time::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();

            let db_url =
                format!("file:testdb_{test_id:?}_{timestamp}:?mode=memory&cache=shared&uri=true");

            let mut connections = Vec::new();

            for _i in 0..CONNECTION_POOL_SIZE {
                let conn =
                    Connection::open(&db_url).expect("Failed to create shared memory database");

                // Configure SQLite for better concurrency
                conn.pragma_update(None, "journal_mode", "WAL")
                    .expect("Failed to set WAL mode");
                conn.pragma_update(None, "busy_timeout", 5000)
                    .expect("Failed to set busy timeout");

                connections.push(Arc::new(Mutex::new(conn)));
            }

            let db = RusqliteDatabase::new(connections);
            Some(Arc::new(db))
        }

        async fn create_test_table(&self, db: &Self::DatabaseType, table_name: &str) {
            let query = format!(
                r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id INTEGER PRIMARY KEY,
                    created_at TIMESTAMP,
                    expires_at TIMESTAMP,
                    scheduled_for TIMESTAMP,
                    description TEXT
                )
                "#,
                table_name
            );
            db.exec_raw(&query)
                .await
                .expect("Failed to create datetime test table");
        }

        async fn cleanup_test_data(&self, db: &Self::DatabaseType, table_name: &str) {
            let query = format!("DROP TABLE IF EXISTS {}", table_name);
            db.exec_raw(&query)
                .await
                .expect("Failed to drop test table");
        }

        async fn get_timestamp_column(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            column: &str,
            id: i32,
        ) -> Option<NaiveDateTime> {
            let query = format!("SELECT {} FROM {} WHERE id = ?", column, table_name);
            let rows = db
                .query_raw_params(&query, &[DatabaseValue::Int64(id as i64)])
                .await
                .unwrap();

            if let Some(row) = rows.first() {
                return Some(row.to_value(column).unwrap());
            }
            None
        }

        async fn get_row_id_by_description(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            description: &str,
        ) -> i32 {
            let query = format!(
                "SELECT id FROM {} WHERE description = ? ORDER BY id LIMIT 1",
                table_name
            );
            let rows = db
                .query_raw_params(&query, &[DatabaseValue::String(description.to_string())])
                .await
                .expect("Failed to get row by description");

            if rows.is_empty() {
                panic!("No row found with description '{}'", description);
            }

            match rows[0].get("id").unwrap() {
                DatabaseValue::Int32(n) => n,
                DatabaseValue::Int64(n) => n as i32,
                _ => panic!("Expected number for id"),
            }
        }

        async fn insert_with_now(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (created_at, description) VALUES (?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    DatabaseValue::Now,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with NOW()");
        }

        async fn insert_with_expires_at(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            expires_at: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (expires_at, description) VALUES (?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[expires_at, DatabaseValue::String(description.to_string())],
            )
            .await
            .expect("Failed to insert with expires_at");
        }

        async fn insert_with_scheduled_for(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            scheduled_for: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (scheduled_for, description) VALUES (?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    scheduled_for,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with scheduled_for");
        }

        async fn insert_with_all_timestamps(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            created_at: DatabaseValue,
            expires_at: DatabaseValue,
            scheduled_for: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {table_name} (created_at, expires_at, scheduled_for, description) VALUES (?, ?, ?, ?)"
            );
            db.exec_raw_params(
                &query,
                &[
                    created_at,
                    expires_at,
                    scheduled_for,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with all timestamps");
        }

        fn gen_param(&self, _i: u8) -> &'static str {
            "?"
        }
    }

    // Test implementations
    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_now_insert() {
        let suite = RusqliteDateTimeTests;
        suite.test_now_insert("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_now_in_where_clause() {
        let suite = RusqliteDateTimeTests;
        suite.test_now_in_where_clause("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_now_plus_days() {
        let suite = RusqliteDateTimeTests;
        suite.test_now_plus_days("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_now_minus_days() {
        let suite = RusqliteDateTimeTests;
        suite.test_now_minus_days("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_now_plus_hours_minutes_seconds() {
        let suite = RusqliteDateTimeTests;
        suite.test_now_plus_hours_minutes_seconds("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_now_plus_minutes_normalization() {
        let suite = RusqliteDateTimeTests;
        suite.test_now_plus_minutes_normalization("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_now_plus_complex_interval() {
        let suite = RusqliteDateTimeTests;
        suite.test_now_plus_complex_interval("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_zero_interval_returns_now() {
        let suite = RusqliteDateTimeTests;
        suite.test_zero_interval_returns_now("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_mixed_parameters() {
        let suite = RusqliteDateTimeTests;
        suite.test_mixed_parameters("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_now_consistency_in_transaction() {
        let suite = RusqliteDateTimeTests;
        suite.test_now_consistency_in_transaction("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_duration_conversion() {
        let suite = RusqliteDateTimeTests;
        suite.test_duration_conversion("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_now_plus_interval() {
        let suite = RusqliteDateTimeTests;
        suite.test_now_plus_interval("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_now_minus_interval() {
        let suite = RusqliteDateTimeTests;
        suite.test_now_minus_interval("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_complex_interval_operations() {
        let suite = RusqliteDateTimeTests;
        suite.test_complex_interval_operations("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_update_with_now() {
        let suite = RusqliteDateTimeTests;
        suite.test_update_with_now("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_multiple_now_consistency() {
        let suite = RusqliteDateTimeTests;
        suite.test_multiple_now_consistency("rusqlite").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_mixed_now_operations() {
        let suite = RusqliteDateTimeTests;
        suite.test_mixed_now_operations("rusqlite").await;
    }
}

// ===== SQLITE SQLX BACKEND TESTS =====
#[cfg(feature = "sqlite-sqlx")]
mod sqlite_sqlx_datetime_tests {
    use super::*;
    use moosicbox_json_utils::database::ToValue as _;
    use switchy_async::sync::Mutex;
    use switchy_database::sqlx::sqlite::SqliteSqlxDatabase;

    use sqlx::sqlite::SqlitePoolOptions;
    struct SqliteSqlxDateTimeTests;

    impl DateTimeTestSuite<&'static str> for SqliteSqlxDateTimeTests {
        type DatabaseType = SqliteSqlxDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            let database_url = "sqlite::memory:?cache=shared";
            let pool = SqlitePoolOptions::new()
                .max_connections(3)
                .min_connections(1)
                .connect(database_url)
                .await
                .ok()?;

            Some(Arc::new(SqliteSqlxDatabase::new(Arc::new(Mutex::new(
                pool,
            )))))
        }

        async fn create_test_table(&self, db: &Self::DatabaseType, table_name: &str) {
            let query = format!(
                r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id INTEGER PRIMARY KEY,
                    created_at TIMESTAMP,
                    expires_at TIMESTAMP,
                    scheduled_for TIMESTAMP,
                    description TEXT
                )
                "#,
                table_name
            );
            db.exec_raw(&query)
                .await
                .expect("Failed to create datetime test table");
        }

        async fn cleanup_test_data(&self, db: &Self::DatabaseType, table_name: &str) {
            let query = format!("DROP TABLE IF EXISTS {}", table_name);
            db.exec_raw(&query)
                .await
                .expect("Failed to drop test table");
        }

        async fn get_timestamp_column(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            column: &str,
            id: i32,
        ) -> Option<NaiveDateTime> {
            let query = format!("SELECT {} FROM {} WHERE id = ?", column, table_name);
            let rows = db
                .query_raw_params(&query, &[DatabaseValue::Int64(id as i64)])
                .await
                .unwrap();

            if let Some(row) = rows.first() {
                return Some(row.to_value(column).unwrap());
            }
            None
        }

        async fn get_row_id_by_description(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            description: &str,
        ) -> i32 {
            let query = format!(
                "SELECT id FROM {} WHERE description = ? ORDER BY id LIMIT 1",
                table_name
            );
            let rows = db
                .query_raw_params(&query, &[DatabaseValue::String(description.to_string())])
                .await
                .expect("Failed to get row by description");

            if rows.is_empty() {
                panic!("No row found with description '{}'", description);
            }

            match rows[0].get("id").unwrap() {
                DatabaseValue::Int32(n) => n,
                DatabaseValue::Int64(n) => n as i32,
                _ => panic!("Expected number for id"),
            }
        }

        async fn insert_with_now(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (created_at, description) VALUES (?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    DatabaseValue::Now,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with NOW()");
        }

        async fn insert_with_expires_at(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            expires_at: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (expires_at, description) VALUES (?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[expires_at, DatabaseValue::String(description.to_string())],
            )
            .await
            .expect("Failed to insert with expires_at");
        }

        async fn insert_with_scheduled_for(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            scheduled_for: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (scheduled_for, description) VALUES (?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    scheduled_for,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with scheduled_for");
        }

        async fn insert_with_all_timestamps(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            created_at: DatabaseValue,
            expires_at: DatabaseValue,
            scheduled_for: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (created_at, expires_at, scheduled_for, description) VALUES (?, ?, ?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    created_at,
                    expires_at,
                    scheduled_for,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with all timestamps");
        }

        fn gen_param(&self, _i: u8) -> &'static str {
            "?"
        }
    }

    // Test implementations for SQLite SQLX
    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_now_insert() {
        let suite = SqliteSqlxDateTimeTests;
        suite.test_now_insert("sqlx_sqlite").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_now_plus_days() {
        let suite = SqliteSqlxDateTimeTests;
        suite.test_now_plus_days("sqlx_sqlite").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_now_plus_complex_interval() {
        let suite = SqliteSqlxDateTimeTests;
        suite.test_now_plus_complex_interval("sqlx_sqlite").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_mixed_parameters() {
        let suite = SqliteSqlxDateTimeTests;
        suite.test_mixed_parameters("sqlx_sqlite").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_now_in_where_clause() {
        let suite = SqliteSqlxDateTimeTests;
        suite.test_now_in_where_clause("sqlx_sqlite").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_now_minus_days() {
        let suite = SqliteSqlxDateTimeTests;
        suite.test_now_minus_days("sqlx_sqlite").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_now_plus_hours_minutes_seconds() {
        let suite = SqliteSqlxDateTimeTests;
        suite
            .test_now_plus_hours_minutes_seconds("sqlx_sqlite")
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_now_plus_minutes_normalization() {
        let suite = SqliteSqlxDateTimeTests;
        suite
            .test_now_plus_minutes_normalization("sqlx_sqlite")
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_zero_interval_returns_now() {
        let suite = SqliteSqlxDateTimeTests;
        suite.test_zero_interval_returns_now("sqlx_sqlite").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_now_consistency_in_transaction() {
        let suite = SqliteSqlxDateTimeTests;
        suite
            .test_now_consistency_in_transaction("sqlx_sqlite")
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_duration_conversion() {
        let suite = SqliteSqlxDateTimeTests;
        suite.test_duration_conversion("sqlx_sqlite").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_now_plus_interval() {
        let suite = SqliteSqlxDateTimeTests;
        suite.test_now_plus_interval("sqlx_sqlite").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_now_minus_interval() {
        let suite = SqliteSqlxDateTimeTests;
        suite.test_now_minus_interval("sqlx_sqlite").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_complex_interval_operations() {
        let suite = SqliteSqlxDateTimeTests;
        suite.test_complex_interval_operations("sqlx_sqlite").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_update_with_now() {
        let suite = SqliteSqlxDateTimeTests;
        suite.test_update_with_now("sqlx_sqlite").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_multiple_now_consistency() {
        let suite = SqliteSqlxDateTimeTests;
        suite.test_multiple_now_consistency("sqlx_sqlite").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_mixed_now_operations() {
        let suite = SqliteSqlxDateTimeTests;
        suite.test_mixed_now_operations("sqlx_sqlite").await;
    }
}

// ===== POSTGRES SQLX BACKEND TESTS =====
#[cfg(feature = "postgres-sqlx")]
mod postgres_sqlx_datetime_tests {
    use super::*;
    use moosicbox_json_utils::database::ToValue as _;
    use sqlx::PgPool;
    use std::sync::Arc;
    use switchy_async::sync::Mutex;
    use switchy_database::sqlx::postgres::PostgresSqlxDatabase;

    struct PostgresSqlxDateTimeTests;

    impl DateTimeTestSuite<String> for PostgresSqlxDateTimeTests {
        type DatabaseType = PostgresSqlxDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            let url = std::env::var("POSTGRES_TEST_URL").ok()?;
            let pool = PgPool::connect(&url).await.ok()?;
            let pool = Arc::new(Mutex::new(pool));
            Some(Arc::new(PostgresSqlxDatabase::new(pool)))
        }

        async fn create_test_table(&self, db: &Self::DatabaseType, table_name: &str) {
            let query = format!(
                r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id BIGSERIAL PRIMARY KEY,
                    created_at TIMESTAMP,
                    expires_at TIMESTAMP,
                    scheduled_for TIMESTAMP,
                    description TEXT
                )
                "#,
                table_name
            );
            db.exec_raw(&query)
                .await
                .expect("Failed to create datetime test table");
        }

        async fn cleanup_test_data(&self, db: &Self::DatabaseType, table_name: &str) {
            let query = format!("DROP TABLE IF EXISTS {}", table_name);
            db.exec_raw(&query)
                .await
                .expect("Failed to drop test table");
        }

        async fn get_timestamp_column(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            column: &str,
            id: i32,
        ) -> Option<NaiveDateTime> {
            let query = format!("SELECT {} FROM {} WHERE id = $1", column, table_name);
            let rows = db
                .query_raw_params(&query, &[DatabaseValue::Int64(id as i64)])
                .await
                .unwrap();

            if let Some(row) = rows.first() {
                return Some(row.to_value(column).unwrap());
            }
            None
        }

        async fn get_row_id_by_description(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            description: &str,
        ) -> i32 {
            let query = format!(
                "SELECT id FROM {} WHERE description = $1 ORDER BY id LIMIT 1",
                table_name
            );
            let rows = db
                .query_raw_params(&query, &[DatabaseValue::String(description.to_string())])
                .await
                .expect("Failed to get row by description");

            match rows[0].get("id").unwrap() {
                DatabaseValue::Int32(n) => n,
                DatabaseValue::Int64(n) => n as i32,
                _ => panic!("Expected number for id"),
            }
        }

        async fn insert_with_now(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (created_at, description) VALUES ($1, $2)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    DatabaseValue::Now,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with NOW()");
        }

        async fn insert_with_expires_at(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            expires_at: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (expires_at, description) VALUES ($1, $2)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[expires_at, DatabaseValue::String(description.to_string())],
            )
            .await
            .expect("Failed to insert with expires_at");
        }

        async fn insert_with_scheduled_for(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            scheduled_for: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (scheduled_for, description) VALUES ($1, $2)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    scheduled_for,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with scheduled_for");
        }

        async fn insert_with_all_timestamps(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            created_at: DatabaseValue,
            expires_at: DatabaseValue,
            scheduled_for: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (created_at, expires_at, scheduled_for, description) VALUES ($1, $2, $3, $4)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    created_at,
                    expires_at,
                    scheduled_for,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with all timestamps");
        }

        fn gen_param(&self, i: u8) -> String {
            format!("${i}")
        }
    }

    // Test implementations for PostgreSQL SQLX
    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_now_insert() {
        let suite = PostgresSqlxDateTimeTests;
        suite.test_now_insert("postgres_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_now_in_where_clause() {
        let suite = PostgresSqlxDateTimeTests;
        suite.test_now_in_where_clause("postgres_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_now_plus_days() {
        let suite = PostgresSqlxDateTimeTests;
        suite.test_now_plus_days("postgres_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_now_minus_days() {
        let suite = PostgresSqlxDateTimeTests;
        suite.test_now_minus_days("postgres_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_now_plus_hours_minutes_seconds() {
        let suite = PostgresSqlxDateTimeTests;
        suite
            .test_now_plus_hours_minutes_seconds("postgres_sqlx")
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_now_plus_minutes_normalization() {
        let suite = PostgresSqlxDateTimeTests;
        suite
            .test_now_plus_minutes_normalization("postgres_sqlx")
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_now_plus_complex_interval() {
        let suite = PostgresSqlxDateTimeTests;
        suite.test_now_plus_complex_interval("postgres_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_zero_interval_returns_now() {
        let suite = PostgresSqlxDateTimeTests;
        suite.test_zero_interval_returns_now("postgres_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_mixed_parameters() {
        let suite = PostgresSqlxDateTimeTests;
        suite.test_mixed_parameters("postgres_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_now_consistency_in_transaction() {
        let suite = PostgresSqlxDateTimeTests;
        suite
            .test_now_consistency_in_transaction("postgres_sqlx")
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_duration_conversion() {
        let suite = PostgresSqlxDateTimeTests;
        suite.test_duration_conversion("postgres_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_now_plus_interval() {
        let suite = PostgresSqlxDateTimeTests;
        suite.test_now_plus_interval("postgres_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_now_minus_interval() {
        let suite = PostgresSqlxDateTimeTests;
        suite.test_now_minus_interval("postgres_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_complex_interval_operations() {
        let suite = PostgresSqlxDateTimeTests;
        suite
            .test_complex_interval_operations("postgres_sqlx")
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_update_with_now() {
        let suite = PostgresSqlxDateTimeTests;
        suite.test_update_with_now("postgres_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_multiple_now_consistency() {
        let suite = PostgresSqlxDateTimeTests;
        suite.test_multiple_now_consistency("postgres_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_mixed_now_operations() {
        let suite = PostgresSqlxDateTimeTests;
        suite.test_mixed_now_operations("postgres_sqlx").await;
    }
}

// ===== MYSQL SQLX BACKEND TESTS =====
#[cfg(feature = "mysql-sqlx")]
mod mysql_sqlx_datetime_tests {
    use super::*;
    use moosicbox_json_utils::database::ToValue as _;
    use sqlx::MySqlPool;
    use std::sync::Arc;
    use switchy_async::sync::Mutex;
    use switchy_database::sqlx::mysql::MySqlSqlxDatabase;

    struct MySqlSqlxDateTimeTests;

    impl DateTimeTestSuite<&'static str> for MySqlSqlxDateTimeTests {
        type DatabaseType = MySqlSqlxDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            let url = std::env::var("MYSQL_TEST_URL").ok()?;
            let pool = MySqlPool::connect(&url).await.ok()?;
            let pool = Arc::new(Mutex::new(pool));
            Some(Arc::new(MySqlSqlxDatabase::new(pool)))
        }

        async fn create_test_table(&self, db: &Self::DatabaseType, table_name: &str) {
            let query = format!(
                r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id INT AUTO_INCREMENT PRIMARY KEY,
                    created_at TIMESTAMP NULL,
                    expires_at TIMESTAMP NULL,
                    scheduled_for TIMESTAMP NULL,
                    description TEXT
                )
                "#,
                table_name
            );
            db.exec_raw(&query)
                .await
                .expect("Failed to create datetime test table");
        }

        async fn cleanup_test_data(&self, db: &Self::DatabaseType, table_name: &str) {
            let query = format!("DROP TABLE IF EXISTS {}", table_name);
            db.exec_raw(&query)
                .await
                .expect("Failed to drop test table");
        }

        async fn get_timestamp_column(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            column: &str,
            id: i32,
        ) -> Option<NaiveDateTime> {
            let query = format!("SELECT {} FROM {} WHERE id = ?", column, table_name);
            let rows = db
                .query_raw_params(&query, &[DatabaseValue::Int64(id as i64)])
                .await
                .unwrap();

            if let Some(row) = rows.first() {
                return Some(row.to_value(column).unwrap());
            }
            None
        }

        async fn get_row_id_by_description(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            description: &str,
        ) -> i32 {
            let query = format!(
                "SELECT id FROM {} WHERE description = ? ORDER BY id LIMIT 1",
                table_name
            );
            let rows = db
                .query_raw_params(&query, &[DatabaseValue::String(description.to_string())])
                .await
                .expect("Failed to get row by description");

            match rows[0].get("id").unwrap() {
                DatabaseValue::Int32(n) => n,
                DatabaseValue::Int64(n) => n as i32,
                _ => panic!("Expected number for id"),
            }
        }

        async fn insert_with_now(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (created_at, description) VALUES (?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    DatabaseValue::Now,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with NOW()");
        }

        async fn insert_with_expires_at(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            expires_at: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (expires_at, description) VALUES (?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[expires_at, DatabaseValue::String(description.to_string())],
            )
            .await
            .expect("Failed to insert with expires_at");
        }

        async fn insert_with_scheduled_for(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            scheduled_for: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (scheduled_for, description) VALUES (?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    scheduled_for,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with scheduled_for");
        }

        async fn insert_with_all_timestamps(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            created_at: DatabaseValue,
            expires_at: DatabaseValue,
            scheduled_for: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (created_at, expires_at, scheduled_for, description) VALUES (?, ?, ?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    created_at,
                    expires_at,
                    scheduled_for,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with all timestamps");
        }

        fn gen_param(&self, _i: u8) -> &'static str {
            "?"
        }
    }

    // Test implementations for MySQL SQLX
    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_now_insert() {
        let suite = MySqlSqlxDateTimeTests;
        suite.test_now_insert("mysql_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_now_in_where_clause() {
        let suite = MySqlSqlxDateTimeTests;
        suite.test_now_in_where_clause("mysql_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_now_plus_days() {
        let suite = MySqlSqlxDateTimeTests;
        suite.test_now_plus_days("mysql_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_now_minus_days() {
        let suite = MySqlSqlxDateTimeTests;
        suite.test_now_minus_days("mysql_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_now_plus_hours_minutes_seconds() {
        let suite = MySqlSqlxDateTimeTests;
        suite
            .test_now_plus_hours_minutes_seconds("mysql_sqlx")
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_now_plus_minutes_normalization() {
        let suite = MySqlSqlxDateTimeTests;
        suite
            .test_now_plus_minutes_normalization("mysql_sqlx")
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_now_plus_complex_interval() {
        let suite = MySqlSqlxDateTimeTests;
        suite.test_now_plus_complex_interval("mysql_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_zero_interval_returns_now() {
        let suite = MySqlSqlxDateTimeTests;
        suite.test_zero_interval_returns_now("mysql_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_mixed_parameters() {
        let suite = MySqlSqlxDateTimeTests;
        suite.test_mixed_parameters("mysql_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_now_consistency_in_transaction() {
        let suite = MySqlSqlxDateTimeTests;
        suite
            .test_now_consistency_in_transaction("mysql_sqlx")
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_duration_conversion() {
        let suite = MySqlSqlxDateTimeTests;
        suite.test_duration_conversion("mysql_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_now_plus_interval() {
        let suite = MySqlSqlxDateTimeTests;
        suite.test_now_plus_interval("mysql_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_now_minus_interval() {
        let suite = MySqlSqlxDateTimeTests;
        suite.test_now_minus_interval("mysql_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_complex_interval_operations() {
        let suite = MySqlSqlxDateTimeTests;
        suite.test_complex_interval_operations("mysql_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_update_with_now() {
        let suite = MySqlSqlxDateTimeTests;
        suite.test_update_with_now("mysql_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_multiple_now_consistency() {
        let suite = MySqlSqlxDateTimeTests;
        suite.test_multiple_now_consistency("mysql_sqlx").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_mixed_now_operations() {
        let suite = MySqlSqlxDateTimeTests;
        suite.test_mixed_now_operations("mysql_sqlx").await;
    }
}

// ===== POSTGRES RAW BACKEND TESTS =====
#[cfg(feature = "postgres-raw")]
mod postgres_raw_datetime_tests {
    use super::*;
    use moosicbox_json_utils::database::ToValue as _;
    use switchy_database::postgres::postgres::PostgresDatabase;

    struct PostgresRawDateTimeTests;

    impl DateTimeTestSuite<String> for PostgresRawDateTimeTests {
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

        async fn create_test_table(&self, db: &Self::DatabaseType, table_name: &str) {
            let query = format!(
                r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id BIGSERIAL PRIMARY KEY,
                    created_at TIMESTAMP,
                    expires_at TIMESTAMP,
                    scheduled_for TIMESTAMP,
                    description TEXT
                )
                "#,
                table_name
            );
            db.exec_raw(&query)
                .await
                .expect("Failed to create datetime test table");
        }

        async fn cleanup_test_data(&self, db: &Self::DatabaseType, table_name: &str) {
            let query = format!("DROP TABLE IF EXISTS {}", table_name);
            db.exec_raw(&query)
                .await
                .expect("Failed to drop test table");
        }

        async fn get_timestamp_column(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            column: &str,
            id: i32,
        ) -> Option<NaiveDateTime> {
            let query = format!("SELECT {} FROM {} WHERE id = $1", column, table_name);
            let rows = db
                .query_raw_params(&query, &[DatabaseValue::Int64(id as i64)])
                .await
                .unwrap();

            if let Some(row) = rows.first() {
                return Some(row.to_value(column).unwrap());
            }
            None
        }

        async fn get_row_id_by_description(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            description: &str,
        ) -> i32 {
            let query = format!(
                "SELECT id FROM {} WHERE description = $1 ORDER BY id LIMIT 1",
                table_name
            );
            let rows = db
                .query_raw_params(&query, &[DatabaseValue::String(description.to_string())])
                .await
                .expect("Failed to get row by description");

            match rows[0].get("id").unwrap() {
                DatabaseValue::Int32(n) => n,
                DatabaseValue::Int64(n) => n as i32,
                _ => panic!("Expected number for id"),
            }
        }

        async fn insert_with_now(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (created_at, description) VALUES ($1, $2)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    DatabaseValue::Now,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with NOW()");
        }

        async fn insert_with_expires_at(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            expires_at: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (expires_at, description) VALUES ($1, $2)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[expires_at, DatabaseValue::String(description.to_string())],
            )
            .await
            .expect("Failed to insert with expires_at");
        }

        async fn insert_with_scheduled_for(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            scheduled_for: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (scheduled_for, description) VALUES ($1, $2)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    scheduled_for,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with scheduled_for");
        }

        async fn insert_with_all_timestamps(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            created_at: DatabaseValue,
            expires_at: DatabaseValue,
            scheduled_for: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (created_at, expires_at, scheduled_for, description) VALUES ($1, $2, $3, $4)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    created_at,
                    expires_at,
                    scheduled_for,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with all timestamps");
        }

        fn gen_param(&self, i: u8) -> String {
            format!("${i}")
        }
    }

    // Test implementations for PostgreSQL Raw
    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_now_insert() {
        let suite = PostgresRawDateTimeTests;
        suite.test_now_insert("postgres_raw").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_now_in_where_clause() {
        let suite = PostgresRawDateTimeTests;
        suite.test_now_in_where_clause("postgres_raw").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_now_plus_days() {
        let suite = PostgresRawDateTimeTests;
        suite.test_now_plus_days("postgres_raw").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_now_minus_days() {
        let suite = PostgresRawDateTimeTests;
        suite.test_now_minus_days("postgres_raw").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_now_plus_hours_minutes_seconds() {
        let suite = PostgresRawDateTimeTests;
        suite
            .test_now_plus_hours_minutes_seconds("postgres_raw")
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_now_plus_minutes_normalization() {
        let suite = PostgresRawDateTimeTests;
        suite
            .test_now_plus_minutes_normalization("postgres_raw")
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_now_plus_complex_interval() {
        let suite = PostgresRawDateTimeTests;
        suite.test_now_plus_complex_interval("postgres_raw").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_zero_interval_returns_now() {
        let suite = PostgresRawDateTimeTests;
        suite.test_zero_interval_returns_now("postgres_raw").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_mixed_parameters() {
        let suite = PostgresRawDateTimeTests;
        suite.test_mixed_parameters("postgres_raw").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_now_consistency_in_transaction() {
        let suite = PostgresRawDateTimeTests;
        suite
            .test_now_consistency_in_transaction("postgres_raw")
            .await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_duration_conversion() {
        let suite = PostgresRawDateTimeTests;
        suite.test_duration_conversion("postgres_raw").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_now_plus_interval() {
        let suite = PostgresRawDateTimeTests;
        suite.test_now_plus_interval("postgres_raw").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_now_minus_interval() {
        let suite = PostgresRawDateTimeTests;
        suite.test_now_minus_interval("postgres_raw").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_complex_interval_operations() {
        let suite = PostgresRawDateTimeTests;
        suite.test_complex_interval_operations("postgres_raw").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_update_with_now() {
        let suite = PostgresRawDateTimeTests;
        suite.test_update_with_now("postgres_raw").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_multiple_now_consistency() {
        let suite = PostgresRawDateTimeTests;
        suite.test_multiple_now_consistency("postgres_raw").await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_raw_mixed_now_operations() {
        let suite = PostgresRawDateTimeTests;
        suite.test_mixed_now_operations("postgres_raw").await;
    }
}

#[cfg(feature = "turso")]
mod turso_datetime_tests {
    use super::*;
    use moosicbox_json_utils::database::ToValue as _;
    use switchy_database::turso::TursoDatabase;

    struct TursoDateTimeTests;

    impl DateTimeTestSuite<&'static str> for TursoDateTimeTests {
        type DatabaseType = TursoDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            TursoDatabase::new(":memory:").await.ok().map(Arc::new)
        }

        async fn create_test_table(&self, db: &Self::DatabaseType, table_name: &str) {
            let query = format!(
                r#"
                CREATE TABLE IF NOT EXISTS {} (
                    id INTEGER PRIMARY KEY,
                    created_at TIMESTAMP,
                    expires_at TIMESTAMP,
                    scheduled_for TIMESTAMP,
                    description TEXT
                )
                "#,
                table_name
            );
            db.exec_raw(&query)
                .await
                .expect("Failed to create datetime test table");
        }

        async fn cleanup_test_data(&self, db: &Self::DatabaseType, table_name: &str) {
            let query = format!("DROP TABLE IF EXISTS {}", table_name);
            db.exec_raw(&query)
                .await
                .expect("Failed to drop test table");
        }

        async fn get_timestamp_column(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            column: &str,
            id: i32,
        ) -> Option<NaiveDateTime> {
            let query = format!("SELECT {} FROM {} WHERE id = ?", column, table_name);
            let rows = db
                .query_raw_params(&query, &[DatabaseValue::Int64(id as i64)])
                .await
                .unwrap();

            if let Some(row) = rows.first() {
                return Some(row.to_value(column).unwrap());
            }
            None
        }

        async fn get_row_id_by_description(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            description: &str,
        ) -> i32 {
            let query = format!(
                "SELECT id FROM {} WHERE description = ? ORDER BY id LIMIT 1",
                table_name
            );
            let rows = db
                .query_raw_params(&query, &[DatabaseValue::String(description.to_string())])
                .await
                .expect("Failed to get row by description");

            if rows.is_empty() {
                panic!("No row found with description '{}'", description);
            }

            match rows[0].get("id").unwrap() {
                DatabaseValue::Int32(n) => n,
                DatabaseValue::Int64(n) => n as i32,
                _ => panic!("Expected number for id"),
            }
        }

        async fn insert_with_now(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (created_at, description) VALUES (?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    DatabaseValue::Now,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with NOW()");
        }

        async fn insert_with_expires_at(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            expires_at: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (expires_at, description) VALUES (?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[expires_at, DatabaseValue::String(description.to_string())],
            )
            .await
            .expect("Failed to insert with expires_at");
        }

        async fn insert_with_scheduled_for(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            scheduled_for: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (scheduled_for, description) VALUES (?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    scheduled_for,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with scheduled_for");
        }

        async fn insert_with_all_timestamps(
            &self,
            db: &Self::DatabaseType,
            table_name: &str,
            created_at: DatabaseValue,
            expires_at: DatabaseValue,
            scheduled_for: DatabaseValue,
            description: &str,
        ) {
            let query = format!(
                "INSERT INTO {} (created_at, expires_at, scheduled_for, description) VALUES (?, ?, ?, ?)",
                table_name
            );
            db.exec_raw_params(
                &query,
                &[
                    created_at,
                    expires_at,
                    scheduled_for,
                    DatabaseValue::String(description.to_string()),
                ],
            )
            .await
            .expect("Failed to insert with all timestamps");
        }

        fn gen_param(&self, _i: u8) -> &'static str {
            "?"
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_now_insert() {
        let suite = TursoDateTimeTests;
        suite.test_now_insert("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_now_in_where_clause() {
        let suite = TursoDateTimeTests;
        suite.test_now_in_where_clause("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_now_plus_days() {
        let suite = TursoDateTimeTests;
        suite.test_now_plus_days("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_now_minus_days() {
        let suite = TursoDateTimeTests;
        suite.test_now_minus_days("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_now_plus_hours_minutes_seconds() {
        let suite = TursoDateTimeTests;
        suite.test_now_plus_hours_minutes_seconds("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_now_plus_minutes_normalization() {
        let suite = TursoDateTimeTests;
        suite.test_now_plus_minutes_normalization("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_now_plus_complex_interval() {
        let suite = TursoDateTimeTests;
        suite.test_now_plus_complex_interval("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_zero_interval_returns_now() {
        let suite = TursoDateTimeTests;
        suite.test_zero_interval_returns_now("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_mixed_parameters() {
        let suite = TursoDateTimeTests;
        suite.test_mixed_parameters("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_now_consistency_in_transaction() {
        let suite = TursoDateTimeTests;
        suite.test_now_consistency_in_transaction("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_duration_conversion() {
        let suite = TursoDateTimeTests;
        suite.test_duration_conversion("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_now_plus_interval() {
        let suite = TursoDateTimeTests;
        suite.test_now_plus_interval("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_now_minus_interval() {
        let suite = TursoDateTimeTests;
        suite.test_now_minus_interval("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_complex_interval_operations() {
        let suite = TursoDateTimeTests;
        suite.test_complex_interval_operations("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_update_with_now() {
        let suite = TursoDateTimeTests;
        suite.test_update_with_now("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_multiple_now_consistency() {
        let suite = TursoDateTimeTests;
        suite.test_multiple_now_consistency("turso").await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_mixed_now_operations() {
        let suite = TursoDateTimeTests;
        suite.test_mixed_now_operations("turso").await;
    }
}
