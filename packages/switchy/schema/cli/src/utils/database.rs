//! Database connection utilities.
//!
//! This module provides database connection functionality for the CLI, supporting
//! both `PostgreSQL` and `SQLite` databases based on URL scheme detection.

use crate::CliError;
use switchy_database::Database;

/// Connect to database based on URL scheme.
///
/// Parses the database URL and establishes a connection to either `PostgreSQL` or `SQLite`
/// based on the URL scheme. Supported schemes are `sqlite`, `postgresql`, and `postgres`.
///
/// # Errors
///
/// Returns an error if:
/// * The database URL format is invalid (missing scheme)
/// * The database scheme is unsupported (not `sqlite`/`postgresql`/`postgres`)
/// * `SQLite` connection fails (invalid path, permissions, etc.)
/// * `PostgreSQL` URL parsing fails (malformed URL)
/// * `PostgreSQL` connection fails (network, authentication, etc.)
pub async fn connect(database_url: &str) -> Result<Box<dyn Database>, CliError> {
    let scheme = database_url
        .split(':')
        .next()
        .ok_or_else(|| CliError::Config("Invalid database URL format".to_string()))?;

    match scheme {
        "sqlite" => connect_sqlite(database_url).await,
        "postgresql" | "postgres" => connect_postgres(database_url).await,
        _ => Err(CliError::Config(format!(
            "Unsupported database scheme: {scheme}. Supported: sqlite, postgresql"
        ))),
    }
}

async fn connect_sqlite(database_url: &str) -> Result<Box<dyn Database>, CliError> {
    use std::path::Path;

    let path = if database_url == "sqlite://:memory:" || database_url == "sqlite:" {
        None
    } else {
        let path_part = database_url
            .strip_prefix("sqlite://")
            .unwrap_or(database_url);
        let path_part = path_part.strip_prefix("sqlite:").unwrap_or(path_part);
        Some(Path::new(path_part))
    };

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

#[cfg(test)]
mod tests {
    use super::*;

    #[switchy_async::test]
    async fn test_invalid_database_scheme() {
        let result = connect("mysql://localhost/test").await;

        match result {
            Err(CliError::Config(msg)) => {
                assert!(msg.contains("Unsupported database scheme: mysql"));
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

    #[switchy_async::test]
    async fn test_sqlite_memory_database() {
        // Test sqlite://:memory: format
        let result = connect("sqlite://:memory:").await;
        assert!(
            result.is_ok(),
            "Should connect to in-memory SQLite database"
        );

        // Test sqlite: format (also in-memory)
        let result2 = connect("sqlite:").await;
        assert!(
            result2.is_ok(),
            "Should connect to in-memory SQLite database with 'sqlite:'"
        );
    }

    #[switchy_async::test]
    async fn test_sqlite_url_parsing_variations() {
        // Test different SQLite URL formats
        let urls = vec!["sqlite://test.db", "sqlite:test.db", "sqlite://./test.db"];

        for url in urls {
            let result = connect(url).await;
            // We're not checking success here since file creation might fail,
            // but we should get a proper SQLite connection attempt, not a scheme error
            match result {
                Err(CliError::Config(msg)) if msg.contains("Unsupported database scheme") => {
                    panic!("Should recognize SQLite scheme for URL: {url}");
                }
                Ok(_) | Err(_) => {
                    // Success is fine, other errors (file system, permissions) are also acceptable
                }
            }
        }
    }

    #[switchy_async::test]
    async fn test_postgres_url_parsing_error() {
        // Test with malformed PostgreSQL URL
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
    async fn test_missing_url_scheme() {
        // Test URL without proper scheme separator
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
        // Test scheme extraction logic
        let test_cases = vec![
            ("sqlite://test.db", "sqlite"),
            ("postgresql://localhost/db", "postgresql"),
            ("postgres://localhost/db", "postgres"),
            ("mysql://localhost/db", "mysql"),
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
    async fn test_sqlite_with_empty_path() {
        // Test that sqlite:// (empty path after //) is treated as in-memory
        let result = connect("sqlite://").await;
        // This should not error with scheme issues - it should either succeed
        // or fail with a SQLite-specific error
        if let Err(CliError::Config(msg)) = result {
            assert!(
                !msg.contains("Unsupported database scheme"),
                "Should not fail with scheme error for 'sqlite://', got: {msg}"
            );
        }
        // Otherwise success or other errors are acceptable
    }

    #[switchy_async::test]
    async fn test_case_sensitive_scheme_handling() {
        // Test that uppercase schemes are not supported (current behavior)
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
}
