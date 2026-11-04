//! [`init_with_options`]: open a [`ConnectionTarget`] with the best enabled backend.

use switchy_database::Database;

#[allow(unused_imports)]
use crate::{
    InitDbError,
    target::{ConnectOptions, ConnectionTarget, SslMode, TlsFiles},
};

/// Default number of pooled connections.
#[allow(dead_code)]
const DEFAULT_POOL_SIZE: usize = 5;

#[allow(dead_code)]
fn pool_size(options: &ConnectOptions) -> usize {
    options
        .pool_size
        .map_or(DEFAULT_POOL_SIZE, std::num::NonZeroUsize::get)
}

#[allow(dead_code)]
fn pool_size_u32(options: &ConnectOptions) -> u32 {
    u32::try_from(pool_size(options)).unwrap_or(u32::MAX)
}

/// Open a connection to `target` using the best backend compiled into this
/// build, applying `options` natively.
///
/// This always opens real connections; unlike [`init`](crate::init) it never
/// substitutes the `simulator` database.
///
/// # Backend selection
///
/// * `PostgreSQL`: `postgres-sqlx` if enabled, otherwise `postgres-raw`
///   (TLS via `postgres-native-tls`, then `postgres-openssl`, otherwise none)
/// * `MySQL`: `mysql-sqlx`
/// * `SQLite`: `sqlite-rusqlite`, otherwise `sqlite-sqlx`, otherwise `turso`
/// * `DuckDB`: `duckdb`
///
/// # Option handling
///
/// * [`AccessMode::ReadOnly`](crate::AccessMode::ReadOnly):
///   * `PostgreSQL`: startup option `default_transaction_read_only=on` on every
///     pooled connection (a session can still explicitly `SET` it back; use a
///     read-only role for a hard guarantee)
///   * `MySQL`: `SET SESSION TRANSACTION READ ONLY` on every pooled connection
///   * `SQLite`: `SQLITE_OPEN_READ_ONLY` (rusqlite) / `read_only(true)` (sqlx);
///     the file must exist. Not supported for in-memory databases or Turso.
///   * `DuckDB`: `AccessMode::ReadOnly`; not supported for in-memory databases
/// * `statement_timeout`:
///   * `PostgreSQL`: startup option `statement_timeout` (milliseconds)
///   * `MySQL`: `SET SESSION max_execution_time` (milliseconds; applies to
///     `SELECT` only), falling back to `MariaDB`'s `max_statement_time` (seconds)
///   * `SQLite`/`DuckDB`/Turso: no server-side equivalent; ignored (this is not
///     mapped to the `SQLite` busy timeout, which is a different concept)
/// * `application_name`: `PostgreSQL` only (falls back to an
///   `application_name` URL parameter); ignored elsewhere
/// * `pool_size`: maximum pool size (default 5); ignored by Turso
///
/// For `PostgreSQL`, [`SslMode`] maps onto the driver's TLS modes; with
/// `postgres-raw`, `Allow`/`Prefer` become `prefer`, `Require` and the verify
/// modes become `require`, and the verification level is applied to the TLS
/// connector. Other URL query parameters are carried in the target but not
/// applied.
///
/// [`TlsFiles`] are applied as follows:
///
/// * `PostgreSQL` (`postgres-sqlx`): `ssl_root_cert`, `ssl_client_cert`, `ssl_client_key`
/// * `PostgreSQL` (`postgres-raw`): the root certificate is added to the TLS
///   connector's trust store and the client certificate/key pair becomes its
///   identity (`postgres-native-tls` requires a PKCS#8 PEM key). Certificate
///   files without a TLS feature are rejected.
/// * `MySQL` (`mysql-sqlx`): `ssl_ca`, `ssl_client_cert`, `ssl_client_key`; TLS
///   itself requires the `tls` feature
///
/// Whether the root certificate is used for verification depends on the
/// [`SslMode`]: modes that skip verification (`Require` and below) also skip it.
///
/// # Errors
///
/// * [`InitDbError::DriverNotEnabled`] when no backend for the target's driver is compiled in
/// * [`InitDbError::UnsupportedOption`] when an option cannot be honored (read-only
///   in-memory databases, read-only Turso, TLS or certificate files requested for
///   raw `PostgreSQL` without a TLS feature, a client certificate without a key or
///   vice versa for raw `PostgreSQL`)
/// * [`InitDbError::TlsFile`] when raw `PostgreSQL` cannot read a certificate file
/// * Backend initialization errors (`InitDatabase`, `InitMySql`, `InitSqlite`,
///   `InitSqliteSqlxDatabase`, `InitTurso`, `InitDuckDb`) when connecting or
///   opening fails
#[allow(clippy::unused_async)]
pub async fn init_with_options(
    target: &ConnectionTarget,
    options: &ConnectOptions,
) -> Result<Box<dyn Database>, InitDbError> {
    match target {
        ConnectionTarget::Postgres {
            creds,
            ssl_mode,
            tls_files,
            params,
        } => {
            #[cfg(feature = "postgres-sqlx")]
            {
                postgres_sqlx(creds, *ssl_mode, tls_files, params, options).await
            }
            #[cfg(all(not(feature = "postgres-sqlx"), feature = "postgres-raw"))]
            {
                postgres_raw(creds, *ssl_mode, tls_files, params, options)
            }
            #[cfg(not(any(feature = "postgres-sqlx", feature = "postgres-raw")))]
            {
                let _ = (creds, ssl_mode, tls_files, params, options);
                Err(InitDbError::DriverNotEnabled {
                    driver: "postgres",
                    features: "postgres-sqlx, postgres-raw",
                })
            }
        }
        ConnectionTarget::MySql {
            creds,
            ssl_mode,
            tls_files,
            params,
        } => {
            #[cfg(feature = "mysql-sqlx")]
            {
                let _ = params;
                mysql_sqlx(creds, *ssl_mode, tls_files, options).await
            }
            #[cfg(not(feature = "mysql-sqlx"))]
            {
                let _ = (creds, ssl_mode, tls_files, params, options);
                Err(InitDbError::DriverNotEnabled {
                    driver: "mysql",
                    features: "mysql-sqlx",
                })
            }
        }
        ConnectionTarget::Sqlite { path } => {
            #[cfg(feature = "sqlite-rusqlite")]
            {
                sqlite_rusqlite(path.as_deref(), options)
            }
            #[cfg(all(not(feature = "sqlite-rusqlite"), feature = "sqlite-sqlx"))]
            {
                sqlite_sqlx(path.as_deref(), options).await
            }
            #[cfg(all(
                not(feature = "sqlite-rusqlite"),
                not(feature = "sqlite-sqlx"),
                feature = "turso"
            ))]
            {
                sqlite_turso(path.as_deref(), options).await
            }
            #[cfg(not(any(
                feature = "sqlite-rusqlite",
                feature = "sqlite-sqlx",
                feature = "turso"
            )))]
            {
                let _ = (path, options);
                Err(InitDbError::DriverNotEnabled {
                    driver: "sqlite",
                    features: "sqlite-rusqlite, sqlite-sqlx, turso",
                })
            }
        }
        ConnectionTarget::DuckDb { path } => {
            #[cfg(feature = "duckdb")]
            {
                duckdb(path.as_deref(), options)
            }
            #[cfg(not(feature = "duckdb"))]
            {
                let _ = (path, options);
                Err(InitDbError::DriverNotEnabled {
                    driver: "duckdb",
                    features: "duckdb",
                })
            }
        }
    }
}

#[cfg(any(
    feature = "sqlite-rusqlite",
    feature = "sqlite-sqlx",
    feature = "duckdb"
))]
fn reject_read_only_in_memory(
    kind: &str,
    path: Option<&std::path::Path>,
    options: &ConnectOptions,
) -> Result<(), InitDbError> {
    if path.is_none() && options.is_read_only() {
        return Err(InitDbError::UnsupportedOption(format!(
            "read-only access to an in-memory {kind} database (it would always be empty)"
        )));
    }
    Ok(())
}

#[cfg(any(feature = "_sqlite", feature = "duckdb"))]
fn log_ignored_statement_timeout(kind: &str, options: &ConnectOptions) {
    if options.statement_timeout.is_some() {
        log::debug!("init_with_options: statement_timeout is not supported by {kind}; ignoring");
    }
}

/// Startup `-c` options for `PostgreSQL`.
#[cfg(feature = "postgres")]
fn postgres_startup_options(options: &ConnectOptions) -> Vec<(&'static str, String)> {
    let mut startup = Vec::new();
    if options.is_read_only() {
        startup.push(("default_transaction_read_only", "on".to_string()));
    }
    if let Some(timeout) = options.statement_timeout {
        startup.push(("statement_timeout", timeout.as_millis().to_string()));
    }
    startup
}

#[cfg(feature = "postgres")]
fn postgres_application_name<'a>(
    params: &'a std::collections::BTreeMap<String, String>,
    options: &'a ConnectOptions,
) -> Option<&'a str> {
    options
        .application_name
        .as_deref()
        .or_else(|| params.get("application_name").map(String::as_str))
}

#[cfg(feature = "postgres-sqlx")]
async fn postgres_sqlx(
    creds: &crate::Credentials,
    ssl_mode: Option<SslMode>,
    tls_files: &TlsFiles,
    params: &std::collections::BTreeMap<String, String>,
    options: &ConnectOptions,
) -> Result<Box<dyn Database>, InitDbError> {
    use std::sync::Arc;

    use sqlx::postgres::PgPoolOptions;
    use switchy_database::sqlx::postgres::PostgresSqlxDatabase;

    let connect_options = pg_connect_options(creds, ssl_mode, tls_files, params, options);

    let pool = PgPoolOptions::new()
        .max_connections(pool_size_u32(options))
        .connect_with(connect_options)
        .await
        .map_err(crate::InitDatabaseError::from)?;

    Ok(Box::new(PostgresSqlxDatabase::new(Arc::new(
        switchy_async::sync::Mutex::new(pool),
    ))))
}

#[cfg(feature = "postgres-sqlx")]
fn pg_connect_options(
    creds: &crate::Credentials,
    ssl_mode: Option<SslMode>,
    tls_files: &TlsFiles,
    params: &std::collections::BTreeMap<String, String>,
    options: &ConnectOptions,
) -> sqlx::postgres::PgConnectOptions {
    use sqlx::postgres::{PgConnectOptions, PgSslMode};

    let mut connect_options = PgConnectOptions::new();
    connect_options = if creds.host().starts_with('/') {
        connect_options.socket(creds.host())
    } else {
        connect_options.host(creds.host())
    };
    if let Some(port) = creds.port() {
        connect_options = connect_options.port(port);
    }
    if !creds.user().is_empty() {
        connect_options = connect_options.username(creds.user());
    }
    if let Some(password) = creds.password() {
        connect_options = connect_options.password(password);
    }
    if !creds.name().is_empty() {
        connect_options = connect_options.database(creds.name());
    }
    if let Some(mode) = ssl_mode {
        connect_options = connect_options.ssl_mode(match mode {
            SslMode::Disable => PgSslMode::Disable,
            SslMode::Allow => PgSslMode::Allow,
            SslMode::Prefer => PgSslMode::Prefer,
            SslMode::Require => PgSslMode::Require,
            SslMode::VerifyCa => PgSslMode::VerifyCa,
            SslMode::VerifyFull => PgSslMode::VerifyFull,
        });
    }
    if let Some(name) = postgres_application_name(params, options) {
        connect_options = connect_options.application_name(name);
    }
    if let Some(path) = &tls_files.root_cert {
        connect_options = connect_options.ssl_root_cert(path);
    }
    if let Some(path) = &tls_files.client_cert {
        connect_options = connect_options.ssl_client_cert(path);
    }
    if let Some(path) = &tls_files.client_key {
        connect_options = connect_options.ssl_client_key(path);
    }
    let startup = postgres_startup_options(options);
    if !startup.is_empty() {
        connect_options = connect_options.options(startup);
    }
    connect_options
}

#[cfg(all(not(feature = "postgres-sqlx"), feature = "postgres-raw"))]
#[allow(clippy::too_many_lines)]
fn postgres_raw(
    creds: &crate::Credentials,
    ssl_mode: Option<SslMode>,
    tls_files: &TlsFiles,
    params: &std::collections::BTreeMap<String, String>,
    options: &ConnectOptions,
) -> Result<Box<dyn Database>, InitDbError> {
    use std::fmt::Write as _;

    use switchy_database::postgres::postgres::PostgresDatabase;

    let mut config = tokio_postgres::Config::new();
    if creds.host().starts_with('/') {
        config.host_path(creds.host());
    } else {
        config.host(creds.host());
    }
    if let Some(port) = creds.port() {
        config.port(port);
    }
    if !creds.user().is_empty() {
        config.user(creds.user());
    }
    if let Some(password) = creds.password() {
        config.password(password);
    }
    if !creds.name().is_empty() {
        config.dbname(creds.name());
    }
    if let Some(name) = postgres_application_name(params, options) {
        config.application_name(name);
    }
    // Match sqlx, which always starts sessions in UTC.
    let mut startup_str = String::from("-c TimeZone=UTC");
    for (key, value) in postgres_startup_options(options) {
        write!(startup_str, " -c {key}={value}").ok();
    }
    config.options(startup_str);
    if let Some(mode) = ssl_mode {
        config.ssl_mode(match mode {
            SslMode::Disable => tokio_postgres::config::SslMode::Disable,
            SslMode::Allow | SslMode::Prefer => tokio_postgres::config::SslMode::Prefer,
            SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull => {
                tokio_postgres::config::SslMode::Require
            }
        });
    }

    let max_size = pool_size(options);

    if ssl_mode == Some(SslMode::Disable) {
        let pool = raw_pool(config, tokio_postgres::NoTls, max_size)?;
        return Ok(Box::new(PostgresDatabase::new(pool)));
    }
    if tls_files.client_cert.is_some() != tls_files.client_key.is_some() {
        return Err(InitDbError::UnsupportedOption(
            "a client certificate and a client key must be given together".to_string(),
        ));
    }

    #[cfg(feature = "postgres-native-tls")]
    {
        let mut builder = native_tls::TlsConnector::builder();
        match ssl_mode {
            Some(SslMode::Allow | SslMode::Prefer | SslMode::Require) => {
                builder.danger_accept_invalid_certs(true);
                builder.danger_accept_invalid_hostnames(true);
            }
            Some(SslMode::VerifyCa) => {
                builder.danger_accept_invalid_hostnames(true);
            }
            Some(SslMode::VerifyFull | SslMode::Disable) => {}
            None => {
                if matches!(
                    creds.host().to_lowercase().as_str(),
                    "localhost" | "127.0.0.1" | "0.0.0.0"
                ) {
                    builder.danger_accept_invalid_hostnames(true);
                }
            }
        }
        if let Some(path) = &tls_files.root_cert {
            let pem = read_tls_file(path)?;
            builder.add_root_certificate(
                native_tls::Certificate::from_pem(&pem).map_err(crate::InitDatabaseError::from)?,
            );
        }
        if let (Some(cert), Some(key)) = (&tls_files.client_cert, &tls_files.client_key) {
            let identity =
                native_tls::Identity::from_pkcs8(&read_tls_file(cert)?, &read_tls_file(key)?)
                    .map_err(crate::InitDatabaseError::from)?;
            builder.identity(identity);
        }
        let connector = postgres_native_tls::MakeTlsConnector::new(
            builder.build().map_err(crate::InitDatabaseError::from)?,
        );
        let pool = raw_pool(config, connector, max_size)?;
        Ok(Box::new(PostgresDatabase::new(pool)))
    }

    #[cfg(all(not(feature = "postgres-native-tls"), feature = "postgres-openssl"))]
    {
        use openssl::ssl::{SslConnector, SslFiletype, SslMethod, SslVerifyMode};

        let mut builder =
            SslConnector::builder(SslMethod::tls()).map_err(crate::InitDatabaseError::from)?;
        match ssl_mode {
            Some(SslMode::Allow | SslMode::Prefer | SslMode::Require) => {
                builder.set_verify(SslVerifyMode::NONE);
            }
            // `VerifyCa` is verified as strictly as `VerifyFull` with OpenSSL.
            Some(SslMode::VerifyCa | SslMode::VerifyFull | SslMode::Disable) => {}
            None => {
                if matches!(
                    creds.host().to_lowercase().as_str(),
                    "localhost" | "127.0.0.1" | "0.0.0.0"
                ) {
                    builder.set_verify(SslVerifyMode::NONE);
                }
            }
        }
        if let Some(path) = &tls_files.root_cert {
            read_tls_file(path)?;
            builder
                .set_ca_file(path)
                .map_err(crate::InitDatabaseError::from)?;
        }
        if let (Some(cert), Some(key)) = (&tls_files.client_cert, &tls_files.client_key) {
            read_tls_file(cert)?;
            read_tls_file(key)?;
            builder
                .set_certificate_chain_file(cert)
                .map_err(crate::InitDatabaseError::from)?;
            builder
                .set_private_key_file(key, SslFiletype::PEM)
                .map_err(crate::InitDatabaseError::from)?;
            builder
                .check_private_key()
                .map_err(crate::InitDatabaseError::from)?;
        }
        let connector = postgres_openssl::MakeTlsConnector::new(builder.build());
        let pool = raw_pool(config, connector, max_size)?;
        Ok(Box::new(PostgresDatabase::new(pool)))
    }

    #[cfg(not(any(feature = "postgres-native-tls", feature = "postgres-openssl")))]
    {
        if matches!(
            ssl_mode,
            Some(SslMode::Require | SslMode::VerifyCa | SslMode::VerifyFull)
        ) {
            return Err(InitDbError::UnsupportedOption(
                "TLS for postgres-raw requires the postgres-native-tls or postgres-openssl feature"
                    .to_string(),
            ));
        }
        if !tls_files.is_empty() {
            return Err(InitDbError::UnsupportedOption(
                "TLS certificate files for postgres-raw require the postgres-native-tls or \
                 postgres-openssl feature"
                    .to_string(),
            ));
        }
        let pool = raw_pool(config, tokio_postgres::NoTls, max_size)?;
        Ok(Box::new(PostgresDatabase::new(pool)))
    }
}

/// Read a TLS certificate or key file, reporting the path on failure.
#[cfg(all(
    not(feature = "postgres-sqlx"),
    feature = "postgres-raw",
    any(feature = "postgres-native-tls", feature = "postgres-openssl")
))]
fn read_tls_file(path: &std::path::Path) -> Result<Vec<u8>, InitDbError> {
    std::fs::read(path).map_err(|source| InitDbError::TlsFile {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(all(not(feature = "postgres-sqlx"), feature = "postgres-raw"))]
fn raw_pool<T>(
    config: tokio_postgres::Config,
    connector: T,
    max_size: usize,
) -> Result<deadpool_postgres::Pool, InitDbError>
where
    T: tokio_postgres::tls::MakeTlsConnect<tokio_postgres::Socket> + Clone + Sync + Send + 'static,
    T::Stream: Sync + Send,
    T::TlsConnect: Sync + Send,
    <T::TlsConnect as tokio_postgres::tls::TlsConnect<tokio_postgres::Socket>>::Future: Send,
{
    use deadpool_postgres::{ManagerConfig, RecyclingMethod};

    let manager_config = ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    };
    let manager = deadpool_postgres::Manager::from_config(config, connector, manager_config);
    Ok(deadpool_postgres::Pool::builder(manager)
        .max_size(max_size)
        .build()
        .map_err(crate::InitDatabaseError::from)?)
}

#[cfg(feature = "mysql-sqlx")]
async fn mysql_sqlx(
    creds: &crate::Credentials,
    ssl_mode: Option<SslMode>,
    tls_files: &TlsFiles,
    options: &ConnectOptions,
) -> Result<Box<dyn Database>, InitDbError> {
    use std::sync::Arc;

    use sqlx::mysql::MySqlPoolOptions;
    use switchy_database::sqlx::mysql::MySqlSqlxDatabase;

    let connect_options = mysql_connect_options(creds, ssl_mode, tls_files);
    if options.application_name.is_some() {
        log::debug!("init_with_options: application_name is not supported by MySQL; ignoring");
    }

    let read_only = options.is_read_only();
    let statement_timeout = options.statement_timeout;

    let mut pool_options = MySqlPoolOptions::new().max_connections(pool_size_u32(options));
    if read_only || statement_timeout.is_some() {
        pool_options = pool_options.after_connect(move |conn, _meta| {
            Box::pin(async move {
                use sqlx::Executor as _;

                if read_only {
                    conn.execute("SET SESSION TRANSACTION READ ONLY").await?;
                }
                if let Some(timeout) = statement_timeout {
                    let mysql = format!("SET SESSION max_execution_time = {}", timeout.as_millis());
                    if conn.execute(sqlx::AssertSqlSafe(mysql)).await.is_err() {
                        // MariaDB has no max_execution_time; it uses seconds.
                        let mariadb =
                            format!("SET SESSION max_statement_time = {}", timeout.as_secs_f64());
                        conn.execute(sqlx::AssertSqlSafe(mariadb)).await?;
                    }
                }
                Ok(())
            })
        });
    }

    let pool = pool_options
        .connect_with(connect_options)
        .await
        .map_err(crate::InitMySqlSqlxError::from)?;

    Ok(Box::new(MySqlSqlxDatabase::new(Arc::new(
        switchy_async::sync::Mutex::new(pool),
    ))))
}

#[cfg(feature = "mysql-sqlx")]
fn mysql_connect_options(
    creds: &crate::Credentials,
    ssl_mode: Option<SslMode>,
    tls_files: &TlsFiles,
) -> sqlx::mysql::MySqlConnectOptions {
    use sqlx::mysql::{MySqlConnectOptions, MySqlSslMode};

    let mut connect_options = MySqlConnectOptions::new();
    connect_options = if creds.host().starts_with('/') {
        connect_options.socket(creds.host())
    } else {
        connect_options.host(creds.host())
    };
    if let Some(port) = creds.port() {
        connect_options = connect_options.port(port);
    }
    if !creds.user().is_empty() {
        connect_options = connect_options.username(creds.user());
    }
    if let Some(password) = creds.password() {
        connect_options = connect_options.password(password);
    }
    if !creds.name().is_empty() {
        connect_options = connect_options.database(creds.name());
    }
    if let Some(mode) = ssl_mode {
        connect_options = connect_options.ssl_mode(match mode {
            SslMode::Disable => MySqlSslMode::Disabled,
            SslMode::Allow | SslMode::Prefer => MySqlSslMode::Preferred,
            SslMode::Require => MySqlSslMode::Required,
            SslMode::VerifyCa => MySqlSslMode::VerifyCa,
            SslMode::VerifyFull => MySqlSslMode::VerifyIdentity,
        });
    }
    if let Some(path) = &tls_files.root_cert {
        connect_options = connect_options.ssl_ca(path);
    }
    if let Some(path) = &tls_files.client_cert {
        connect_options = connect_options.ssl_client_cert(path);
    }
    if let Some(path) = &tls_files.client_key {
        connect_options = connect_options.ssl_client_key(path);
    }
    connect_options
}

#[cfg(feature = "sqlite-rusqlite")]
fn sqlite_rusqlite(
    path: Option<&std::path::Path>,
    options: &ConnectOptions,
) -> Result<Box<dyn Database>, InitDbError> {
    use crate::{SqliteAccess, SqliteOpenOptions};

    reject_read_only_in_memory("SQLite", path, options)?;
    log_ignored_statement_timeout("SQLite", options);

    let connections =
        std::num::NonZeroUsize::new(pool_size(options)).unwrap_or(std::num::NonZeroUsize::MIN);
    let (location, allow_uri) = path.map_or_else(
        || {
            (
                std::path::PathBuf::from(crate::unique_memory_db_uri("rusqlite")),
                true,
            )
        },
        |path| (path.to_path_buf(), false),
    );
    let open_options = SqliteOpenOptions {
        access: if options.is_read_only() {
            SqliteAccess::ReadOnly
        } else {
            SqliteAccess::ReadWriteCreate
        },
        allow_uri,
        busy_timeout: std::time::Duration::from_millis(10),
        foreign_keys: false,
        connections,
    };
    Ok(Box::new(crate::open_sqlite_rusqlite(
        &location,
        &open_options,
    )?))
}

#[cfg(all(not(feature = "sqlite-rusqlite"), feature = "sqlite-sqlx"))]
async fn sqlite_sqlx(
    path: Option<&std::path::Path>,
    options: &ConnectOptions,
) -> Result<Box<dyn Database>, InitDbError> {
    use std::sync::Arc;

    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
    use switchy_database::sqlx::sqlite::SqliteSqlxDatabase;

    reject_read_only_in_memory("SQLite", path, options)?;
    log_ignored_statement_timeout("SQLite", options);

    let connect_options = SqliteConnectOptions::new();
    let connect_options = if let Some(path) = path {
        connect_options
            .filename(path)
            .read_only(options.is_read_only())
            .create_if_missing(!options.is_read_only())
    } else {
        connect_options.filename(crate::unique_memory_db_uri("sqlx"))
    };

    let pool = SqlitePoolOptions::new()
        .max_connections(pool_size_u32(options))
        .connect_with(connect_options)
        .await
        .map_err(crate::InitSqliteSqlxDatabaseError::from)?;

    Ok(Box::new(SqliteSqlxDatabase::new(Arc::new(
        switchy_async::sync::Mutex::new(pool),
    ))))
}

#[cfg(all(
    not(feature = "sqlite-rusqlite"),
    not(feature = "sqlite-sqlx"),
    feature = "turso"
))]
async fn sqlite_turso(
    path: Option<&std::path::Path>,
    options: &ConnectOptions,
) -> Result<Box<dyn Database>, InitDbError> {
    if options.is_read_only() {
        return Err(InitDbError::UnsupportedOption(
            "read-only access is not supported by the Turso backend".to_string(),
        ));
    }
    log_ignored_statement_timeout("Turso", options);
    Ok(crate::init_turso_local(path).await?)
}

#[cfg(feature = "duckdb")]
fn duckdb(
    path: Option<&std::path::Path>,
    options: &ConnectOptions,
) -> Result<Box<dyn Database>, InitDbError> {
    reject_read_only_in_memory("DuckDB", path, options)?;
    log_ignored_statement_timeout("DuckDB", options);
    let config = crate::duckdb_config_from_env()?;
    Ok(crate::open_duckdb(
        path,
        options.is_read_only(),
        config,
        pool_size(options),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "sqlite-rusqlite")]
    fn temp_db_path(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "switchy_conn_{name}_{}_{nanos}.db",
            std::process::id()
        ))
    }

    #[cfg(all(feature = "sqlite-rusqlite", feature = "raw-sql"))]
    #[test_log::test(tokio::test)]
    async fn sqlite_read_only_rejects_writes() {
        let path = temp_db_path("ro");
        let target = ConnectionTarget::Sqlite {
            path: Some(path.clone()),
        };
        {
            let db = init_with_options(&target, &ConnectOptions::new())
                .await
                .unwrap();
            db.exec_raw("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)")
                .await
                .unwrap();
            db.exec_raw("INSERT INTO t (v) VALUES ('a')").await.unwrap();
            db.close().await.unwrap();
        }
        let db = init_with_options(&target, &ConnectOptions::new().read_only())
            .await
            .unwrap();
        let rows = db.query_raw("SELECT v FROM t").await.unwrap();
        assert_eq!(rows.len(), 1);
        assert!(db.exec_raw("INSERT INTO t (v) VALUES ('b')").await.is_err());
        db.close().await.unwrap();
        std::fs::remove_file(&path).ok();
    }

    #[cfg(feature = "sqlite-rusqlite")]
    #[test_log::test(tokio::test)]
    async fn sqlite_read_only_missing_file_fails() {
        let target = ConnectionTarget::Sqlite {
            path: Some(temp_db_path("missing")),
        };
        assert!(
            init_with_options(&target, &ConnectOptions::new().read_only())
                .await
                .is_err()
        );
    }

    #[cfg(any(feature = "_sqlite", feature = "duckdb"))]
    #[test_log::test(tokio::test)]
    async fn read_only_in_memory_is_unsupported() {
        for target in [
            ConnectionTarget::Sqlite { path: None },
            ConnectionTarget::DuckDb { path: None },
        ] {
            let result = init_with_options(&target, &ConnectOptions::new().read_only()).await;
            assert!(matches!(
                result,
                Err(InitDbError::UnsupportedOption(_) | InitDbError::DriverNotEnabled { .. })
            ));
        }
    }

    #[cfg(all(feature = "sqlite-rusqlite", feature = "raw-sql"))]
    #[test_log::test(tokio::test)]
    async fn sqlite_in_memory_from_url() {
        let target = ConnectionTarget::from_url("sqlite::memory:").unwrap();
        let db = init_with_options(
            &target,
            &ConnectOptions::new().pool_size(std::num::NonZeroUsize::new(2).unwrap()),
        )
        .await
        .unwrap();
        db.exec_raw("CREATE TABLE t (id INTEGER)").await.unwrap();
        // Shared-cache memory database: visible from every pooled connection
        for _ in 0..4 {
            assert!(db.query_raw("SELECT * FROM t").await.is_ok());
        }
    }

    #[cfg(not(feature = "mysql-sqlx"))]
    #[test_log::test(tokio::test)]
    async fn mysql_driver_not_enabled() {
        let target = ConnectionTarget::from_url("mysql://u@localhost/db").unwrap();
        assert!(matches!(
            init_with_options(&target, &ConnectOptions::new()).await,
            Err(InitDbError::DriverNotEnabled {
                driver: "mysql",
                ..
            })
        ));
    }

    #[cfg(not(any(feature = "postgres-sqlx", feature = "postgres-raw")))]
    #[test_log::test(tokio::test)]
    async fn postgres_driver_not_enabled() {
        let target = ConnectionTarget::from_url("postgres://u@localhost/db").unwrap();
        assert!(matches!(
            init_with_options(&target, &ConnectOptions::new()).await,
            Err(InitDbError::DriverNotEnabled {
                driver: "postgres",
                ..
            })
        ));
    }

    // DuckDB uses switchy_async blocking tasks, which require the simulator
    // runtime when the `simulator` feature is enabled.
    #[cfg(all(feature = "duckdb", feature = "raw-sql"))]
    #[test_log::test(switchy_async::test)]
    async fn duckdb_read_only_rejects_writes() {
        let path = std::env::temp_dir().join(format!(
            "switchy_conn_duck_{}_{}.duckdb",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let target = ConnectionTarget::DuckDb {
            path: Some(path.clone()),
        };
        {
            let db = init_with_options(&target, &ConnectOptions::new())
                .await
                .unwrap();
            db.exec_raw("CREATE TABLE t (id INTEGER)").await.unwrap();
            db.exec_raw("INSERT INTO t VALUES (1)").await.unwrap();
            db.close().await.unwrap();
        }
        let db = init_with_options(&target, &ConnectOptions::new().read_only())
            .await
            .unwrap();
        assert_eq!(db.query_raw("SELECT id FROM t").await.unwrap().len(), 1);
        assert!(db.exec_raw("INSERT INTO t VALUES (2)").await.is_err());
        db.close().await.unwrap();
        std::fs::remove_file(&path).ok();
    }

    #[cfg(feature = "postgres-sqlx")]
    #[test_log::test]
    fn postgres_sqlx_applies_tls_files() {
        let target = ConnectionTarget::from_url(
            "postgres://u@db.example.com/app?sslmode=verify-full\
             &sslrootcert=/certs/pg-ca.pem&sslcert=/certs/pg-client.pem&sslkey=/certs/pg-client.key",
        )
        .unwrap();
        let ConnectionTarget::Postgres {
            creds,
            ssl_mode,
            tls_files,
            params,
        } = &target
        else {
            panic!("expected postgres")
        };
        let options =
            pg_connect_options(creds, *ssl_mode, tls_files, params, &ConnectOptions::new());
        assert!(matches!(
            options.get_ssl_mode(),
            sqlx::postgres::PgSslMode::VerifyFull
        ));
        let debug = format!("{options:?}");
        for path in [
            "/certs/pg-ca.pem",
            "/certs/pg-client.pem",
            "/certs/pg-client.key",
        ] {
            assert!(debug.contains(path), "{path} missing from {debug}");
        }

        let target = ConnectionTarget::from_url("postgres://u@db.example.com/app").unwrap();
        let ConnectionTarget::Postgres {
            creds,
            ssl_mode,
            tls_files,
            params,
        } = &target
        else {
            panic!("expected postgres")
        };
        let debug = format!(
            "{:?}",
            pg_connect_options(creds, *ssl_mode, tls_files, params, &ConnectOptions::new())
        );
        assert!(!debug.contains("/certs/"));
    }

    #[cfg(feature = "mysql-sqlx")]
    #[test_log::test]
    fn mysql_sqlx_applies_tls_files() {
        let target = ConnectionTarget::from_url(
            "mysql://u@db.example.com/app?ssl-mode=VERIFY_CA\
             &ssl-ca=/certs/my-ca.pem&ssl-cert=/certs/my-client.pem&ssl-key=/certs/my-client.key",
        )
        .unwrap();
        let ConnectionTarget::MySql {
            creds,
            ssl_mode,
            tls_files,
            ..
        } = &target
        else {
            panic!("expected mysql")
        };
        let debug = format!("{:?}", mysql_connect_options(creds, *ssl_mode, tls_files));
        for path in [
            "/certs/my-ca.pem",
            "/certs/my-client.pem",
            "/certs/my-client.key",
            "VerifyCa",
        ] {
            assert!(debug.contains(path), "{path} missing from {debug}");
        }
    }

    /// Requires `POSTGRES_TEST_URL`, e.g. `postgres://postgres@localhost/postgres`.
    #[cfg(all(
        any(feature = "postgres-sqlx", feature = "postgres-raw"),
        feature = "raw-sql"
    ))]
    #[test_log::test(tokio::test)]
    async fn postgres_options_are_applied() {
        let Ok(url) = std::env::var("POSTGRES_TEST_URL") else {
            return;
        };
        let target = ConnectionTarget::from_url(&url).unwrap();
        let options = ConnectOptions::new()
            .read_only()
            .statement_timeout(std::time::Duration::from_millis(1234))
            .application_name("switchy_conn_test")
            .pool_size(std::num::NonZeroUsize::new(2).unwrap());
        let db = init_with_options(&target, &options).await.unwrap();
        let rows = db
            .query_raw(
                "SELECT current_setting('default_transaction_read_only') AS ro, \
                 current_setting('statement_timeout') AS st, \
                 current_setting('application_name') AS app",
            )
            .await
            .unwrap();
        let row = &rows[0];
        assert_eq!(row.get("ro").unwrap().as_str(), Some("on"));
        assert_eq!(row.get("st").unwrap().as_str(), Some("1234ms"));
        assert_eq!(row.get("app").unwrap().as_str(), Some("switchy_conn_test"));
        assert!(
            db.exec_raw("CREATE TABLE switchy_conn_ro_check (id int)")
                .await
                .is_err()
        );
        assert!(db.query_raw("SELECT pg_sleep(3)").await.is_err());
    }

    /// Requires `MYSQL_TEST_URL`, e.g. `mysql://root@127.0.0.1:3306/test`.
    #[cfg(all(feature = "mysql-sqlx", feature = "raw-sql"))]
    #[test_log::test(tokio::test)]
    async fn mysql_options_are_applied() {
        let Ok(url) = std::env::var("MYSQL_TEST_URL") else {
            return;
        };
        let target = ConnectionTarget::from_url(&url).unwrap();
        let rw = init_with_options(&target, &ConnectOptions::new())
            .await
            .unwrap();
        rw.exec_raw("CREATE TABLE IF NOT EXISTS switchy_conn_ro_check (id INT)")
            .await
            .unwrap();
        let options = ConnectOptions::new()
            .read_only()
            .statement_timeout(std::time::Duration::from_secs(1));
        let db = init_with_options(&target, &options).await.unwrap();
        assert!(
            db.exec_raw("INSERT INTO switchy_conn_ro_check VALUES (1)")
                .await
                .is_err()
        );
        assert!(
            db.query_raw("SELECT * FROM switchy_conn_ro_check")
                .await
                .is_ok()
        );
        let timeout_rows = match db
            .query_raw("SELECT @@session.max_execution_time AS v")
            .await
        {
            Ok(rows) => rows,
            // MariaDB
            Err(_) => db
                .query_raw("SELECT @@session.max_statement_time AS v")
                .await
                .unwrap(),
        };
        let value = timeout_rows[0].get("v").unwrap();
        let positive = value.as_f64().is_some_and(|v| v > 0.0)
            || value.as_i64().is_some_and(|v| v > 0)
            || value.as_u64().is_some_and(|v| v > 0);
        assert!(positive, "statement timeout not applied: {value:?}");
        rw.exec_raw("DROP TABLE switchy_conn_ro_check")
            .await
            .unwrap();
    }
}
