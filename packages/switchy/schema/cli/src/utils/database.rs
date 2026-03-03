//! Database connection utilities.
//!
//! This module provides database connection functionality for the CLI, supporting
//! `PostgreSQL`, `SQLite`, `DuckDB`, `MySQL`, and local `Turso` databases based
//! on URL scheme detection.

use crate::CliError;
use switchy_database::Database;

/// Connect to database based on URL scheme.
///
/// Parses the database URL and establishes a connection based on the URL
/// scheme. Supported schemes are `sqlite`, `postgresql`, `postgres`, `duckdb`,
/// `mysql`, and `turso`.
///
/// # Errors
///
/// Returns an error if:
/// * The database URL format is invalid (missing scheme)
/// * The database scheme is unsupported
/// * `SQLite` connection fails (invalid path, permissions, etc.)
/// * `PostgreSQL` URL parsing fails (malformed URL)
/// * `PostgreSQL` connection fails (network, authentication, etc.)
/// * `DuckDB` connection fails (invalid path, permissions, etc.)
/// * `MySQL` URL parsing fails (malformed URL)
/// * `MySQL` connection fails (network, authentication, etc.)
/// * Local `Turso` connection fails (invalid path, permissions, etc.)
pub async fn connect(database_url: &str) -> Result<Box<dyn Database>, CliError> {
    let scheme = database_url
        .split(':')
        .next()
        .ok_or_else(|| CliError::Config("Invalid database URL format".to_string()))?;

    match scheme {
        "sqlite" => connect_sqlite(database_url).await,
        "postgresql" | "postgres" => connect_postgres(database_url).await,
        "duckdb" => connect_duckdb(database_url),
        "mysql" => connect_mysql(database_url).await,
        "turso" => connect_turso(database_url).await,
        _ => Err(CliError::Config(format!(
            "Unsupported database scheme: {scheme}. Supported: sqlite, postgresql, postgres, duckdb, mysql, turso"
        ))),
    }
}

fn path_from_url<'a>(database_url: &'a str, scheme: &str) -> Option<&'a std::path::Path> {
    let with_slashes = format!("{scheme}://");
    let without_slashes = format!("{scheme}:");

    let path_part = database_url
        .strip_prefix(&with_slashes)
        .unwrap_or(database_url);
    let path_part = path_part
        .strip_prefix(&without_slashes)
        .unwrap_or(path_part);

    if path_part.is_empty() || path_part == ":memory:" {
        None
    } else {
        Some(std::path::Path::new(path_part))
    }
}

async fn connect_sqlite(database_url: &str) -> Result<Box<dyn Database>, CliError> {
    let path = path_from_url(database_url, "sqlite");

    switchy_database_connection::init_sqlite_sqlx(path)
        .await
        .map_err(|e| CliError::Config(format!("SQLite connection error: {e}")))
}

async fn connect_postgres(database_url: &str) -> Result<Box<dyn Database>, CliError> {
    let creds = switchy_database_connection::Credentials::from_url(database_url)
        .map_err(|e| CliError::Config(format!("Failed to parse PostgreSQL URL: {e}")))?;

    switchy_database_connection::init_postgres_sqlx(creds)
        .await
        .map_err(|e| CliError::Config(format!("PostgreSQL connection error: {e}")))
}

fn connect_duckdb(database_url: &str) -> Result<Box<dyn Database>, CliError> {
    let path = path_from_url(database_url, "duckdb");

    switchy_database_connection::init_duckdb(path)
        .map_err(|e| CliError::Config(format!("DuckDB connection error: {e}")))
}

async fn connect_mysql(database_url: &str) -> Result<Box<dyn Database>, CliError> {
    let creds = switchy_database_connection::Credentials::from_url(database_url)
        .map_err(|e| CliError::Config(format!("Failed to parse MySQL URL: {e}")))?;

    switchy_database_connection::init_mysql_sqlx(creds)
        .await
        .map_err(|e| CliError::Config(format!("MySQL connection error: {e}")))
}

async fn connect_turso(database_url: &str) -> Result<Box<dyn Database>, CliError> {
    let path = path_from_url(database_url, "turso");

    switchy_database_connection::init_turso_local(path)
        .await
        .map_err(|e| CliError::Config(format!("Turso connection error: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[switchy_async::test]
    async fn test_invalid_database_scheme() {
        let result = connect("mongodb://localhost/test").await;

        match result {
            Err(CliError::Config(msg)) => {
                assert!(msg.contains("Unsupported database scheme: mongodb"));
            }
            _ => panic!("Expected Config error for unsupported scheme"),
        }
    }

    #[switchy_async::test]
    async fn test_invalid_database_url_format() {
        let result = connect("invalid-url").await;

        match result {
            Err(CliError::Config(msg)) => {
                assert!(msg.contains("Unsupported database scheme"));
            }
            _ => panic!("Expected Config error for invalid URL format"),
        }
    }

    #[test_log::test]
    fn test_postgres_scheme_recognition() {
        let pg_schemes = vec![
            "postgresql://user@localhost/db",
            "postgres://user@localhost/db",
        ];

        for url in pg_schemes {
            let scheme = url.split(':').next().unwrap();
            assert!(matches!(scheme, "postgresql" | "postgres"));
        }
    }

    #[test_log::test]
    fn test_additional_scheme_recognition() {
        let schemes = vec![
            ("duckdb://test.db", "duckdb"),
            ("duckdb:test.db", "duckdb"),
            ("mysql://user@localhost/db", "mysql"),
            ("turso://test.db", "turso"),
            ("turso:test.db", "turso"),
        ];

        for (url, expected_scheme) in schemes {
            let scheme = url.split(':').next().unwrap();
            assert_eq!(scheme, expected_scheme);
        }
    }

    #[switchy_async::test]
    async fn test_sqlite_memory_database() {
        let result = connect("sqlite://:memory:").await;
        assert!(
            result.is_ok(),
            "Should connect to in-memory SQLite database"
        );

        let result2 = connect("sqlite:").await;
        assert!(
            result2.is_ok(),
            "Should connect to in-memory SQLite database with 'sqlite:'"
        );

        let result3 = connect("sqlite://").await;
        assert!(
            result3.is_ok(),
            "Should connect to in-memory SQLite database with 'sqlite://'"
        );
    }

    #[switchy_async::test]
    async fn test_duckdb_memory_database() {
        let result = connect("duckdb://:memory:").await;
        assert!(
            result.is_ok(),
            "Should connect to in-memory DuckDB database"
        );

        let result2 = connect("duckdb:").await;
        assert!(
            result2.is_ok(),
            "Should connect to in-memory DuckDB database with 'duckdb:'"
        );

        let result3 = connect("duckdb://").await;
        assert!(
            result3.is_ok(),
            "Should connect to in-memory DuckDB database with 'duckdb://'"
        );
    }

    #[switchy_async::test]
    async fn test_turso_memory_database() {
        let result = connect("turso://:memory:").await;
        assert!(result.is_ok(), "Should connect to in-memory Turso database");

        let result2 = connect("turso:").await;
        assert!(
            result2.is_ok(),
            "Should connect to in-memory Turso database with 'turso:'"
        );

        let result3 = connect("turso://").await;
        assert!(
            result3.is_ok(),
            "Should connect to in-memory Turso database with 'turso://'"
        );
    }

    #[switchy_async::test]
    async fn test_sqlite_url_parsing_variations() {
        let urls = vec!["sqlite://test.db", "sqlite:test.db", "sqlite://./test.db"];

        for url in urls {
            let result = connect(url).await;
            match result {
                Err(CliError::Config(msg)) if msg.contains("Unsupported database scheme") => {
                    panic!("Should recognize SQLite scheme for URL: {url}");
                }
                Ok(_) | Err(_) => {}
            }
        }
    }

    #[switchy_async::test]
    async fn test_duckdb_url_parsing_variations() {
        let urls = vec![
            "duckdb://test.duckdb",
            "duckdb:test.duckdb",
            "duckdb://./test.duckdb",
        ];

        for url in urls {
            let result = connect(url).await;
            match result {
                Err(CliError::Config(msg)) if msg.contains("Unsupported database scheme") => {
                    panic!("Should recognize DuckDB scheme for URL: {url}");
                }
                Ok(_) | Err(_) => {}
            }
        }
    }

    #[switchy_async::test]
    async fn test_turso_url_parsing_variations() {
        let urls = vec!["turso://test.db", "turso:test.db", "turso://./test.db"];

        for url in urls {
            let result = connect(url).await;
            match result {
                Err(CliError::Config(msg)) if msg.contains("Unsupported database scheme") => {
                    panic!("Should recognize Turso scheme for URL: {url}");
                }
                Ok(_) | Err(_) => {}
            }
        }
    }

    #[switchy_async::test]
    async fn test_postgres_url_parsing_error() {
        let result = connect("postgresql://").await;

        match result {
            Err(CliError::Config(msg)) => {
                assert!(
                    msg.contains("Failed to parse PostgreSQL URL")
                        || msg.contains("PostgreSQL connection error"),
                    "Should fail with PostgreSQL parsing or connection error, got: {msg}"
                );
            }
            _ => panic!("Expected Config error for malformed PostgreSQL URL"),
        }
    }

    #[switchy_async::test]
    async fn test_mysql_url_parsing_error() {
        let result = connect("mysql://").await;

        match result {
            Err(CliError::Config(msg)) => {
                assert!(
                    msg.contains("Failed to parse MySQL URL")
                        || msg.contains("MySQL connection error"),
                    "Should fail with MySQL parsing or connection error, got: {msg}"
                );
            }
            _ => panic!("Expected Config error for malformed MySQL URL"),
        }
    }

    #[switchy_async::test]
    async fn test_missing_url_scheme() {
        let result = connect("notascheme").await;

        match result {
            Err(CliError::Config(msg)) => {
                assert!(
                    msg.contains("Unsupported database scheme"),
                    "Should fail with unsupported scheme error, got: {msg}"
                );
            }
            _ => panic!("Expected Config error for URL without scheme"),
        }
    }

    #[test_log::test]
    fn test_scheme_extraction() {
        let test_cases = vec![
            ("sqlite://test.db", "sqlite"),
            ("postgresql://localhost/db", "postgresql"),
            ("postgres://localhost/db", "postgres"),
            ("duckdb://test.duckdb", "duckdb"),
            ("duckdb:test.duckdb", "duckdb"),
            ("mysql://localhost/db", "mysql"),
            ("turso://test.db", "turso"),
            ("turso:test.db", "turso"),
            ("mongodb://localhost/db", "mongodb"),
        ];

        for (url, expected_scheme) in test_cases {
            let scheme = url.split(':').next().unwrap();
            assert_eq!(
                scheme, expected_scheme,
                "Scheme extraction failed for {url}"
            );
        }
    }

    #[switchy_async::test]
    async fn test_case_sensitive_scheme_handling() {
        let result = connect("SQLITE://test.db").await;

        match result {
            Err(CliError::Config(msg)) => {
                assert!(
                    msg.contains("Unsupported database scheme"),
                    "Should reject uppercase scheme"
                );
            }
            _ => panic!("Expected error for uppercase scheme"),
        }
    }

    #[test_log::test]
    fn test_path_from_url_memory_behavior() {
        assert!(path_from_url("sqlite://:memory:", "sqlite").is_none());
        assert!(path_from_url("sqlite:", "sqlite").is_none());
        assert!(path_from_url("sqlite://", "sqlite").is_none());

        assert!(path_from_url("duckdb://:memory:", "duckdb").is_none());
        assert!(path_from_url("duckdb:", "duckdb").is_none());
        assert!(path_from_url("duckdb://", "duckdb").is_none());

        assert!(path_from_url("turso://:memory:", "turso").is_none());
        assert!(path_from_url("turso:", "turso").is_none());
        assert!(path_from_url("turso://", "turso").is_none());
    }
}
