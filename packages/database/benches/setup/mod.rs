//! Backend setup utilities for benchmarks
//!
//! This module provides functions to initialize database backends for benchmarking.
//! Each backend is conditionally compiled based on feature flags.
//!
//! Uses `switchy_database_connection` for consistent initialization with the rest
//! of the codebase.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use switchy_database::Database;

/// Type alias for the database
/// We use `Box<dyn Database>` wrapped in `Arc` for shared ownership across benchmarks.
pub type Db = Arc<Box<dyn Database>>;

/// Helper function to get a reference to the inner `dyn Database` from `Db`.
#[must_use]
pub fn db_ref(db: &Db) -> &dyn Database {
    db.as_ref().as_ref()
}

/// Information about an available database backend
pub struct BackendInfo {
    /// Human-readable name for the backend
    pub name: &'static str,
    /// The database instance
    pub db: Db,
}

/// Counter for generating unique table names
static TABLE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique table name to avoid conflicts between benchmark runs
#[must_use]
pub fn unique_table_name(prefix: &str) -> String {
    let timestamp = switchy_time::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis();
    let counter = TABLE_COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("{prefix}_{timestamp}_{counter}")
}

/// Initialize backends synchronously using the provided runtime.
///
/// This function uses the provided Tokio runtime for backend initialization,
/// which is necessary because sqlx pool creation requires a runtime context
/// and the pool must be created on the same runtime that will be used for queries.
///
/// The runtime is entered (via `rt.enter()`) before calling async initialization,
/// which is required for sqlx pool creation.
#[must_use]
pub fn init_backends(rt: &tokio::runtime::Runtime) -> Vec<BackendInfo> {
    let _guard = rt.enter();
    rt.block_on(get_available_backends())
}

/// Get all available database backends based on enabled features and environment.
///
/// This function initializes each backend that is available based on
/// the compile-time feature flags and returns them for benchmarking.
///
/// ## Environment Variables
///
/// For non-SQLite backends, set these environment variables:
///
/// - `POSTGRES_BENCH_URL`: PostgreSQL connection URL (e.g., `postgres://user:pass@localhost:5432/benchdb`)
/// - `MYSQL_BENCH_URL`: MySQL connection URL (e.g., `mysql://user:pass@localhost:3306/benchdb`)
///
/// ## Available Backends
///
/// - `sqlite-rusqlite`: In-memory SQLite via rusqlite (always available)
/// - `sqlite-sqlx`: In-memory SQLite via sqlx (always available)
/// - `turso`: In-memory Turso/LibSQL (always available)
/// - `postgres-raw`: PostgreSQL via tokio-postgres (requires `POSTGRES_BENCH_URL`)
/// - `postgres-sqlx`: PostgreSQL via sqlx (requires `POSTGRES_BENCH_URL`)
/// - `mysql-sqlx`: MySQL via sqlx (requires `MYSQL_BENCH_URL`)
pub async fn get_available_backends() -> Vec<BackendInfo> {
    let mut backends = Vec::new();

    // SQLite (rusqlite) - in-memory, always available
    #[cfg(feature = "sqlite-rusqlite")]
    if let Some(backend) = init_rusqlite().await {
        backends.push(backend);
    }

    // SQLite (sqlx) - in-memory, always available
    #[cfg(feature = "sqlite-sqlx")]
    if let Some(backend) = init_sqlx_sqlite().await {
        backends.push(backend);
    }

    // Turso - in-memory, always available
    #[cfg(feature = "turso")]
    if let Some(backend) = init_turso().await {
        backends.push(backend);
    }

    // PostgreSQL (raw/tokio-postgres) - requires POSTGRES_BENCH_URL env var
    #[cfg(feature = "postgres-raw")]
    if let Some(backend) = init_postgres_raw().await {
        backends.push(backend);
    }

    // PostgreSQL (sqlx) - requires POSTGRES_BENCH_URL env var
    #[cfg(feature = "postgres-sqlx")]
    if let Some(backend) = init_postgres_sqlx().await {
        backends.push(backend);
    }

    // MySQL (sqlx) - requires MYSQL_BENCH_URL env var
    #[cfg(feature = "mysql-sqlx")]
    if let Some(backend) = init_mysql_sqlx().await {
        backends.push(backend);
    }

    backends
}

// ============================================================================
// SQLite (rusqlite)
// ============================================================================

#[cfg(feature = "sqlite-rusqlite")]
async fn init_rusqlite() -> Option<BackendInfo> {
    let db = switchy_database_connection::init_sqlite_rusqlite(None).ok()?;

    Some(BackendInfo {
        name: "sqlite-rusqlite",
        db: Arc::new(db),
    })
}

// ============================================================================
// SQLite (sqlx)
// ============================================================================

#[cfg(feature = "sqlite-sqlx")]
async fn init_sqlx_sqlite() -> Option<BackendInfo> {
    let db = switchy_database_connection::init_sqlite_sqlx(None)
        .await
        .ok()?;

    Some(BackendInfo {
        name: "sqlite-sqlx",
        db: Arc::new(db),
    })
}

// ============================================================================
// Turso
// ============================================================================

#[cfg(feature = "turso")]
async fn init_turso() -> Option<BackendInfo> {
    let db = switchy_database_connection::init_turso_local(None)
        .await
        .ok()?;

    Some(BackendInfo {
        name: "turso",
        db: Arc::new(db),
    })
}

// ============================================================================
// PostgreSQL (raw/tokio-postgres)
// ============================================================================

#[cfg(feature = "postgres-raw")]
async fn init_postgres_raw() -> Option<BackendInfo> {
    let url = std::env::var("POSTGRES_BENCH_URL").ok()?;
    let creds = switchy_database_connection::Credentials::from_url(&url).ok()?;

    // Try native-tls first, then fall back to no-tls
    #[cfg(feature = "tls")]
    {
        if let Ok(db) = switchy_database_connection::init_postgres_raw_native_tls(creds).await {
            return Some(BackendInfo {
                name: "postgres-raw",
                db: Arc::new(db),
            });
        }
    }

    #[cfg(not(feature = "tls"))]
    {
        if let Ok(db) = switchy_database_connection::init_postgres_raw_no_tls(creds).await {
            return Some(BackendInfo {
                name: "postgres-raw",
                db: Arc::new(db),
            });
        }
    }

    None
}

// ============================================================================
// PostgreSQL (sqlx)
// ============================================================================

#[cfg(feature = "postgres-sqlx")]
async fn init_postgres_sqlx() -> Option<BackendInfo> {
    let url = std::env::var("POSTGRES_BENCH_URL").ok()?;
    let creds = switchy_database_connection::Credentials::from_url(&url).ok()?;

    let db = switchy_database_connection::init_postgres_sqlx(creds)
        .await
        .ok()?;

    Some(BackendInfo {
        name: "postgres-sqlx",
        db: Arc::new(db),
    })
}

// ============================================================================
// MySQL (sqlx)
// ============================================================================

#[cfg(feature = "mysql-sqlx")]
async fn init_mysql_sqlx() -> Option<BackendInfo> {
    let url = std::env::var("MYSQL_BENCH_URL").ok()?;
    let creds = switchy_database_connection::Credentials::from_url(&url).ok()?;

    let db = switchy_database_connection::init_mysql_sqlx(creds)
        .await
        .ok()?;

    Some(BackendInfo {
        name: "mysql-sqlx",
        db: Arc::new(db),
    })
}
