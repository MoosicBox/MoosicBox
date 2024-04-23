use std::pin::Pin;

use actix_web::error::ErrorInternalServerError;
use chrono::NaiveDateTime;
use futures_util::Future;
use moosicbox_database::{
    boxed,
    query::{where_eq, where_gte, FilterableQuery},
    Database, DatabaseValue, Row,
};
use moosicbox_json_utils::{database::ToValue, MissingValue, ParseError, ToValueType};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;

impl From<DatabaseError> for actix_web::Error {
    fn from(value: DatabaseError) -> Self {
        log::error!("{value:?}");
        ErrorInternalServerError(value)
    }
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[cfg(feature = "postgres")]
    #[error(transparent)]
    InitDatabase(#[from] InitDatabaseError),
    #[error(transparent)]
    Db(#[from] moosicbox_database::DatabaseError),
    #[error(transparent)]
    Parse(#[from] moosicbox_json_utils::ParseError),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Connection {
    pub client_id: String,
    pub tunnel_ws_id: String,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
}

impl MissingValue<Connection> for &moosicbox_database::Row {}
impl ToValueType<Connection> for &Row {
    fn to_value_type(self) -> Result<Connection, ParseError> {
        Ok(Connection {
            client_id: self.to_value("client_id")?,
            tunnel_ws_id: self.to_value("tunnel_ws_id")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SignatureToken {
    pub token_hash: String,
    pub client_id: String,
    pub expires: NaiveDateTime,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
}

impl MissingValue<SignatureToken> for &moosicbox_database::Row {}
impl ToValueType<SignatureToken> for &Row {
    fn to_value_type(self) -> Result<SignatureToken, ParseError> {
        Ok(SignatureToken {
            token_hash: self.to_value("token_hash")?,
            client_id: self.to_value("client_id")?,
            expires: self.to_value("expires")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClientAccessToken {
    pub token_hash: String,
    pub client_id: String,
    pub expires: Option<NaiveDateTime>,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
}

impl MissingValue<ClientAccessToken> for &moosicbox_database::Row {}
impl ToValueType<ClientAccessToken> for &Row {
    fn to_value_type(self) -> Result<ClientAccessToken, ParseError> {
        Ok(ClientAccessToken {
            token_hash: self.to_value("token_hash")?,
            client_id: self.to_value("client_id")?,
            expires: self.to_value("expires")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MagicToken {
    pub magic_token_hash: String,
    pub client_id: String,
    pub expires: Option<NaiveDateTime>,
    pub created: NaiveDateTime,
    pub updated: NaiveDateTime,
}

impl MissingValue<MagicToken> for &moosicbox_database::Row {}
impl ToValueType<MagicToken> for &Row {
    fn to_value_type(self) -> Result<MagicToken, ParseError> {
        Ok(MagicToken {
            magic_token_hash: self.to_value("magic_token_hash")?,
            client_id: self.to_value("client_id")?,
            expires: self.to_value("expires")?,
            created: self.to_value("created")?,
            updated: self.to_value("updated")?,
        })
    }
}

pub(crate) static DB: Lazy<Mutex<Option<Box<dyn Database>>>> = Lazy::new(|| Mutex::new(None));

#[cfg(feature = "postgres-raw")]
pub(crate) static DB_CONNECTION: Lazy<
    Mutex<Option<tokio::task::JoinHandle<Result<(), tokio_postgres::Error>>>>,
> = Lazy::new(|| Mutex::new(None));

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

            let config = aws_config::defaults(BehaviorVersion::v2023_11_09())
                .region(Region::new("us-east-1"))
                .load()
                .await;

            let client = Client::new(&config);

            let ssm_db_name_param_name =
                std::env::var("SSM_DB_NAME_PARAM_NAME").unwrap_or("moosicbox_db_name".to_string());
            let ssm_db_host_param_name = std::env::var("SSM_DB_HOST_PARAM_NAME")
                .unwrap_or("moosicbox_db_hostname".to_string());
            let ssm_db_user_param_name =
                std::env::var("SSM_DB_USER_PARAM_NAME").unwrap_or("moosicbox_db_user".to_string());
            let ssm_db_password_param_name = std::env::var("SSM_DB_PASSWORD_PARAM_NAME")
                .unwrap_or("moosicbox_db_password".to_string());

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
                .expect("No db_password")
                .to_string();

            let password = if password.is_empty() {
                None
            } else {
                Some(password)
            };

            (
                params
                    .get(ssm_db_host_param_name)
                    .cloned()
                    .expect("No hostname")
                    .to_string(),
                params
                    .get(ssm_db_name_param_name)
                    .cloned()
                    .expect("No db_name")
                    .to_string(),
                params
                    .get(ssm_db_user_param_name)
                    .cloned()
                    .expect("No db_user")
                    .to_string(),
                password,
            )
        },
    )
}

#[cfg(feature = "postgres-sqlx")]
#[allow(unused)]
pub async fn init_postgres_sqlx() -> Result<(), InitDatabaseError> {
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

    DB.lock()
        .await
        .replace(Box::new(PostgresSqlxDatabase::new(Arc::new(
            tokio::sync::Mutex::new(pool),
        ))));

    Ok(())
}

#[cfg(feature = "postgres-raw")]
#[allow(unused)]
pub async fn init_postgres_raw() -> Result<(), InitDatabaseError> {
    #[cfg(feature = "postgres-openssl")]
    return init_postgres_raw_openssl().await;
    #[cfg(feature = "postgres-native-tls")]
    return init_postgres_raw_native_tls().await;
    #[cfg(all(
        not(feature = "postgres-openssl"),
        not(feature = "postgres-native-tls")
    ))]
    return init_postgres_raw_no_tls().await;
}

#[cfg(all(feature = "postgres-native-tls", feature = "postgres-raw"))]
#[allow(unused)]
pub async fn init_postgres_raw_native_tls() -> Result<(), InitDatabaseError> {
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

    {
        DB_CONNECTION.lock().await.take();
    }

    let (client, connection) = config.connect(connector).await?;

    DB.lock()
        .await
        .replace(Box::new(PostgresDatabase::new(client)));

    DB_CONNECTION
        .lock()
        .await
        .replace(tokio::spawn(async move { connection.await }));

    Ok(())
}

#[cfg(all(feature = "postgres-openssl", feature = "postgres-raw"))]
#[allow(unused)]
pub async fn init_postgres_raw_openssl() -> Result<(), InitDatabaseError> {
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

    {
        DB_CONNECTION.lock().await.take();
    }

    let (client, connection) = config.connect(connector).await?;

    DB.lock()
        .await
        .replace(Box::new(PostgresDatabase::new(client)));

    DB_CONNECTION
        .lock()
        .await
        .replace(tokio::spawn(async move { connection.await }));

    Ok(())
}

#[cfg(feature = "postgres-raw")]
#[allow(unused)]
pub async fn init_postgres_raw_no_tls() -> Result<(), InitDatabaseError> {
    use moosicbox_database::postgres::postgres::PostgresDatabase;

    let (db_host, db_name, db_user, db_password) = get_db_config().await?;

    let mut config = tokio_postgres::Config::new();
    let mut config = config.host(&db_host).dbname(&db_name).user(&db_user);

    if let Some(ref db_password) = db_password {
        config = config.password(db_password);
    }

    let connector = tokio_postgres::NoTls;

    {
        DB_CONNECTION.lock().await.take();
    }

    let (client, connection) = config.connect(connector).await?;

    DB.lock()
        .await
        .replace(Box::new(PostgresDatabase::new(client)));

    DB_CONNECTION
        .lock()
        .await
        .replace(tokio::spawn(async move { connection.await }));

    Ok(())
}

async fn resilient_exec<T, F>(
    exec: Box<dyn Fn() -> Pin<Box<F>> + Send + Sync>,
) -> Result<T, DatabaseError>
where
    F: Future<Output = Result<T, DatabaseError>> + Send + 'static,
{
    #[cfg(feature = "postgres-sqlx")]
    return resilient_exec_postgres_sqlx(exec).await;
    #[cfg(all(not(feature = "postgres-sqlx"), feature = "postgres-raw"))]
    return resilient_exec_postgres_raw(exec).await;
    #[cfg(all(not(feature = "postgres-sqlx"), not(feature = "postgres-raw")))]
    exec().await
}

#[cfg(feature = "postgres-sqlx")]
async fn resilient_exec_postgres_sqlx<T, F>(
    exec: Box<dyn Fn() -> Pin<Box<F>> + Send + Sync>,
) -> Result<T, DatabaseError>
where
    F: Future<Output = Result<T, DatabaseError>> + Send + 'static,
{
    #[allow(unused)]
    static MAX_RETRY: u8 = 3;
    #[allow(unused)]
    let mut retries = 0;
    loop {
        match exec().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                match err {
                    DatabaseError::Db(moosicbox_database::DatabaseError::PostgresSqlx(
                        ref postgres_err,
                    )) => match postgres_err {
                        moosicbox_database::sqlx::postgres::SqlxDatabaseError::Sqlx(
                            sqlx::Error::Io(_io_err),
                        ) => {
                            if retries >= MAX_RETRY {
                                return Err(err);
                            }
                            log::info!(
                                "Database IO error. Attempting reconnect... {}/{MAX_RETRY}",
                                retries + 1
                            );
                            if let Err(init_err) = init_postgres_sqlx().await {
                                log::error!("Failed to reinitialize: {init_err:?}");
                                return Err(init_err.into());
                            }
                            retries += 1;
                            continue;
                        }
                        _ => {}
                    },
                    _ => {}
                }
                return Err(err);
            }
        }
    }
}

#[cfg(all(not(feature = "postgres-sqlx"), feature = "postgres-raw"))]
async fn resilient_exec_postgres_raw<T, F>(
    exec: Box<dyn Fn() -> Pin<Box<F>> + Send + Sync>,
) -> Result<T, DatabaseError>
where
    F: Future<Output = Result<T, DatabaseError>> + Send + 'static,
{
    static MAX_RETRY: u8 = 3;
    let mut retries = 0;
    loop {
        match exec().await {
            Ok(value) => return Ok(value),
            Err(err) => {
                match err {
                    DatabaseError::Db(moosicbox_database::DatabaseError::Postgres(
                        ref postgres_err,
                    )) => match postgres_err {
                        moosicbox_database::postgres::postgres::PostgresDatabaseError::Postgres(
                            pg_err,
                        ) => {
                            if pg_err.to_string().as_str() == "connection closed" {
                                if retries >= MAX_RETRY {
                                    return Err(err);
                                }
                                log::info!(
                                    "Database connection closed. Attempting reconnect... {}/{MAX_RETRY}",
                                    retries + 1
                                );
                                if let Err(init_err) = init_postgres_raw().await {
                                    log::error!("Failed to reinitialize: {init_err:?}");
                                    return Err(init_err.into());
                                }
                                retries += 1;
                                continue;
                            }
                        }
                        _ => {}
                    },
                    _ => {}
                }
                return Err(err);
            }
        }
    }
}

pub async fn upsert_connection(client_id: &str, tunnel_ws_id: &str) -> Result<(), DatabaseError> {
    let client_id = client_id.to_owned();
    let tunnel_ws_id = tunnel_ws_id.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();
        let tunnel_ws_id = tunnel_ws_id.clone();

        Box::pin(async move {
            moosicbox_database::query::upsert("connections")
                .value("client_id", client_id.clone())
                .value("tunnel_ws_id", tunnel_ws_id.clone())
                .execute(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}

pub async fn select_connection(client_id: &str) -> Result<Option<Connection>, DatabaseError> {
    let client_id = client_id.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();

        Box::pin(async move {
            Ok(moosicbox_database::query::select("connections")
                .where_eq("client_id", client_id)
                .execute_first(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?
                .as_ref()
                .to_value_type()?)
        })
    }))
    .await
}

pub async fn delete_connection(tunnel_ws_id: &str) -> Result<(), DatabaseError> {
    let tunnel_ws_id = tunnel_ws_id.to_owned();

    resilient_exec(Box::new(move || {
        let tunnel_ws_id = tunnel_ws_id.clone();

        Box::pin(async move {
            moosicbox_database::query::delete("connections")
                .where_eq("tunnel_ws_id", tunnel_ws_id)
                .execute(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}

pub async fn insert_client_access_token(
    client_id: &str,
    token_hash: &str,
) -> Result<(), DatabaseError> {
    let client_id = client_id.to_owned();
    let token_hash = token_hash.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();
        let token_hash = token_hash.clone();

        Box::pin(async move {
            moosicbox_database::query::insert("client_access_tokens")
                .value("token_hash", token_hash)
                .value("client_id", client_id)
                .value("expires", DatabaseValue::Null)
                .execute(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}

pub async fn valid_client_access_token(
    client_id: &str,
    token_hash: &str,
) -> Result<bool, DatabaseError> {
    Ok(select_client_access_token(client_id, token_hash)
        .await?
        .is_some())
}

pub async fn select_client_access_token(
    client_id: &str,
    token_hash: &str,
) -> Result<Option<ClientAccessToken>, DatabaseError> {
    let client_id = client_id.to_owned();
    let token_hash = token_hash.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();
        let token_hash = token_hash.clone();

        Box::pin(async move {
            Ok(moosicbox_database::query::select("client_access_tokens")
                .where_eq("client_id", client_id)
                .where_eq("token_hash", token_hash)
                .where_or(boxed!(
                    where_eq("expires", DatabaseValue::Null),
                    where_gte("expires", DatabaseValue::Now)
                ))
                .execute_first(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?
                .as_ref()
                .to_value_type()?)
        })
    }))
    .await
}

pub async fn insert_magic_token(
    client_id: &str,
    magic_token_hash: &str,
) -> Result<(), DatabaseError> {
    let magic_token_hash = magic_token_hash.to_owned();
    let client_id = client_id.to_owned();

    resilient_exec(Box::new(move || {
        let magic_token_hash = magic_token_hash.clone();
        let client_id = client_id.clone();

        Box::pin(async move {
            moosicbox_database::query::insert("magic_tokens")
                .value("magic_token_hash", magic_token_hash)
                .value("client_id", client_id)
                .value("expires", DatabaseValue::Null)
                .execute(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}

pub async fn select_magic_token(token_hash: &str) -> Result<Option<MagicToken>, DatabaseError> {
    let token_hash = token_hash.to_owned();

    resilient_exec(Box::new(move || {
        let token_hash = token_hash.clone();

        Box::pin(async move {
            Ok(moosicbox_database::query::select("magic_tokens")
                .where_eq("magic_token_hash", token_hash)
                .where_or(boxed!(
                    where_eq("expires", DatabaseValue::Null),
                    where_gte("expires", DatabaseValue::Now)
                ))
                .execute_first(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?
                .as_ref()
                .to_value_type()?)
        })
    }))
    .await
}

pub async fn insert_signature_token(
    client_id: &str,
    token_hash: &str,
) -> Result<(), DatabaseError> {
    let token_hash = token_hash.to_owned();
    let client_id = client_id.to_owned();

    resilient_exec(Box::new(move || {
        let token_hash = token_hash.clone();
        let client_id = client_id.clone();

        Box::pin(async move {
            moosicbox_database::query::insert("signature_tokens")
                .value("token_hash", token_hash)
                .value("client_id", client_id)
                .value(
                    "expires",
                    DatabaseValue::NowAdd("INTERVAL '14 day'".to_string()),
                )
                .execute(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}

pub async fn valid_signature_token(
    client_id: &str,
    token_hash: &str,
) -> Result<bool, DatabaseError> {
    Ok(select_signature_token(client_id, token_hash)
        .await?
        .is_some())
}

pub async fn select_signature_token(
    client_id: &str,
    token_hash: &str,
) -> Result<Option<SignatureToken>, DatabaseError> {
    let client_id = client_id.to_owned();
    let token_hash = token_hash.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();
        let token_hash = token_hash.clone();

        Box::pin(async move {
            Ok(moosicbox_database::query::select("signature_tokens")
                .where_eq("client_id", client_id)
                .where_eq("token_hash", token_hash)
                .where_gte("expires", DatabaseValue::Now)
                .execute_first(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?
                .as_ref()
                .to_value_type()?)
        })
    }))
    .await
}

#[allow(dead_code)]
pub async fn select_signature_tokens(
    client_id: &str,
) -> Result<Vec<SignatureToken>, DatabaseError> {
    let client_id = client_id.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();

        Box::pin(async move {
            Ok(moosicbox_database::query::select("signature_tokens")
                .where_eq("client_id", client_id)
                .execute(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?
                .to_value_type()?)
        })
    }))
    .await
}

#[allow(dead_code)]
pub async fn delete_signature_token(token_hash: &str) -> Result<(), DatabaseError> {
    let token_hash = token_hash.to_owned();

    resilient_exec(Box::new(move || {
        let token_hash = token_hash.clone();

        Box::pin(async move {
            moosicbox_database::query::delete("signature_tokens")
                .where_eq("token_hash", token_hash)
                .execute(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}
