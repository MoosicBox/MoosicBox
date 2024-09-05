use moosicbox_database::Database;
use thiserror::Error;

#[cfg(feature = "sqlite-rusqlite")]
#[derive(Debug, Error)]
pub enum InitSqliteError {
    #[error(transparent)]
    Sqlite(#[from] ::rusqlite::Error),
}

#[cfg(feature = "sqlite-rusqlite")]
pub fn init_sqlite() -> Result<Box<dyn Database>, InitSqliteError> {
    let library = ::rusqlite::Connection::open("library.db")?;
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

#[cfg(feature = "postgres")]
#[allow(unused)]
async fn get_db_config() -> Result<(String, String, String, Option<String>), InitDatabaseError> {
    let env_db_host = std::env::var("DB_HOST").ok();
    let env_db_name = std::env::var("DB_NAME").ok();
    let env_db_user = std::env::var("DB_USER").ok();
    let env_db_password = std::env::var("DB_PASSWORD").ok();

    Ok(
        if env_db_host.is_some() || env_db_name.is_some() || env_db_user.is_some() {
            (
                env_db_host.ok_or(InitDatabaseError::InvalidConnectionOptions)?,
                env_db_name.ok_or(InitDatabaseError::InvalidConnectionOptions)?,
                env_db_user.ok_or(InitDatabaseError::InvalidConnectionOptions)?,
                env_db_password,
            )
        } else {
            use aws_config::{BehaviorVersion, Region};
            use aws_sdk_ssm::Client;
            use std::collections::HashMap;

            let config = aws_config::defaults(BehaviorVersion::latest())
                .region(Region::new("us-east-1"))
                .load()
                .await;

            let client = Client::new(&config);

            let ssm_db_name_param_name = std::env::var("SSM_DB_NAME_PARAM_NAME")
                .unwrap_or_else(|_| "moosicbox_server_db_name".to_string());
            let ssm_db_host_param_name = std::env::var("SSM_DB_HOST_PARAM_NAME")
                .unwrap_or_else(|_| "moosicbox_server_db_hostname".to_string());
            let ssm_db_user_param_name = std::env::var("SSM_DB_USER_PARAM_NAME")
                .unwrap_or_else(|_| "moosicbox_server_db_user".to_string());
            let ssm_db_password_param_name = std::env::var("SSM_DB_PASSWORD_PARAM_NAME")
                .unwrap_or_else(|_| "moosicbox_server_db_password".to_string());

            let ssm_db_name_param_name = ssm_db_name_param_name.as_str();
            let ssm_db_host_param_name = ssm_db_host_param_name.as_str();
            let ssm_db_user_param_name = ssm_db_user_param_name.as_str();
            let ssm_db_password_param_name = ssm_db_password_param_name.as_str();

            let params = match client
                .get_parameters()
                .set_with_decryption(Some(true))
                .names(ssm_db_name_param_name)
                .names(ssm_db_host_param_name)
                .names(ssm_db_password_param_name)
                .names(ssm_db_user_param_name)
                .send()
                .await
            {
                Ok(params) => params,
                Err(err) => panic!("Failed to get parameters {err:?}"),
            };
            let params = params.parameters.expect("Failed to get params");
            let params: HashMap<String, String> = params
                .iter()
                .map(|param| {
                    (
                        param.name().unwrap().to_string(),
                        param.value().unwrap().to_string(),
                    )
                })
                .collect();

            let password = params
                .get(ssm_db_password_param_name)
                .cloned()
                .expect("No db_password");

            let password = if password.is_empty() {
                None
            } else {
                Some(password)
            };

            (
                params
                    .get(ssm_db_host_param_name)
                    .cloned()
                    .expect("No hostname"),
                params
                    .get(ssm_db_name_param_name)
                    .cloned()
                    .expect("No db_name"),
                params
                    .get(ssm_db_user_param_name)
                    .cloned()
                    .expect("No db_user"),
                password,
            )
        },
    )
}

#[cfg(all(
    not(feature = "postgres"),
    not(feature = "postgres-sqlx"),
    not(feature = "sqlite-rusqlite")
))]
#[derive(Debug, Error)]
pub enum InitDatabaseError {
    #[error(transparent)]
    SqliteSqlx(#[from] sqlx::Error),
}

#[cfg(all(
    not(feature = "postgres"),
    not(feature = "postgres-sqlx"),
    not(feature = "sqlite-rusqlite")
))]
#[allow(unused)]
pub async fn init_sqlite_sqlx() -> Result<Box<dyn Database>, InitDatabaseError> {
    use std::sync::Arc;

    use moosicbox_database::sqlx::sqlite::SqliteSqlxDatabase;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let connect_options = SqliteConnectOptions::new();
    let mut connect_options = connect_options.filename("library.db");

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
pub async fn init_postgres_sqlx() -> Result<Box<dyn Database>, InitDatabaseError> {
    use std::sync::Arc;

    use moosicbox_database::sqlx::postgres::PostgresSqlxDatabase;
    use sqlx::postgres::{PgConnectOptions, PgPoolOptions};

    let (db_host, db_name, db_user, db_password) = get_db_config().await?;

    let connect_options = PgConnectOptions::new();
    let mut connect_options = connect_options
        .host(&db_host)
        .database(&db_name)
        .username(&db_user);

    if let Some(ref db_password) = db_password {
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
pub async fn init_postgres_raw_native_tls() -> Result<
    (
        Box<dyn Database>,
        tokio_postgres::Connection<
            tokio_postgres::Socket,
            postgres_native_tls::TlsStream<tokio_postgres::Socket>,
        >,
    ),
    InitDatabaseError,
> {
    use moosicbox_database::postgres::postgres::PostgresDatabase;
    use postgres_native_tls::MakeTlsConnector;

    let (db_host, db_name, db_user, db_password) = get_db_config().await?;

    let mut config = tokio_postgres::Config::new();
    let mut config = config.host(&db_host).dbname(&db_name).user(&db_user);

    if let Some(ref db_password) = db_password {
        config = config.password(db_password);
    }

    let mut builder = native_tls::TlsConnector::builder();

    match db_host.to_lowercase().as_str() {
        "localhost" | "127.0.0.1" | "0.0.0.0" => {
            builder.danger_accept_invalid_hostnames(true);
        }
        _ => {}
    }

    let connector = MakeTlsConnector::new(builder.build()?);

    let (client, connection) = config.connect(connector).await?;

    Ok((Box::new(PostgresDatabase::new(client)), connection))
}

#[cfg(all(feature = "postgres-openssl", feature = "postgres-raw"))]
#[allow(unused)]
pub async fn init_postgres_raw_openssl() -> Result<
    (
        Box<dyn Database>,
        tokio_postgres::Connection<
            tokio_postgres::Socket,
            postgres_openssl::TlsStream<tokio_postgres::Socket>,
        >,
    ),
    InitDatabaseError,
> {
    use moosicbox_database::postgres::postgres::PostgresDatabase;
    use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
    use postgres_openssl::MakeTlsConnector;

    let (db_host, db_name, db_user, db_password) = get_db_config().await?;

    let mut config = tokio_postgres::Config::new();
    let mut config = config.host(&db_host).dbname(&db_name).user(&db_user);

    if let Some(ref db_password) = db_password {
        config = config.password(db_password);
    }

    let mut builder = SslConnector::builder(SslMethod::tls())?;

    match db_host.to_lowercase().as_str() {
        "localhost" | "127.0.0.1" | "0.0.0.0" => {
            builder.set_verify(SslVerifyMode::NONE);
        }
        _ => {}
    }

    let connector = MakeTlsConnector::new(builder.build());

    let (client, connection) = config.connect(connector).await?;

    Ok((Box::new(PostgresDatabase::new(client)), connection))
}

#[cfg(feature = "postgres-raw")]
#[allow(unused)]
pub async fn init_postgres_raw_no_tls() -> Result<
    (
        Box<dyn Database>,
        tokio_postgres::Connection<tokio_postgres::Socket, tokio_postgres::tls::NoTlsStream>,
    ),
    InitDatabaseError,
> {
    use moosicbox_database::postgres::postgres::PostgresDatabase;

    let (db_host, db_name, db_user, db_password) = get_db_config().await?;

    let mut config = tokio_postgres::Config::new();
    let mut config = config.host(&db_host).dbname(&db_name).user(&db_user);

    if let Some(ref db_password) = db_password {
        config = config.password(db_password);
    }

    let connector = tokio_postgres::NoTls;

    let (client, connection) = config.connect(connector).await?;

    Ok((Box::new(PostgresDatabase::new(client)), connection))
}
