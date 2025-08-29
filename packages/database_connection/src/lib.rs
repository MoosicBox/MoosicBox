#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use switchy_database::Database;
use thiserror::Error;

#[cfg(feature = "creds")]
pub mod creds;

#[allow(unused)]
pub struct Credentials {
    host: String,
    name: String,
    user: String,
    password: Option<String>,
}

#[derive(Debug, Error)]
pub enum CredentialsParseError {
    #[error("Invalid URL format")]
    InvalidUrl,
    #[error("Missing host")]
    MissingHost,
    #[error("Missing database name")]
    MissingDatabase,
    #[error("Missing username")]
    MissingUsername,
    #[error("Unsupported scheme: {0}")]
    UnsupportedScheme(String),
}

impl Credentials {
    #[must_use]
    pub const fn new(host: String, name: String, user: String, password: Option<String>) -> Self {
        Self {
            host,
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

                let Some((auth, host)) = auth_host.rsplit_once('@') else {
                    return Err(CredentialsParseError::MissingUsername);
                };

                let (user, password) = if let Some((user, pass)) = auth.split_once(':') {
                    (user, Some(pass.to_string()))
                } else {
                    (auth, None)
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
                    dbname.to_string(),
                    user.to_string(),
                    password,
                ))
            }
            _ => Err(CredentialsParseError::UnsupportedScheme(scheme.to_string())),
        }
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn user(&self) -> &str {
        &self.user
    }

    #[must_use]
    pub fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }
}

#[derive(Debug, Error)]
pub enum InitDbError {
    #[cfg(feature = "sqlite-rusqlite")]
    #[error(transparent)]
    InitSqlite(#[from] InitSqliteRusqliteError),
    #[cfg(feature = "postgres")]
    #[error(transparent)]
    InitPostgres(#[from] InitPostgresError),
    #[cfg(feature = "postgres")]
    #[error(transparent)]
    InitDatabase(#[from] InitDatabaseError),
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
    #[error("Credentials are required")]
    CredentialsRequired,
    #[error(transparent)]
    Database(#[from] switchy_database::DatabaseError),
}

/// # Panics
///
/// * If invalid features are specified for the crate
///
/// # Errors
///
/// * If fails to initialize the generic database connection
#[allow(clippy::branches_sharing_code, clippy::unused_async)]
pub async fn init(
    #[cfg(feature = "sqlite")]
    #[allow(unused)]
    path: Option<&std::path::Path>,
    #[allow(unused)] creds: Option<Credentials>,
) -> Result<Box<dyn Database>, InitDbError> {
    #[cfg(feature = "simulator")]
    {
        Ok(Box::new(
            switchy_database::simulator::SimulationDatabase::new()?,
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

#[cfg(feature = "sqlite-rusqlite")]
#[derive(Debug, Error)]
pub enum InitSqliteRusqliteError {
    #[error(transparent)]
    Sqlite(#[from] ::rusqlite::Error),
}

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
        return Ok(Box::new(
            switchy_database::simulator::SimulationDatabase::new().unwrap(),
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

        connections.push(std::sync::Arc::new(tokio::sync::Mutex::new(conn)));
    }

    Ok(Box::new(switchy_database::rusqlite::RusqliteDatabase::new(
        connections,
    )))
}

#[cfg(feature = "postgres")]
#[derive(Debug, Error)]
pub enum InitPostgresError {
    #[cfg(feature = "postgres-raw")]
    #[error(transparent)]
    Postgres(#[from] tokio_postgres::Error),
    #[cfg(feature = "postgres-sqlx")]
    #[error(transparent)]
    PostgresSqlx(#[from] sqlx::Error),
}

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
    #[error(transparent)]
    SqliteSqlx(#[from] sqlx::Error),
}

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
        return Ok(Box::new(
            switchy_database::simulator::SimulationDatabase::new().unwrap(),
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
        tokio::sync::Mutex::new(pool),
    ))))
}

#[cfg(feature = "postgres")]
#[derive(Debug, Error)]
pub enum InitDatabaseError {
    #[cfg(all(feature = "postgres-openssl", feature = "postgres-raw"))]
    #[error(transparent)]
    OpenSsl(#[from] openssl::error::ErrorStack),
    #[cfg(all(feature = "postgres-native-tls", feature = "postgres-raw"))]
    #[error(transparent)]
    NativeTls(#[from] native_tls::Error),
    #[cfg(feature = "postgres-raw")]
    #[error(transparent)]
    Postgres(#[from] tokio_postgres::Error),
    #[cfg(feature = "postgres-sqlx")]
    #[error(transparent)]
    PostgresSqlx(#[from] sqlx::Error),
    #[error("Invalid Connection Options")]
    InvalidConnectionOptions,
}

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

    if let Some(db_password) = &creds.password {
        connect_options = connect_options.password(db_password);
    }

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await?;

    Ok(Box::new(PostgresSqlxDatabase::new(Arc::new(
        tokio::sync::Mutex::new(pool),
    ))))
}

/// # Errors
///
/// * If fails to initialize the raw Postgres connection over native TLS
#[cfg(all(feature = "postgres-native-tls", feature = "postgres-raw"))]
#[allow(unused)]
pub async fn init_postgres_raw_native_tls(
    creds: Credentials,
) -> Result<Box<dyn Database>, InitDatabaseError> {
    use postgres_native_tls::MakeTlsConnector;
    use switchy_database::postgres::postgres::PostgresDatabase;

    let mut config = tokio_postgres::Config::new();
    let mut config = config
        .host(&creds.host)
        .dbname(&creds.name)
        .user(&creds.user);

    if let Some(db_password) = &creds.password {
        config = config.password(db_password);
    }

    let mut builder = native_tls::TlsConnector::builder();

    match creds.host.to_lowercase().as_str() {
        "localhost" | "127.0.0.1" | "0.0.0.0" => {
            builder.danger_accept_invalid_hostnames(true);
        }
        _ => {}
    }

    let connector = MakeTlsConnector::new(builder.build()?);

    let (client, connection) = config.connect(connector).await?;

    Ok(Box::new(PostgresDatabase::new(client, connection)))
}

/// # Errors
///
/// * If fails to initialize the raw Postgres connection over OpenSSL
#[cfg(all(feature = "postgres-openssl", feature = "postgres-raw"))]
#[allow(unused)]
pub async fn init_postgres_raw_openssl(
    creds: Credentials,
) -> Result<Box<dyn Database>, InitDatabaseError> {
    use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
    use postgres_openssl::MakeTlsConnector;
    use switchy_database::postgres::postgres::PostgresDatabase;

    let mut config = tokio_postgres::Config::new();
    let mut config = config
        .host(&creds.host)
        .dbname(&creds.name)
        .user(&creds.user);

    if let Some(db_password) = &creds.password {
        config = config.password(db_password);
    }

    let mut builder = SslConnector::builder(SslMethod::tls())?;

    match creds.host.to_lowercase().as_str() {
        "localhost" | "127.0.0.1" | "0.0.0.0" => {
            builder.set_verify(SslVerifyMode::NONE);
        }
        _ => {}
    }

    let connector = MakeTlsConnector::new(builder.build());

    let (client, connection) = config.connect(connector).await?;

    Ok(Box::new(PostgresDatabase::new(client, connection)))
}

/// # Errors
///
/// * If fails to initialize the raw Postgres connection
#[cfg(feature = "postgres-raw")]
#[allow(unused)]
pub async fn init_postgres_raw_no_tls(
    creds: Credentials,
) -> Result<Box<dyn Database>, InitDatabaseError> {
    use switchy_database::postgres::postgres::PostgresDatabase;

    let mut config = tokio_postgres::Config::new();
    let mut config = config
        .host(&creds.host)
        .dbname(&creds.name)
        .user(&creds.user);

    if let Some(db_password) = &creds.password {
        config = config.password(db_password);
    }

    let connector = tokio_postgres::NoTls;

    let (client, connection) = config.connect(connector).await?;

    Ok(Box::new(PostgresDatabase::new(client, connection)))
}
