//! Database connection initialization and credential management.
//!
//! This crate provides a unified interface for initializing database connections
//! across multiple database backends, including `SQLite` (via rusqlite or sqlx),
//! `PostgreSQL` (via tokio-postgres or sqlx), and Turso. It supports various TLS
//! configurations and credential management through environment variables, URLs,
//! or AWS SSM parameters.
//!
//! # Features
//!
//! * Multiple database backends: `SQLite`, `PostgreSQL`, `Turso`
//! * TLS support for `PostgreSQL`: `OpenSSL`, native-tls, or no TLS
//! * Flexible credential management from URLs or environment
//! * Connection pooling with configurable pool sizes
//! * Feature-gated compilation for minimal dependencies
//!
//! # Examples
//!
//! ```rust,no_run
//! # use switchy_database_connection::{Credentials, init};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Parse credentials from a database URL
//! let creds = Credentials::from_url("postgres://user:pass@localhost:5432/mydb")?;
//!
//! // Initialize a database connection (parameters vary by feature)
//! # #[cfg(feature = "sqlite")]
//! let db = init(None, Some(creds)).await?;
//! # #[cfg(not(feature = "sqlite"))]
//! # let db = init(Some(creds)).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Feature Flags
//!
//! The crate uses feature flags to control which database backend is compiled:
//!
//! * `sqlite-rusqlite` - `SQLite` via rusqlite
//! * `sqlite-sqlx` - `SQLite` via sqlx
//! * `postgres-raw` - `PostgreSQL` via tokio-postgres
//! * `postgres-sqlx` - `PostgreSQL` via sqlx
//! * `postgres-native-tls` - `PostgreSQL` with native-tls
//! * `postgres-openssl` - `PostgreSQL` with `OpenSSL`
//! * `turso` - Turso database support
//! * `creds` - AWS SSM credential retrieval
//! * `simulator` - Test database simulator

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use switchy_database::Database;
use thiserror::Error;

#[cfg(feature = "creds")]
pub mod creds;

/// Database connection credentials
///
/// Contains host, database name, username, and optional password
/// for establishing database connections.
#[allow(unused)]
pub struct Credentials {
    host: String,
    port: Option<u16>,
    name: String,
    user: String,
    password: Option<String>,
}

/// Errors that can occur when parsing database credentials from a URL
#[derive(Debug, Error)]
pub enum CredentialsParseError {
    /// URL does not contain `://` separator
    #[error("Invalid URL format")]
    InvalidUrl,
    /// Host component is missing or empty in the URL
    #[error("Missing host")]
    MissingHost,
    /// Database name component is missing or empty in the URL
    #[error("Missing database name")]
    MissingDatabase,
    /// Username component is missing or empty in the URL
    #[error("Missing username")]
    MissingUsername,
    /// URL scheme is not supported (must be postgres, postgresql, or mysql)
    #[error("Unsupported scheme: {0}")]
    UnsupportedScheme(String),
}

impl Credentials {
    /// Creates new database credentials
    #[must_use]
    pub const fn new(
        host: String,
        port: Option<u16>,
        name: String,
        user: String,
        password: Option<String>,
    ) -> Self {
        Self {
            host,
            port,
            name,
            user,
            password,
        }
    }

    /// Parse credentials from a database URL
    ///
    /// Supports formats like:
    /// * `postgres://user:pass@host:port/dbname`
    /// * `mysql://user:pass@host:port/dbname`
    /// * `sqlite://path/to/db.sqlite`
    ///
    /// # Errors
    ///
    /// * If the URL format is invalid
    /// * If required components are missing
    /// * If the scheme is unsupported
    pub fn from_url(url: &str) -> Result<Self, CredentialsParseError> {
        // Simple URL parsing without external dependencies
        let url = url.trim();

        // Find scheme
        let scheme_end = url.find("://").ok_or(CredentialsParseError::InvalidUrl)?;
        let scheme = &url[..scheme_end];
        let rest = &url[scheme_end + 3..];

        match scheme {
            "postgres" | "postgresql" | "mysql" => {
                // Format: user:pass@host:port/dbname
                let (auth_host, dbname) = rest
                    .rsplit_once('/')
                    .ok_or(CredentialsParseError::MissingDatabase)?;

                let Some((auth, host_port)) = auth_host.rsplit_once('@') else {
                    return Err(CredentialsParseError::MissingUsername);
                };

                let (user, password) = if let Some((user, pass)) = auth.split_once(':') {
                    (user, Some(pass.to_string()))
                } else {
                    (auth, None)
                };

                // Split host and port
                let (host, port) = if let Some((h, p)) = host_port.rsplit_once(':') {
                    p.parse::<u16>()
                        .map_or((host_port, None), |port_num| (h, Some(port_num)))
                } else {
                    (host_port, None)
                };

                if user.is_empty() {
                    return Err(CredentialsParseError::MissingUsername);
                }
                if host.is_empty() {
                    return Err(CredentialsParseError::MissingHost);
                }
                if dbname.is_empty() {
                    return Err(CredentialsParseError::MissingDatabase);
                }

                Ok(Self::new(
                    host.to_string(),
                    port,
                    dbname.to_string(),
                    user.to_string(),
                    password,
                ))
            }
            _ => Err(CredentialsParseError::UnsupportedScheme(scheme.to_string())),
        }
    }

    /// Returns the database host
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Returns the database port, if present
    #[must_use]
    pub const fn port(&self) -> Option<u16> {
        self.port
    }

    /// Returns the database name
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the username
    #[must_use]
    pub fn user(&self) -> &str {
        &self.user
    }

    /// Returns the password, if present
    #[must_use]
    pub fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }
}

/// Errors that can occur when initializing a database connection
#[derive(Debug, Error)]
pub enum InitDbError {
    /// Error initializing `SQLite` via rusqlite
    #[cfg(feature = "sqlite-rusqlite")]
    #[error(transparent)]
    InitSqlite(#[from] InitSqliteRusqliteError),
    /// Error initializing `PostgreSQL` connection
    #[cfg(feature = "postgres")]
    #[error(transparent)]
    InitPostgres(#[from] InitPostgresError),
    /// Error initializing generic database connection
    #[cfg(feature = "postgres")]
    #[error(transparent)]
    InitDatabase(#[from] InitDatabaseError),
    /// Error initializing `SQLite` via sqlx
    #[cfg(any(
        feature = "sqlite-sqlx",
        all(
            feature = "sqlx",
            not(feature = "postgres"),
            not(feature = "postgres-sqlx"),
            not(feature = "sqlite-rusqlite")
        )
    ))]
    #[error(transparent)]
    InitSqliteSqlxDatabase(#[from] InitSqliteSqlxDatabaseError),
    /// Error initializing Turso database
    #[cfg(feature = "turso")]
    #[error(transparent)]
    InitTurso(#[from] InitTursoError),
    /// Error initializing `DuckDB` database
    #[cfg(feature = "duckdb")]
    #[error(transparent)]
    InitDuckDb(#[from] InitDuckDbError),
    /// Credentials were required but not provided
    #[error("Credentials are required")]
    CredentialsRequired,
    /// Generic database error
    #[error(transparent)]
    Database(#[from] switchy_database::DatabaseError),
}

/// Initializes a database connection based on active feature flags.
///
/// This function selects the appropriate database backend based on compile-time
/// features (e.g., `sqlite-rusqlite`, `postgres-raw`, `turso`) and returns a
/// boxed trait object implementing the `Database` interface.
///
/// # Panics
///
/// * If invalid features are specified for the crate
///
/// # Errors
///
/// * If fails to initialize the generic database connection
#[allow(clippy::branches_sharing_code, clippy::unused_async)]
pub async fn init(
    #[cfg(any(feature = "sqlite", feature = "duckdb"))]
    #[allow(unused)]
    path: Option<&std::path::Path>,
    #[allow(unused)] creds: Option<Credentials>,
) -> Result<Box<dyn Database>, InitDbError> {
    #[cfg(feature = "simulator")]
    {
        // Convert Path to string for the simulator
        #[cfg(any(feature = "sqlite", feature = "duckdb"))]
        let path_str = path.as_ref().map(|p| p.to_string_lossy().to_string());
        Ok(Box::new(
            switchy_database::simulator::SimulationDatabase::new_for_path(
                #[cfg(any(feature = "sqlite", feature = "duckdb"))]
                path_str.as_deref(),
                #[cfg(not(any(feature = "sqlite", feature = "duckdb")))]
                None,
            )
            .unwrap(),
        ))
    }

    #[cfg(not(feature = "simulator"))]
    {
        if cfg!(all(
            feature = "postgres-native-tls",
            feature = "postgres-raw"
        )) {
            #[cfg(all(feature = "postgres-native-tls", feature = "postgres-raw"))]
            return Ok(init_postgres_raw_native_tls(
                creds.ok_or(InitDbError::CredentialsRequired)?,
            )
            .await?);
            #[cfg(not(all(feature = "postgres-native-tls", feature = "postgres-raw")))]
            panic!("Invalid database features")
        } else if cfg!(all(feature = "postgres-openssl", feature = "postgres-raw")) {
            #[cfg(all(feature = "postgres-openssl", feature = "postgres-raw"))]
            return Ok(
                init_postgres_raw_openssl(creds.ok_or(InitDbError::CredentialsRequired)?).await?,
            );
            #[cfg(not(all(feature = "postgres-openssl", feature = "postgres-raw")))]
            panic!("Invalid database features")
        } else if cfg!(feature = "postgres-raw") {
            #[cfg(feature = "postgres-raw")]
            return Ok(
                init_postgres_raw_no_tls(creds.ok_or(InitDbError::CredentialsRequired)?).await?,
            );
            #[cfg(not(feature = "postgres-raw"))]
            panic!("Invalid database features")
        } else if cfg!(feature = "postgres-sqlx") {
            #[cfg(feature = "postgres-sqlx")]
            return Ok(init_postgres_sqlx(creds.ok_or(InitDbError::CredentialsRequired)?).await?);
            #[cfg(not(feature = "postgres-sqlx"))]
            panic!("Invalid database features")
        } else if cfg!(feature = "turso") {
            #[cfg(feature = "turso")]
            return Ok(init_turso_local(path).await?);
            #[cfg(not(feature = "turso"))]
            panic!("Invalid database features")
        } else if cfg!(feature = "duckdb") {
            #[cfg(feature = "duckdb")]
            return Ok(init_duckdb(path)?);
            #[cfg(not(feature = "duckdb"))]
            panic!("Invalid database features")
        } else if cfg!(feature = "sqlite-rusqlite") {
            #[cfg(feature = "sqlite-rusqlite")]
            return Ok(init_sqlite_rusqlite(path)?);
            #[cfg(not(feature = "sqlite-rusqlite"))]
            panic!("Invalid database features")
        } else if cfg!(feature = "sqlite-sqlx") {
            #[cfg(all(not(feature = "postgres"), feature = "sqlite", feature = "sqlite-sqlx"))]
            return Ok(init_sqlite_sqlx(path).await?);
            #[cfg(not(all(
                not(feature = "postgres"),
                feature = "sqlite",
                feature = "sqlite-sqlx"
            )))]
            panic!("Invalid database features")
        } else {
            panic!("Invalid database features")
        }
    }
}

/// Initializes a non-SQLite database connection based on active feature flags.
///
/// This function is similar to `init` but specifically for non-SQLite backends
/// (e.g., `PostgreSQL`). It selects the appropriate database based on compile-time
/// features and returns a boxed trait object implementing the `Database` interface.
///
/// # Panics
///
/// * If invalid features are specified for the crate
///
/// # Errors
///
/// * If fails to initialize the generic database connection
#[allow(clippy::branches_sharing_code, clippy::unused_async)]
pub async fn init_default_non_sqlite(
    #[allow(unused)] creds: Option<Credentials>,
) -> Result<Box<dyn Database>, InitDbError> {
    if cfg!(all(
        feature = "postgres-native-tls",
        feature = "postgres-raw"
    )) {
        #[cfg(all(feature = "postgres-native-tls", feature = "postgres-raw"))]
        return Ok(
            init_postgres_raw_native_tls(creds.ok_or(InitDbError::CredentialsRequired)?).await?,
        );
        #[cfg(not(all(feature = "postgres-native-tls", feature = "postgres-raw")))]
        panic!("Invalid database features")
    } else if cfg!(all(feature = "postgres-openssl", feature = "postgres-raw")) {
        #[cfg(all(feature = "postgres-openssl", feature = "postgres-raw"))]
        return Ok(
            init_postgres_raw_openssl(creds.ok_or(InitDbError::CredentialsRequired)?).await?,
        );
        #[cfg(not(all(feature = "postgres-openssl", feature = "postgres-raw")))]
        panic!("Invalid database features")
    } else if cfg!(feature = "postgres-raw") {
        #[cfg(feature = "postgres-raw")]
        return Ok(init_postgres_raw_no_tls(creds.ok_or(InitDbError::CredentialsRequired)?).await?);
        #[cfg(not(feature = "postgres-raw"))]
        panic!("Invalid database features")
    } else if cfg!(feature = "postgres-sqlx") {
        #[cfg(feature = "postgres-sqlx")]
        return Ok(init_postgres_sqlx(creds.ok_or(InitDbError::CredentialsRequired)?).await?);
        #[cfg(not(feature = "postgres-sqlx"))]
        panic!("Invalid database features")
    }

    panic!("Invalid database features")
}

/// Errors that can occur when initializing a `SQLite` connection via `rusqlite`
#[cfg(feature = "sqlite-rusqlite")]
#[derive(Debug, Error)]
pub enum InitSqliteRusqliteError {
    /// Rusqlite database error
    #[error(transparent)]
    Sqlite(#[from] ::rusqlite::Error),
}

/// Errors that can occur when initializing a `DuckDB` connection
#[cfg(feature = "duckdb")]
#[derive(Debug, Error)]
pub enum InitDuckDbError {
    /// `DuckDB` database error
    #[error(transparent)]
    DuckDb(#[from] ::duckdb::Error),
}

/// Initializes a `DuckDB` database connection.
///
/// Creates a connection pool with 5 connections. If no path is provided,
/// creates an in-memory database.
///
/// # Errors
///
/// * If fails to initialize the `DuckDB` connection
#[cfg(feature = "duckdb")]
pub fn init_duckdb(
    db_location: Option<&std::path::Path>,
) -> Result<Box<dyn Database>, InitDuckDbError> {
    let mut connections = Vec::new();
    for _ in 0..5 {
        let conn = if let Some(path) = db_location {
            ::duckdb::Connection::open(path)?
        } else {
            ::duckdb::Connection::open_in_memory()?
        };
        connections.push(std::sync::Arc::new(switchy_async::sync::Mutex::new(conn)));
    }

    Ok(Box::new(switchy_database::duckdb::DuckDbDatabase::new(
        connections,
    )))
}

/// Initializes a read-only `DuckDB` database connection.
///
/// Creates a connection pool with 5 connections opened in read-only mode.
///
/// # Errors
///
/// * If fails to initialize the `DuckDB` connection
///
/// # Panics
///
/// * If the `DuckDB` access mode configuration fails (should not happen)
#[cfg(feature = "duckdb")]
pub fn init_duckdb_read_only(
    db_location: &std::path::Path,
) -> Result<Box<dyn Database>, InitDuckDbError> {
    let mut connections = Vec::new();
    for _ in 0..5 {
        let config = ::duckdb::Config::default()
            .access_mode(::duckdb::AccessMode::ReadOnly)
            .expect("Failed to set DuckDB access mode");
        let conn = ::duckdb::Connection::open_with_flags(db_location, config)?;
        connections.push(std::sync::Arc::new(switchy_async::sync::Mutex::new(conn)));
    }

    Ok(Box::new(switchy_database::duckdb::DuckDbDatabase::new(
        connections,
    )))
}

/// Initializes a `SQLite` database connection using rusqlite.
///
/// Creates a connection pool with 5 connections. If no path is provided,
/// creates an in-memory database with a unique identifier.
///
/// # Errors
///
/// * If fails to initialize the Sqlite connection via rusqlite
///
/// # Panics
///
/// * If the `simulator` db connection fails to be initialized
#[cfg(feature = "sqlite-rusqlite")]
#[allow(unused, unreachable_code)]
pub fn init_sqlite_rusqlite(
    db_location: Option<&std::path::Path>,
) -> Result<Box<dyn Database>, InitSqliteRusqliteError> {
    #[cfg(feature = "simulator")]
    {
        // Convert Path to string for the simulator
        let path_str = db_location
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        return Ok(Box::new(
            switchy_database::simulator::SimulationDatabase::new_for_path(path_str.as_deref())
                .unwrap(),
        ));
    }

    let db_url = db_location.map_or_else(
        || {
            use std::sync::atomic::AtomicU64;

            static ID: AtomicU64 = AtomicU64::new(0);

            let id = ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();

            format!("file:rusqlite_memdb_{id}_{timestamp}:?mode=memory&cache=shared&uri=true")
        },
        |p| p.to_string_lossy().into_owned(),
    );

    let mut connections = Vec::new();
    for _ in 0..5 {
        let conn = ::rusqlite::Connection::open(&db_url)?;
        conn.busy_timeout(std::time::Duration::from_millis(10))?;

        connections.push(std::sync::Arc::new(switchy_async::sync::Mutex::new(conn)));
    }

    Ok(Box::new(switchy_database::rusqlite::RusqliteDatabase::new(
        connections,
    )))
}

/// Errors that can occur when initializing a `PostgreSQL` connection
#[cfg(feature = "postgres")]
#[derive(Debug, Error)]
pub enum InitPostgresError {
    /// Tokio-postgres connection error
    #[cfg(feature = "postgres-raw")]
    #[error(transparent)]
    Postgres(#[from] tokio_postgres::Error),
    /// Sqlx `PostgreSQL` error
    #[cfg(feature = "postgres-sqlx")]
    #[error(transparent)]
    PostgresSqlx(#[from] sqlx::Error),
}

/// Errors that can occur when initializing a `SQLite` connection via `sqlx`
#[cfg(any(
    feature = "sqlite-sqlx",
    all(
        feature = "sqlx",
        not(feature = "postgres"),
        not(feature = "postgres-sqlx"),
        not(feature = "sqlite-rusqlite")
    )
))]
#[derive(Debug, Error)]
pub enum InitSqliteSqlxDatabaseError {
    /// Sqlx `SQLite` error
    #[error(transparent)]
    SqliteSqlx(#[from] sqlx::Error),
}

/// Initializes a `SQLite` database connection using sqlx.
///
/// Creates a connection pool with 5 connections. If no path is provided,
/// creates an in-memory database with a unique identifier.
///
/// # Errors
///
/// * If fails to initialize the Sqlite connection via Sqlx
///
/// # Panics
///
/// * If the `simulator` db connection fails to be initialized
#[cfg(feature = "sqlite-sqlx")]
#[allow(unused, unreachable_code)]
pub async fn init_sqlite_sqlx(
    db_location: Option<&std::path::Path>,
) -> Result<Box<dyn Database>, InitSqliteSqlxDatabaseError> {
    use std::sync::Arc;

    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use switchy_database::sqlx::sqlite::SqliteSqlxDatabase;

    const CONNECTION_POOL_SIZE: u8 = 5;

    #[cfg(feature = "simulator")]
    {
        // Convert Path to string for the simulator
        let path_str = db_location
            .as_ref()
            .map(|p| p.to_string_lossy().to_string());
        return Ok(Box::new(
            switchy_database::simulator::SimulationDatabase::new_for_path(path_str.as_deref())
                .unwrap(),
        ));
    }

    let connect_options = SqliteConnectOptions::new();
    let mut connect_options = if let Some(db_location) = db_location {
        connect_options
            .filename(db_location)
            .create_if_missing(true)
    } else {
        use std::sync::atomic::AtomicU64;

        static ID: AtomicU64 = AtomicU64::new(0);

        let id = ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_url = format!("file:sqlx_memdb_{id}_{timestamp}:?mode=memory&cache=shared&uri=true");

        connect_options.filename(db_url)
    };

    let pool = SqlitePoolOptions::new()
        .max_connections(CONNECTION_POOL_SIZE.into())
        .connect_with(connect_options)
        .await?;

    Ok(Box::new(SqliteSqlxDatabase::new(Arc::new(
        switchy_async::sync::Mutex::new(pool),
    ))))
}

/// Errors that can occur when initializing a Turso database connection
#[cfg(feature = "turso")]
#[derive(Debug, Error)]
pub enum InitTursoError {
    /// Turso database error
    #[error(transparent)]
    Turso(#[from] switchy_database::turso::TursoDatabaseError),
}

/// Initializes a local Turso database connection.
///
/// If no path is provided, creates an in-memory database using `:memory:`.
///
/// # Errors
///
/// * If fails to initialize the Turso database connection
#[cfg(feature = "turso")]
pub async fn init_turso_local(
    path: Option<&std::path::Path>,
) -> Result<Box<dyn Database>, InitTursoError> {
    let db_path = path.map_or_else(
        || ":memory:".to_string(),
        |p| p.to_string_lossy().to_string(),
    );

    let db = switchy_database::turso::TursoDatabase::new(&db_path).await?;

    Ok(Box::new(db))
}

/// Errors that can occur during database initialization
#[cfg(feature = "postgres")]
#[derive(Debug, Error)]
pub enum InitDatabaseError {
    /// `OpenSSL` error during TLS setup
    #[cfg(all(feature = "postgres-openssl", feature = "postgres-raw"))]
    #[error(transparent)]
    OpenSsl(#[from] openssl::error::ErrorStack),
    /// Native-tls error during TLS setup
    #[cfg(all(feature = "postgres-native-tls", feature = "postgres-raw"))]
    #[error(transparent)]
    NativeTls(#[from] native_tls::Error),
    /// Tokio-postgres connection error
    #[cfg(feature = "postgres-raw")]
    #[error(transparent)]
    Postgres(#[from] tokio_postgres::Error),
    /// Sqlx `PostgreSQL` error
    #[cfg(feature = "postgres-sqlx")]
    #[error(transparent)]
    PostgresSqlx(#[from] sqlx::Error),
    /// Deadpool connection pool build error
    #[cfg(feature = "postgres-raw")]
    #[error(transparent)]
    DeadpoolBuildError(#[from] deadpool_postgres::BuildError),
    /// Invalid connection options provided
    #[error("Invalid Connection Options")]
    InvalidConnectionOptions,
}

/// Initializes a `PostgreSQL` database connection using sqlx.
///
/// Creates a connection pool with 5 connections using the provided credentials.
///
/// # Errors
///
/// * If fails to initialize the raw Postgres connection via Sqlx
#[cfg(feature = "postgres-sqlx")]
#[allow(unused)]
pub async fn init_postgres_sqlx(
    creds: Credentials,
) -> Result<Box<dyn Database>, InitDatabaseError> {
    use std::sync::Arc;

    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
    use switchy_database::sqlx::postgres::PostgresSqlxDatabase;

    let connect_options = PgConnectOptions::new();
    let mut connect_options = connect_options
        .host(&creds.host)
        .database(&creds.name)
        .username(&creds.user);

    if let Some(port) = creds.port {
        connect_options = connect_options.port(port);
    }

    if let Some(db_password) = &creds.password {
        connect_options = connect_options.password(db_password);
    }

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await?;

    Ok(Box::new(PostgresSqlxDatabase::new(Arc::new(
        switchy_async::sync::Mutex::new(pool),
    ))))
}

/// Errors that can occur when initializing a `MySQL` connection via `sqlx`
#[cfg(feature = "mysql-sqlx")]
#[derive(Debug, Error)]
pub enum InitMySqlSqlxError {
    /// Sqlx `MySQL` error
    #[error(transparent)]
    MySqlSqlx(#[from] sqlx::Error),
}

/// Initializes a `MySQL` database connection using sqlx.
///
/// Creates a connection pool with 5 connections using the provided credentials.
///
/// # Errors
///
/// * If fails to initialize the `MySQL` connection via Sqlx
#[cfg(feature = "mysql-sqlx")]
#[allow(unused)]
pub async fn init_mysql_sqlx(creds: Credentials) -> Result<Box<dyn Database>, InitMySqlSqlxError> {
    use std::sync::Arc;

    use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};
    use switchy_database::sqlx::mysql::MySqlSqlxDatabase;

    let connect_options = MySqlConnectOptions::new()
        .host(&creds.host)
        .database(&creds.name)
        .username(&creds.user);

    let connect_options = if let Some(port) = creds.port {
        connect_options.port(port)
    } else {
        connect_options
    };

    let connect_options = if let Some(db_password) = &creds.password {
        connect_options.password(db_password)
    } else {
        connect_options
    };

    let pool = MySqlPoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await?;

    Ok(Box::new(MySqlSqlxDatabase::new(Arc::new(
        switchy_async::sync::Mutex::new(pool),
    ))))
}

/// Initializes a `PostgreSQL` database connection using tokio-postgres with native-tls.
///
/// Creates a connection pool with 5 connections. For localhost connections,
/// accepts invalid hostnames to support self-signed certificates.
///
/// # Errors
///
/// * If fails to initialize the raw Postgres connection over native TLS
#[cfg(all(feature = "postgres-native-tls", feature = "postgres-raw"))]
#[allow(unused, clippy::unused_async)]
pub async fn init_postgres_raw_native_tls(
    creds: Credentials,
) -> Result<Box<dyn Database>, InitDatabaseError> {
    use deadpool_postgres::{ManagerConfig, RecyclingMethod};
    use postgres_native_tls::MakeTlsConnector;
    use switchy_database::postgres::postgres::PostgresDatabase;

    let mut config = tokio_postgres::Config::new();
    config
        .host(&creds.host)
        .dbname(&creds.name)
        .user(&creds.user);

    if let Some(port) = creds.port {
        config.port(port);
    }

    if let Some(db_password) = &creds.password {
        config.password(db_password);
    }

    let mut builder = native_tls::TlsConnector::builder();

    match creds.host.to_lowercase().as_str() {
        "localhost" | "127.0.0.1" | "0.0.0.0" => {
            builder.danger_accept_invalid_hostnames(true);
        }
        _ => {}
    }

    let connector = MakeTlsConnector::new(builder.build()?);

    let manager_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let manager = deadpool_postgres::Manager::from_config(config, connector, manager_config);
    let pool = deadpool_postgres::Pool::builder(manager)
        .max_size(5)
        .build()?;

    Ok(Box::new(PostgresDatabase::new(pool)))
}

/// Initializes a `PostgreSQL` database connection using tokio-postgres with `OpenSSL`.
///
/// Creates a connection pool with 5 connections. For localhost connections,
/// disables certificate verification to support self-signed certificates.
///
/// # Errors
///
/// * If fails to initialize the raw Postgres connection over OpenSSL
#[cfg(all(feature = "postgres-openssl", feature = "postgres-raw"))]
#[allow(unused, clippy::unused_async)]
pub async fn init_postgres_raw_openssl(
    creds: Credentials,
) -> Result<Box<dyn Database>, InitDatabaseError> {
    use deadpool_postgres::{ManagerConfig, RecyclingMethod};
    use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
    use postgres_openssl::MakeTlsConnector;
    use switchy_database::postgres::postgres::PostgresDatabase;

    let mut config = tokio_postgres::Config::new();
    config
        .host(&creds.host)
        .dbname(&creds.name)
        .user(&creds.user);

    if let Some(port) = creds.port {
        config.port(port);
    }

    if let Some(db_password) = &creds.password {
        config.password(db_password);
    }

    let mut builder = SslConnector::builder(SslMethod::tls())?;

    match creds.host.to_lowercase().as_str() {
        "localhost" | "127.0.0.1" | "0.0.0.0" => {
            builder.set_verify(SslVerifyMode::NONE);
        }
        _ => {}
    }

    let connector = MakeTlsConnector::new(builder.build());

    let manager_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let manager = deadpool_postgres::Manager::from_config(config, connector, manager_config);
    let pool = deadpool_postgres::Pool::builder(manager)
        .max_size(5)
        .build()?;

    Ok(Box::new(PostgresDatabase::new(pool)))
}

/// Initializes a `PostgreSQL` database connection using tokio-postgres without TLS.
///
/// Creates a connection pool with 5 connections using the provided credentials.
///
/// # Errors
///
/// * If fails to initialize the raw Postgres connection
#[cfg(feature = "postgres-raw")]
#[allow(unused, clippy::unused_async)]
pub async fn init_postgres_raw_no_tls(
    creds: Credentials,
) -> Result<Box<dyn Database>, InitDatabaseError> {
    use deadpool_postgres::{ManagerConfig, RecyclingMethod};
    use switchy_database::postgres::postgres::PostgresDatabase;

    let mut config = tokio_postgres::Config::new();
    config
        .host(&creds.host)
        .dbname(&creds.name)
        .user(&creds.user);

    if let Some(port) = creds.port {
        config.port(port);
    }

    if let Some(db_password) = &creds.password {
        config.password(db_password);
    }

    let connector = tokio_postgres::NoTls;

    let manager_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let manager = deadpool_postgres::Manager::from_config(config, connector, manager_config);
    let pool = deadpool_postgres::Pool::builder(manager)
        .max_size(5)
        .build()?;

    Ok(Box::new(PostgresDatabase::new(pool)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn test_credentials_from_url_postgres_with_password() {
        let url = "postgres://user:pass123@localhost:5432/mydb";
        let creds = Credentials::from_url(url).expect("Failed to parse URL");

        assert_eq!(creds.host(), "localhost");
        assert_eq!(creds.port(), Some(5432));
        assert_eq!(creds.name(), "mydb");
        assert_eq!(creds.user(), "user");
        assert_eq!(creds.password(), Some("pass123"));
    }

    #[test_log::test]
    fn test_credentials_from_url_postgres_without_password() {
        let url = "postgres://user@localhost:5432/mydb";
        let creds = Credentials::from_url(url).expect("Failed to parse URL");

        assert_eq!(creds.host(), "localhost");
        assert_eq!(creds.port(), Some(5432));
        assert_eq!(creds.name(), "mydb");
        assert_eq!(creds.user(), "user");
        assert_eq!(creds.password(), None);
    }

    #[test_log::test]
    fn test_credentials_from_url_postgresql_scheme() {
        let url = "postgresql://user:pass@host:1234/database";
        let creds = Credentials::from_url(url).expect("Failed to parse URL");

        assert_eq!(creds.host(), "host");
        assert_eq!(creds.port(), Some(1234));
        assert_eq!(creds.name(), "database");
        assert_eq!(creds.user(), "user");
        assert_eq!(creds.password(), Some("pass"));
    }

    #[test_log::test]
    fn test_credentials_from_url_mysql_scheme() {
        let url = "mysql://dbuser:dbpass@dbhost:3306/mydb";
        let creds = Credentials::from_url(url).expect("Failed to parse URL");

        assert_eq!(creds.host(), "dbhost");
        assert_eq!(creds.port(), Some(3306));
        assert_eq!(creds.name(), "mydb");
        assert_eq!(creds.user(), "dbuser");
        assert_eq!(creds.password(), Some("dbpass"));
    }

    #[test_log::test]
    fn test_credentials_from_url_with_special_chars_in_password() {
        let url = "postgres://user:p@ss:w0rd!@localhost:5432/mydb";
        let creds = Credentials::from_url(url).expect("Failed to parse URL");

        assert_eq!(creds.host(), "localhost");
        assert_eq!(creds.port(), Some(5432));
        assert_eq!(creds.name(), "mydb");
        assert_eq!(creds.user(), "user");
        assert_eq!(creds.password(), Some("p@ss:w0rd!"));
    }

    #[test_log::test]
    fn test_credentials_from_url_with_trailing_whitespace() {
        let url = "  postgres://user:pass@localhost:5432/mydb  ";
        let creds = Credentials::from_url(url).expect("Failed to parse URL");

        assert_eq!(creds.host(), "localhost");
        assert_eq!(creds.port(), Some(5432));
        assert_eq!(creds.name(), "mydb");
        assert_eq!(creds.user(), "user");
        assert_eq!(creds.password(), Some("pass"));
    }

    #[test_log::test]
    fn test_credentials_from_url_invalid_no_scheme_separator() {
        let url = "postgresuser:pass@localhost:5432/mydb";
        let result = Credentials::from_url(url);

        assert!(matches!(result, Err(CredentialsParseError::InvalidUrl)));
    }

    #[test_log::test]
    fn test_credentials_from_url_missing_database() {
        let url = "postgres://user:pass@localhost:5432/";
        let result = Credentials::from_url(url);

        assert!(matches!(
            result,
            Err(CredentialsParseError::MissingDatabase)
        ));
    }

    #[test_log::test]
    fn test_credentials_from_url_missing_database_no_slash() {
        let url = "postgres://user:pass@localhost:5432";
        let result = Credentials::from_url(url);

        assert!(matches!(
            result,
            Err(CredentialsParseError::MissingDatabase)
        ));
    }

    #[test_log::test]
    fn test_credentials_from_url_missing_host() {
        let url = "postgres://user:pass@/mydb";
        let result = Credentials::from_url(url);

        assert!(matches!(result, Err(CredentialsParseError::MissingHost)));
    }

    #[test_log::test]
    fn test_credentials_from_url_missing_username() {
        let url = "postgres://@localhost:5432/mydb";
        let result = Credentials::from_url(url);

        assert!(matches!(
            result,
            Err(CredentialsParseError::MissingUsername)
        ));
    }

    #[test_log::test]
    fn test_credentials_from_url_missing_username_with_password() {
        let url = "postgres://:pass@localhost:5432/mydb";
        let result = Credentials::from_url(url);

        assert!(matches!(
            result,
            Err(CredentialsParseError::MissingUsername)
        ));
    }

    #[test_log::test]
    fn test_credentials_from_url_missing_auth() {
        let url = "postgres://localhost:5432/mydb";
        let result = Credentials::from_url(url);

        assert!(matches!(
            result,
            Err(CredentialsParseError::MissingUsername)
        ));
    }

    #[test_log::test]
    fn test_credentials_from_url_unsupported_scheme() {
        let url = "mongodb://user:pass@localhost:27017/mydb";
        let result = Credentials::from_url(url);

        assert!(matches!(
            result,
            Err(CredentialsParseError::UnsupportedScheme(_))
        ));

        if let Err(CredentialsParseError::UnsupportedScheme(scheme)) = result {
            assert_eq!(scheme, "mongodb");
        }
    }

    #[test_log::test]
    fn test_credentials_from_url_sqlite_scheme_unsupported() {
        let url = "sqlite:///path/to/db.sqlite";
        let result = Credentials::from_url(url);

        assert!(matches!(
            result,
            Err(CredentialsParseError::UnsupportedScheme(_))
        ));
    }

    #[test_log::test]
    fn test_credentials_from_url_http_scheme_unsupported() {
        let url = "http://user:pass@localhost/mydb";
        let result = Credentials::from_url(url);

        assert!(matches!(
            result,
            Err(CredentialsParseError::UnsupportedScheme(_))
        ));
    }

    #[test_log::test]
    fn test_credentials_from_url_complex_host_with_domain() {
        let url = "postgres://user:pass@db.example.com:5432/production";
        let creds = Credentials::from_url(url).expect("Failed to parse URL");

        assert_eq!(creds.host(), "db.example.com");
        assert_eq!(creds.port(), Some(5432));
        assert_eq!(creds.name(), "production");
        assert_eq!(creds.user(), "user");
        assert_eq!(creds.password(), Some("pass"));
    }

    #[test_log::test]
    fn test_credentials_from_url_ipv4_host() {
        let url = "postgres://user:pass@192.168.1.100:5432/mydb";
        let creds = Credentials::from_url(url).expect("Failed to parse URL");

        assert_eq!(creds.host(), "192.168.1.100");
        assert_eq!(creds.port(), Some(5432));
        assert_eq!(creds.name(), "mydb");
        assert_eq!(creds.user(), "user");
    }

    #[test_log::test]
    fn test_credentials_from_url_localhost_without_port() {
        let url = "postgres://user:pass@localhost/mydb";
        let creds = Credentials::from_url(url).expect("Failed to parse URL");

        assert_eq!(creds.host(), "localhost");
        assert_eq!(creds.port(), None);
        assert_eq!(creds.name(), "mydb");
        assert_eq!(creds.user(), "user");
    }

    #[test_log::test]
    fn test_credentials_from_url_database_with_underscore() {
        let url = "postgres://user:pass@localhost:5432/my_database_name";
        let creds = Credentials::from_url(url).expect("Failed to parse URL");

        assert_eq!(creds.name(), "my_database_name");
    }

    #[test_log::test]
    fn test_credentials_from_url_username_with_underscore() {
        let url = "postgres://db_user:pass@localhost:5432/mydb";
        let creds = Credentials::from_url(url).expect("Failed to parse URL");

        assert_eq!(creds.user(), "db_user");
    }

    #[test_log::test]
    fn test_credentials_from_url_empty_password() {
        let url = "postgres://user:@localhost:5432/mydb";
        let creds = Credentials::from_url(url).expect("Failed to parse URL");

        assert_eq!(creds.password(), Some(""));
    }
}
