#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

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
    pub fn new(host: String, name: String, user: String, password: Option<String>) -> Self {
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
    InitSqlite(#[from] InitSqliteError),
    #[cfg(feature = "postgres")]
    #[error(transparent)]
    InitPostgres(#[from] InitPostgresError),
    #[cfg(feature = "postgres")]
    #[error(transparent)]
    InitDatabase(#[from] InitDatabaseError),
    #[cfg(any(
        feature = "sqlite-sqlx",
        all(
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

pub async fn init(
    #[allow(unused)] creds: Option<Credentials>,
) -> Result<Box<dyn Database>, InitDbError> {
    #[cfg(any(
        feature = "postgres",
        feature = "sqlite",
        all(
            not(feature = "postgres"),
            not(feature = "postgres-sqlx"),
            not(feature = "sqlite-rusqlite")
        )
    ))]
    let db_profile_path = {
        let db_profile_dir_path = moosicbox_config::make_profile_db_dir_path("master")
            .expect("Failed to get DB profile dir path");

        db_profile_dir_path.join("library.db")
    };

    #[cfg(any(feature = "postgres", feature = "sqlite",))]
    {
        let db_profile_path_str = db_profile_path
            .to_str()
            .expect("Failed to get DB profile path");
        if let Err(e) = moosicbox_schema::migrate_library(db_profile_path_str) {
            moosicbox_assert::die_or_panic!("Failed to migrate database: {e:?}");
        };
    }

    #[cfg(all(feature = "postgres-native-tls", feature = "postgres-raw"))]
    #[allow(unused)]
    let db = init_postgres_raw_native_tls(creds.ok_or(InitDbError::CredentialsRequired)?).await?;
    #[cfg(all(
        not(feature = "postgres-native-tls"),
        feature = "postgres-openssl",
        feature = "postgres-raw"
    ))]
    #[allow(unused)]
    let db = init_postgres_raw_openssl(creds.ok_or(InitDbError::CredentialsRequired)?).await?;
    #[cfg(all(
        not(feature = "postgres-native-tls"),
        not(feature = "postgres-openssl"),
        not(feature = "postgres-sqlx"),
        feature = "postgres-raw"
    ))]
    let db = init_postgres_raw_no_tls(creds.ok_or(InitDbError::CredentialsRequired)?).await?;
    #[cfg(all(not(feature = "sqlite-sqlx"), feature = "postgres-sqlx"))]
    let db = init_postgres_sqlx(creds.ok_or(InitDbError::CredentialsRequired)?).await?;
    #[cfg(feature = "sqlite-rusqlite")]
    #[allow(unused_variables)]
    let db = init_sqlite(&db_profile_path)?;
    #[cfg(any(
        feature = "sqlite-sqlx",
        all(
            not(feature = "postgres"),
            not(feature = "postgres-sqlx"),
            not(feature = "sqlite-rusqlite")
        )
    ))]
    #[allow(unused_variables)]
    let db = init_sqlite_sqlx(&db_profile_path).await?;

    Ok(db)
}

#[cfg(feature = "sqlite-rusqlite")]
#[derive(Debug, Error)]
pub enum InitSqliteError {
    #[error(transparent)]
    Sqlite(#[from] ::rusqlite::Error),
}

#[cfg(feature = "sqlite-rusqlite")]
pub fn init_sqlite(db_location: &std::path::Path) -> Result<Box<dyn Database>, InitSqliteError> {
    let library = ::rusqlite::Connection::open(db_location)?;
    library
        .busy_timeout(std::time::Duration::from_millis(10))
        .expect("Failed to set busy timeout");
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

#[cfg(any(
    feature = "sqlite-sqlx",
    all(
        not(feature = "postgres"),
        not(feature = "postgres-sqlx"),
        not(feature = "sqlite-rusqlite")
    )
))]
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
