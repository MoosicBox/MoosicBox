#![allow(clippy::module_name_repetitions, clippy::struct_field_names)]

use std::{pin::Pin, sync::LazyLock};

use actix_web::error::ErrorInternalServerError;
use chrono::NaiveDateTime;
use futures_util::Future;
use moosicbox_database::{
    boxed,
    query::{where_eq, where_gte, FilterableQuery},
    Database, DatabaseValue, Row,
};
use moosicbox_database_connection::InitDbError;
use moosicbox_json_utils::{database::ToValue, MissingValue, ParseError, ToValueType};
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
    #[error(transparent)]
    InitDb(#[from] InitDbError),
    #[error(transparent)]
    Db(#[from] moosicbox_database::DatabaseError),
    #[error(transparent)]
    Parse(#[from] moosicbox_json_utils::ParseError),
}

#[allow(clippy::significant_drop_tightening)]
pub async fn init() -> Result<(), DatabaseError> {
    #[allow(unused_mut)]
    let mut binding = DB.lock().await;
    let db: Option<&Box<dyn Database>> = binding.as_ref();

    if let Some(db) = db {
        db.close().await?;
    }

    #[cfg(feature = "postgres")]
    let creds = Some(
        moosicbox_database_connection::creds::get_db_creds()
            .await
            .expect("Failed to get DB creds"),
    );
    #[cfg(all(not(feature = "postgres"), not(feature = "sqlite")))]
    let creds = None;

    #[cfg(feature = "sqlite")]
    unimplemented!("sqlite database is not implemented");

    #[cfg(not(feature = "sqlite"))]
    {
        binding.replace(moosicbox_database_connection::init_default_non_sqlite(creds).await?);

        Ok(())
    }
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

pub static DB: LazyLock<Mutex<Option<Box<dyn Database>>>> = LazyLock::new(|| Mutex::new(None));

async fn resilient_exec<T: Send, F>(
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
                if let DatabaseError::Db(ref db_err) = err {
                    if db_err.is_connection_error() {
                        if retries >= MAX_RETRY {
                            return Err(err);
                        }
                        log::info!(
                            "Database IO error. Attempting reconnect... {}/{MAX_RETRY}",
                            retries + 1
                        );
                        if let Err(init_err) = init().await {
                            log::error!("Failed to reinitialize: {init_err:?}");
                            return Err(init_err);
                        }
                        retries += 1;
                        continue;
                    }
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
    log::debug!("delete_connection: tunnel_ws_id={tunnel_ws_id}");

    let tunnel_ws_id = tunnel_ws_id.to_owned();

    resilient_exec(Box::new(move || {
        let tunnel_ws_id = tunnel_ws_id.clone();

        Box::pin(async move {
            let deleted = moosicbox_database::query::delete("connections")
                .where_eq("tunnel_ws_id", tunnel_ws_id)
                .execute(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            log::debug!("delete_connection: deleted={deleted:?}");

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
