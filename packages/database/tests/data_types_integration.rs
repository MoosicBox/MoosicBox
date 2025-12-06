#![cfg(feature = "schema")]

mod common;

use common::data_types_tests::DataTypeTestSuite;
use std::sync::Arc;

// ===== RUSQLITE BACKEND TESTS =====
#[cfg(feature = "sqlite-rusqlite")]
mod rusqlite_data_type_tests {
    use super::*;
    use rusqlite::Connection;
    use switchy_async::sync::Mutex;
    use switchy_database::rusqlite::RusqliteDatabase;

    struct RusqliteDataTypeTests;

    impl DataTypeTestSuite for RusqliteDataTypeTests {
        type DatabaseType = RusqliteDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            let test_id = std::thread::current().id();
            let timestamp = switchy_time::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let db_url = format!(
                "file:datatypes_{test_id:?}_{timestamp}:?mode=memory&cache=shared&uri=true"
            );

            let conn = Connection::open(&db_url).expect("Failed to create shared memory database");

            conn.pragma_update(None, "journal_mode", "WAL")
                .expect("Failed to set WAL mode");
            conn.pragma_update(None, "busy_timeout", 5000)
                .expect("Failed to set busy timeout");

            Some(Arc::new(RusqliteDatabase::new(vec![Arc::new(Mutex::new(
                conn,
            ))])))
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_int_vs_bigint_type_safety() {
        let suite = RusqliteDataTypeTests;
        suite.test_int_vs_bigint_type_safety().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_string_types_varchar_text_char() {
        let suite = RusqliteDataTypeTests;
        suite.test_string_types_varchar_text_char().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_floating_point_types() {
        let suite = RusqliteDataTypeTests;
        suite.test_floating_point_types().await;
    }

    #[cfg(feature = "decimal")]
    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_decimal_precision() {
        let suite = RusqliteDataTypeTests;
        suite.test_decimal_precision().await;
    }

    #[cfg(feature = "uuid")]
    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_uuid_storage() {
        let suite = RusqliteDataTypeTests;
        suite.test_uuid_storage().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_boolean_type() {
        let suite = RusqliteDataTypeTests;
        suite.test_boolean_type().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_datetime_types() {
        let suite = RusqliteDataTypeTests;
        suite.test_datetime_types().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_null_handling_all_types() {
        let suite = RusqliteDataTypeTests;
        suite.test_null_handling_all_types().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_serial_auto_increment() {
        let suite = RusqliteDataTypeTests;
        suite.test_serial_auto_increment().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_default_values_all_types() {
        let suite = RusqliteDataTypeTests;
        suite.test_default_values_all_types().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_int8_specific_type_and_retrieval() {
        let suite = RusqliteDataTypeTests;
        suite.test_int8_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_rusqlite_int16_specific_type_and_retrieval() {
        let suite = RusqliteDataTypeTests;
        suite.test_int16_specific_type_and_retrieval().await;
    }
}

// ===== SQLITE SQLX BACKEND TESTS =====
#[cfg(feature = "sqlite-sqlx")]
mod sqlite_sqlx_data_type_tests {
    use super::*;
    use std::sync::Arc;
    use switchy_async::sync::Mutex;
    use switchy_database::sqlx::sqlite::SqliteSqlxDatabase;

    struct SqliteSqlxDataTypeTests;

    impl DataTypeTestSuite for SqliteSqlxDataTypeTests {
        type DatabaseType = SqliteSqlxDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            use sqlx::sqlite::SqlitePoolOptions;

            let database_url = "sqlite::memory:?cache=shared";
            let pool = SqlitePoolOptions::new()
                .max_connections(5)
                .min_connections(2)
                .connect(database_url)
                .await
                .ok()?;

            sqlx::query("PRAGMA journal_mode = WAL")
                .execute(&pool)
                .await
                .expect("Failed to set WAL mode");
            sqlx::query("PRAGMA busy_timeout = 5000")
                .execute(&pool)
                .await
                .expect("Failed to set busy timeout");

            let pool = Arc::new(Mutex::new(pool));
            Some(Arc::new(SqliteSqlxDatabase::new(pool)))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_integer_types_boundary_values() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_integer_types_boundary_values().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_int_vs_bigint_type_safety() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_int_vs_bigint_type_safety().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_string_types_varchar_text_char() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_string_types_varchar_text_char().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_floating_point_types() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_floating_point_types().await;
    }

    #[cfg(feature = "decimal")]
    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_decimal_precision() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_decimal_precision().await;
    }

    #[cfg(feature = "uuid")]
    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_uuid_storage() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_uuid_storage().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_boolean_type() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_boolean_type().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_datetime_types() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_datetime_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_null_handling_all_types() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_null_handling_all_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_serial_auto_increment() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_serial_auto_increment().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_default_values_all_types() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_default_values_all_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_int8_specific_type_and_retrieval() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_int8_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_uint8_specific_type_and_retrieval() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_uint8_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_uint16_specific_type_and_retrieval() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_uint16_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_uint32_specific_type_and_retrieval() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_uint32_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_sqlite_sqlx_int16_specific_type_and_retrieval() {
        let suite = SqliteSqlxDataTypeTests;
        suite.test_int16_specific_type_and_retrieval().await;
    }
}

#[cfg(feature = "turso")]
mod turso_data_type_tests {
    use super::*;
    use switchy_database::turso::TursoDatabase;

    struct TursoDataTypeTests;

    impl DataTypeTestSuite for TursoDataTypeTests {
        type DatabaseType = TursoDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            TursoDatabase::new(":memory:").await.ok().map(Arc::new)
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_integer_types_boundary_values() {
        let suite = TursoDataTypeTests;
        suite.test_integer_types_boundary_values().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_int_vs_bigint_type_safety() {
        let suite = TursoDataTypeTests;
        suite.test_int_vs_bigint_type_safety().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_string_types_varchar_text_char() {
        let suite = TursoDataTypeTests;
        suite.test_string_types_varchar_text_char().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_floating_point_types() {
        let suite = TursoDataTypeTests;
        suite.test_floating_point_types().await;
    }

    #[cfg(feature = "decimal")]
    #[test_log::test(switchy_async::test)]
    async fn test_turso_decimal_precision() {
        let suite = TursoDataTypeTests;
        suite.test_decimal_precision().await;
    }

    #[cfg(feature = "uuid")]
    #[test_log::test(switchy_async::test)]
    async fn test_turso_uuid_storage() {
        let suite = TursoDataTypeTests;
        suite.test_uuid_storage().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_boolean_type() {
        let suite = TursoDataTypeTests;
        suite.test_boolean_type().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_datetime_types() {
        let suite = TursoDataTypeTests;
        suite.test_datetime_types().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_null_handling_all_types() {
        let suite = TursoDataTypeTests;
        suite.test_null_handling_all_types().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_serial_auto_increment() {
        let suite = TursoDataTypeTests;
        suite.test_serial_auto_increment().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_default_values_all_types() {
        let suite = TursoDataTypeTests;
        suite.test_default_values_all_types().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_int8_specific_type_and_retrieval() {
        let suite = TursoDataTypeTests;
        suite.test_int8_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test)]
    async fn test_turso_int16_specific_type_and_retrieval() {
        let suite = TursoDataTypeTests;
        suite.test_int16_specific_type_and_retrieval().await;
    }
}

// ===== POSTGRES RAW BACKEND TESTS =====
#[cfg(feature = "postgres-raw")]
mod postgres_data_type_tests {
    use super::*;
    use switchy_database::postgres::postgres::PostgresDatabase;

    struct PostgresDataTypeTests;

    impl DataTypeTestSuite for PostgresDataTypeTests {
        type DatabaseType = PostgresDatabase;

        fn get_table_name(&self, test_suffix: &str) -> String {
            format!("data_type_test_postgres_tokio_{}", test_suffix)
        }

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
    async fn test_postgres_integer_types_boundary_values() {
        let suite = PostgresDataTypeTests;
        suite.test_integer_types_boundary_values().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_int_vs_bigint_type_safety() {
        let suite = PostgresDataTypeTests;
        suite.test_int_vs_bigint_type_safety().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_string_types_varchar_text_char() {
        let suite = PostgresDataTypeTests;
        suite.test_string_types_varchar_text_char().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_floating_point_types() {
        let suite = PostgresDataTypeTests;
        suite.test_floating_point_types().await;
    }

    #[cfg(feature = "decimal")]
    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_decimal_precision() {
        let suite = PostgresDataTypeTests;
        suite.test_decimal_precision().await;
    }

    #[cfg(feature = "uuid")]
    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_uuid_storage() {
        let suite = PostgresDataTypeTests;
        suite.test_uuid_storage().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_boolean_type() {
        let suite = PostgresDataTypeTests;
        suite.test_boolean_type().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_null_handling_all_types() {
        let suite = PostgresDataTypeTests;
        suite.test_null_handling_all_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_serial_auto_increment() {
        let suite = PostgresDataTypeTests;
        suite.test_serial_auto_increment().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_default_values_all_types() {
        let suite = PostgresDataTypeTests;
        suite.test_default_values_all_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_int8_specific_type_and_retrieval() {
        let suite = PostgresDataTypeTests;
        suite.test_int8_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_uint8_specific_type_and_retrieval() {
        let suite = PostgresDataTypeTests;
        suite.test_uint8_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_uint16_specific_type_and_retrieval() {
        let suite = PostgresDataTypeTests;
        suite.test_uint16_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_uint32_specific_type_and_retrieval() {
        let suite = PostgresDataTypeTests;
        suite.test_uint32_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_int16_specific_type_and_retrieval() {
        let suite = PostgresDataTypeTests;
        suite.test_int16_specific_type_and_retrieval().await;
    }
}

// ===== POSTGRES SQLX BACKEND TESTS =====
#[cfg(feature = "postgres-sqlx")]
mod postgres_sqlx_data_type_tests {
    use super::*;
    use sqlx::PgPool;
    use std::sync::Arc;
    use switchy_async::sync::Mutex;
    use switchy_database::sqlx::postgres::PostgresSqlxDatabase;

    struct PostgresSqlxDataTypeTests;

    impl DataTypeTestSuite for PostgresSqlxDataTypeTests {
        type DatabaseType = PostgresSqlxDatabase;

        fn get_table_name(&self, test_suffix: &str) -> String {
            format!("data_type_test_postgres_sqlx_{test_suffix}")
        }

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            let url = std::env::var("POSTGRES_TEST_URL").ok()?;
            let pool = PgPool::connect(&url).await.ok()?;
            let pool = Arc::new(Mutex::new(pool));
            Some(Arc::new(PostgresSqlxDatabase::new(pool)))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_integer_types_boundary_values() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_integer_types_boundary_values().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_int_vs_bigint_type_safety() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_int_vs_bigint_type_safety().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_string_types_varchar_text_char() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_string_types_varchar_text_char().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_floating_point_types() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_floating_point_types().await;
    }

    #[cfg(feature = "decimal")]
    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_decimal_precision() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_decimal_precision().await;
    }

    #[cfg(feature = "uuid")]
    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_uuid_storage() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_uuid_storage().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_boolean_type() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_boolean_type().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_datetime_types() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_datetime_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_null_handling_all_types() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_null_handling_all_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_serial_auto_increment() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_serial_auto_increment().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_default_values_all_types() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_default_values_all_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_int8_specific_type_and_retrieval() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_int8_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_uint8_specific_type_and_retrieval() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_uint8_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_uint16_specific_type_and_retrieval() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_uint16_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_uint32_specific_type_and_retrieval() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_uint32_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_postgres_sqlx_int16_specific_type_and_retrieval() {
        let suite = PostgresSqlxDataTypeTests;
        suite.test_int16_specific_type_and_retrieval().await;
    }
}

// ===== MYSQL SQLX BACKEND TESTS =====
#[cfg(feature = "mysql-sqlx")]
mod mysql_sqlx_data_type_tests {
    use super::*;
    use sqlx::MySqlPool;
    use std::sync::Arc;
    use switchy_async::sync::Mutex;
    use switchy_database::sqlx::mysql::MySqlSqlxDatabase;

    struct MysqlSqlxDataTypeTests;

    impl DataTypeTestSuite for MysqlSqlxDataTypeTests {
        type DatabaseType = MySqlSqlxDatabase;

        async fn get_database(&self) -> Option<Arc<Self::DatabaseType>> {
            let url = std::env::var("MYSQL_TEST_URL").ok()?;
            let pool = MySqlPool::connect(&url).await.ok()?;
            let pool = Arc::new(Mutex::new(pool));
            Some(Arc::new(MySqlSqlxDatabase::new(pool)))
        }
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_integer_types_boundary_values() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_integer_types_boundary_values().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_int_vs_bigint_type_safety() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_int_vs_bigint_type_safety().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_string_types_varchar_text_char() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_string_types_varchar_text_char().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_floating_point_types() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_floating_point_types().await;
    }

    #[cfg(feature = "decimal")]
    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_decimal_precision() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_decimal_precision().await;
    }

    #[cfg(feature = "uuid")]
    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_uuid_storage() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_uuid_storage().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_boolean_type() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_boolean_type().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_datetime_types() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_datetime_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_null_handling_all_types() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_null_handling_all_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_serial_auto_increment() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_serial_auto_increment().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_default_values_all_types() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_default_values_all_types().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_int8_specific_type_and_retrieval() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_int8_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_uint8_specific_type_and_retrieval() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_uint8_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_uint16_specific_type_and_retrieval() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_uint16_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_uint32_specific_type_and_retrieval() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_uint32_specific_type_and_retrieval().await;
    }

    #[test_log::test(switchy_async::test(no_simulator))]
    async fn test_mysql_sqlx_int16_specific_type_and_retrieval() {
        let suite = MysqlSqlxDataTypeTests;
        suite.test_int16_specific_type_and_retrieval().await;
    }
}
