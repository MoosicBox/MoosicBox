#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use moosicbox_database::Database;
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
    path: &std::path::Path,
    #[allow(unused)] creds: Option<Credentials>,
) -> Result<Box<dyn Database>, InitDbError> {
    #[cfg(feature = "simulator")]
    if moosicbox_simulator_utils::simulator_enabled() {
        return Ok(Box::new(
            moosicbox_database::simulator::SimulationDatabase::new(),
        ));
    }

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
    } else if cfg!(feature = "sqlite-rusqlite") {
        #[cfg(feature = "sqlite-rusqlite")]
        return Ok(init_sqlite_rusqlite(path)?);
        #[cfg(not(feature = "sqlite-rusqlite"))]
        panic!("Invalid database features")
    } else if cfg!(feature = "sqlite-sqlx") {
        #[cfg(all(not(feature = "postgres"), feature = "sqlite", feature = "sqlite-sqlx"))]
        return Ok(init_sqlite_sqlx(path).await?);
        #[cfg(not(all(not(feature = "postgres"), feature = "sqlite", feature = "sqlite-sqlx")))]
        panic!("Invalid database features")
    } else {
        panic!("Invalid database features")
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
#[cfg(feature = "sqlite-rusqlite")]
pub fn init_sqlite_rusqlite(
    db_location: &std::path::Path,
) -> Result<Box<dyn Database>, InitSqliteRusqliteError> {
    let library = ::rusqlite::Connection::open(db_location)?;
    library.busy_timeout(std::time::Duration::from_millis(10))?;
    let library = std::sync::Arc::new(tokio::sync::Mutex::new(library));

    Ok(Box::new(
        moosicbox_database::rusqlite::RusqliteDatabase::new(library),
    ))
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
#[cfg(feature = "sqlite-sqlx")]
#[allow(unused)]
pub async fn init_sqlite_sqlx(
    db_location: &std::path::Path,
) -> Result<Box<dyn Database>, InitSqliteSqlxDatabaseError> {
    use std::sync::Arc;

    use moosicbox_database::sqlx::sqlite::SqliteSqlxDatabase;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let connect_options = SqliteConnectOptions::new();
    let mut connect_options = connect_options.filename(db_location);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
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

    use moosicbox_database::sqlx::postgres::PostgresSqlxDatabase;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

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
    use moosicbox_database::postgres::postgres::PostgresDatabase;
    use postgres_native_tls::MakeTlsConnector;

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
    use moosicbox_database::postgres::postgres::PostgresDatabase;
    use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
    use postgres_openssl::MakeTlsConnector;

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
    use moosicbox_database::postgres::postgres::PostgresDatabase;

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
