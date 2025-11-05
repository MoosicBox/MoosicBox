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

    #[test]
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
}
