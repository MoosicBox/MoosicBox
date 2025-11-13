//! Database layer for tunnel server persistent storage.
//!
//! This module provides database operations for storing and retrieving tunnel connection
//! data, authentication tokens (client access tokens, signature tokens, magic tokens),
//! and client-to-connection mappings. It includes automatic reconnection logic for
//! handling transient database connection failures.

#![allow(clippy::module_name_repetitions, clippy::struct_field_names)]

use std::{pin::Pin, sync::LazyLock};

use actix_web::error::ErrorInternalServerError;
use chrono::NaiveDateTime;
use futures_util::Future;
use moosicbox_json_utils::{MissingValue, ParseError, ToValueType, database::ToValue};
use serde::{Deserialize, Serialize};
use switchy_database::{
    Database, DatabaseValue, Row, boxed,
    query::{FilterableQuery, where_eq, where_gte},
};
use switchy_database_connection::InitDbError;
use thiserror::Error;
use tokio::sync::Mutex;

impl From<DatabaseError> for actix_web::Error {
    fn from(value: DatabaseError) -> Self {
        log::error!("{value:?}");
        ErrorInternalServerError(value)
    }
}

/// Errors that can occur during database operations.
#[derive(Debug, Error)]
pub enum DatabaseError {
    /// Failed to initialize the database connection.
    #[error(transparent)]
    InitDb(#[from] InitDbError),
    /// Database query or operation failed.
    #[error(transparent)]
    Db(#[from] switchy_database::DatabaseError),
    /// Failed to parse database row into a struct.
    #[error(transparent)]
    Parse(#[from] moosicbox_json_utils::ParseError),
}

/// Initialize the database connection.
///
/// This function must be called once at application startup before any database
/// operations are performed. It closes any existing connection and establishes
/// a new one based on the configured database type.
///
/// # Errors
///
/// * [`DatabaseError::InitDb`] - Failed to initialize the database connection.
///
/// # Panics
///
/// * Panics if `SQLite` feature is enabled (not yet implemented).
/// * Panics if database credentials cannot be retrieved (when postgres feature is enabled).
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
        switchy_database_connection::creds::get_db_creds()
            .await
            .expect("Failed to get DB creds"),
    );
    #[cfg(all(not(feature = "postgres"), not(feature = "sqlite")))]
    let creds = None;

    #[cfg(feature = "sqlite")]
    unimplemented!("sqlite database is not implemented");

    #[cfg(not(feature = "sqlite"))]
    {
        binding.replace(switchy_database_connection::init_default_non_sqlite(creds).await?);

        Ok(())
    }
}

/// Database record representing an active tunnel connection.
///
/// This struct maps a client ID to its WebSocket connection ID in the database.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Connection {
    /// The unique identifier for the client.
    pub client_id: String,
    /// The WebSocket connection ID for this client.
    pub tunnel_ws_id: String,
    /// Timestamp when the connection was first created.
    pub created: NaiveDateTime,
    /// Timestamp when the connection was last updated.
    pub updated: NaiveDateTime,
}

impl MissingValue<Connection> for &switchy_database::Row {}
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

/// Database record for a signature token.
///
/// Signature tokens are temporary tokens used for request signing. They expire
/// after a configured duration (typically 14 days).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SignatureToken {
    /// SHA-256 hash of the signature token.
    pub token_hash: String,
    /// The client ID this token is associated with.
    pub client_id: String,
    /// Timestamp when the token expires.
    pub expires: NaiveDateTime,
    /// Timestamp when the token was created.
    pub created: NaiveDateTime,
    /// Timestamp when the token was last updated.
    pub updated: NaiveDateTime,
}

impl MissingValue<SignatureToken> for &switchy_database::Row {}
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

/// Database record for a client access token.
///
/// Client access tokens are long-lived tokens used for client authentication.
/// They may optionally expire, or remain valid indefinitely if expires is None.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ClientAccessToken {
    /// SHA-256 hash of the access token.
    pub token_hash: String,
    /// The client ID this token is associated with.
    pub client_id: String,
    /// Optional expiration timestamp. If None, the token never expires.
    pub expires: Option<NaiveDateTime>,
    /// Timestamp when the token was created.
    pub created: NaiveDateTime,
    /// Timestamp when the token was last updated.
    pub updated: NaiveDateTime,
}

impl MissingValue<ClientAccessToken> for &switchy_database::Row {}
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

/// Database record for a magic token.
///
/// Magic tokens provide a temporary authentication mechanism for one-time use
/// or short-term access. They may optionally expire.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MagicToken {
    /// SHA-256 hash of the magic token.
    pub magic_token_hash: String,
    /// The client ID this token is associated with.
    pub client_id: String,
    /// Optional expiration timestamp. If None, the token never expires.
    pub expires: Option<NaiveDateTime>,
    /// Timestamp when the token was created.
    pub created: NaiveDateTime,
    /// Timestamp when the token was last updated.
    pub updated: NaiveDateTime,
}

impl MissingValue<MagicToken> for &switchy_database::Row {}
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

/// Global database connection handle.
///
/// This static provides thread-safe access to the database connection. It is
/// initialized once at startup via the `init` function and reused throughout
/// the application lifetime.
///
/// # Panics
///
/// Most database operations will panic with "DB not initialized" if accessed
/// before calling `init`.
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
                if let DatabaseError::Db(db_err) = &err
                    && db_err.is_connection_error()
                {
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
                return Err(err);
            }
        }
    }
}

/// Insert or update a connection record in the database.
///
/// Associates a client ID with a WebSocket connection ID. If a connection already
/// exists for the client ID, it is updated with the new WebSocket ID.
///
/// # Errors
///
/// * [`DatabaseError::Db`] - Database operation failed.
/// * [`DatabaseError::InitDb`] - Database reconnection failed after I/O error.
///
/// # Panics
///
/// Panics if the database has not been initialized.
pub async fn upsert_connection(client_id: &str, tunnel_ws_id: &str) -> Result<(), DatabaseError> {
    let client_id = client_id.to_owned();
    let tunnel_ws_id = tunnel_ws_id.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();
        let tunnel_ws_id = tunnel_ws_id.clone();

        Box::pin(async move {
            switchy_database::query::upsert("connections")
                .value("client_id", client_id.clone())
                .value("tunnel_ws_id", tunnel_ws_id.clone())
                .execute(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}

/// Retrieve a connection record from the database by client ID.
///
/// # Errors
///
/// * [`DatabaseError::Db`] - Database query failed.
/// * [`DatabaseError::Parse`] - Failed to parse database row.
/// * [`DatabaseError::InitDb`] - Database reconnection failed after I/O error.
///
/// # Panics
///
/// Panics if the database has not been initialized.
pub async fn select_connection(client_id: &str) -> Result<Option<Connection>, DatabaseError> {
    let client_id = client_id.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();

        Box::pin(async move {
            Ok(switchy_database::query::select("connections")
                .where_eq("client_id", client_id)
                .execute_first(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?
                .as_ref()
                .to_value_type()?)
        })
    }))
    .await
}

/// Delete a connection record from the database by WebSocket connection ID.
///
/// # Errors
///
/// * [`DatabaseError::Db`] - Database operation failed.
/// * [`DatabaseError::InitDb`] - Database reconnection failed after I/O error.
///
/// # Panics
///
/// Panics if the database has not been initialized.
pub async fn delete_connection(tunnel_ws_id: &str) -> Result<(), DatabaseError> {
    log::debug!("delete_connection: tunnel_ws_id={tunnel_ws_id}");

    let tunnel_ws_id = tunnel_ws_id.to_owned();

    resilient_exec(Box::new(move || {
        let tunnel_ws_id = tunnel_ws_id.clone();

        Box::pin(async move {
            let deleted = switchy_database::query::delete("connections")
                .where_eq("tunnel_ws_id", tunnel_ws_id)
                .execute(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            log::debug!("delete_connection: deleted={deleted:?}");

            Ok(())
        })
    }))
    .await
}

/// Insert a new client access token into the database.
///
/// Creates a non-expiring client access token for the specified client ID.
///
/// # Errors
///
/// * [`DatabaseError::Db`] - Database operation failed.
/// * [`DatabaseError::InitDb`] - Database reconnection failed after I/O error.
///
/// # Panics
///
/// Panics if the database has not been initialized.
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
            switchy_database::query::insert("client_access_tokens")
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

/// Check if a client access token is valid for the given client ID.
///
/// Returns `true` if a matching non-expired token exists in the database.
///
/// # Errors
///
/// * [`DatabaseError::Db`] - Database query failed.
/// * [`DatabaseError::Parse`] - Failed to parse database row.
/// * [`DatabaseError::InitDb`] - Database reconnection failed after I/O error.
///
/// # Panics
///
/// Panics if the database has not been initialized.
pub async fn valid_client_access_token(
    client_id: &str,
    token_hash: &str,
) -> Result<bool, DatabaseError> {
    Ok(select_client_access_token(client_id, token_hash)
        .await?
        .is_some())
}

/// Retrieve a client access token from the database.
///
/// Queries for a non-expired token matching the client ID and token hash.
///
/// # Errors
///
/// * [`DatabaseError::Db`] - Database query failed.
/// * [`DatabaseError::Parse`] - Failed to parse database row.
/// * [`DatabaseError::InitDb`] - Database reconnection failed after I/O error.
///
/// # Panics
///
/// Panics if the database has not been initialized.
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
            Ok(switchy_database::query::select("client_access_tokens")
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

/// Insert a new magic token into the database.
///
/// Creates a non-expiring magic token for the specified client ID.
///
/// # Errors
///
/// * [`DatabaseError::Db`] - Database operation failed.
/// * [`DatabaseError::InitDb`] - Database reconnection failed after I/O error.
///
/// # Panics
///
/// Panics if the database has not been initialized.
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
            switchy_database::query::insert("magic_tokens")
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

/// Retrieve a magic token from the database by its hash.
///
/// Queries for a non-expired magic token matching the provided hash.
///
/// # Errors
///
/// * [`DatabaseError::Db`] - Database query failed.
/// * [`DatabaseError::Parse`] - Failed to parse database row.
/// * [`DatabaseError::InitDb`] - Database reconnection failed after I/O error.
///
/// # Panics
///
/// Panics if the database has not been initialized.
pub async fn select_magic_token(token_hash: &str) -> Result<Option<MagicToken>, DatabaseError> {
    let token_hash = token_hash.to_owned();

    resilient_exec(Box::new(move || {
        let token_hash = token_hash.clone();

        Box::pin(async move {
            Ok(switchy_database::query::select("magic_tokens")
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

/// Insert a new signature token into the database.
///
/// Creates a signature token for the specified client ID with a 14-day expiration.
///
/// # Errors
///
/// * [`DatabaseError::Db`] - Database operation failed.
/// * [`DatabaseError::InitDb`] - Database reconnection failed after I/O error.
///
/// # Panics
///
/// Panics if the database has not been initialized.
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
            switchy_database::query::insert("signature_tokens")
                .value("token_hash", token_hash)
                .value("client_id", client_id)
                .value("expires", DatabaseValue::now().plus_days(14))
                .execute(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}

/// Check if a signature token is valid for the given client ID.
///
/// Returns `true` if a matching non-expired token exists in the database.
///
/// # Errors
///
/// * [`DatabaseError::Db`] - Database query failed.
/// * [`DatabaseError::Parse`] - Failed to parse database row.
/// * [`DatabaseError::InitDb`] - Database reconnection failed after I/O error.
///
/// # Panics
///
/// Panics if the database has not been initialized.
pub async fn valid_signature_token(
    client_id: &str,
    token_hash: &str,
) -> Result<bool, DatabaseError> {
    Ok(select_signature_token(client_id, token_hash)
        .await?
        .is_some())
}

/// Retrieve a signature token from the database.
///
/// Queries for a non-expired token matching the client ID and token hash.
///
/// # Errors
///
/// * [`DatabaseError::Db`] - Database query failed.
/// * [`DatabaseError::Parse`] - Failed to parse database row.
/// * [`DatabaseError::InitDb`] - Database reconnection failed after I/O error.
///
/// # Panics
///
/// Panics if the database has not been initialized.
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
            Ok(switchy_database::query::select("signature_tokens")
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

/// Retrieve all signature tokens for a given client ID.
///
/// Returns all signature tokens (both expired and non-expired) for the client.
///
/// # Errors
///
/// * [`DatabaseError::Db`] - Database query failed.
/// * [`DatabaseError::Parse`] - Failed to parse database rows.
/// * [`DatabaseError::InitDb`] - Database reconnection failed after I/O error.
///
/// # Panics
///
/// Panics if the database has not been initialized.
#[allow(dead_code)]
pub async fn select_signature_tokens(
    client_id: &str,
) -> Result<Vec<SignatureToken>, DatabaseError> {
    let client_id = client_id.to_owned();

    resilient_exec(Box::new(move || {
        let client_id = client_id.clone();

        Box::pin(async move {
            Ok(switchy_database::query::select("signature_tokens")
                .where_eq("client_id", client_id)
                .execute(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?
                .to_value_type()?)
        })
    }))
    .await
}

/// Delete a signature token from the database by its hash.
///
/// # Errors
///
/// * [`DatabaseError::Db`] - Database operation failed.
/// * [`DatabaseError::InitDb`] - Database reconnection failed after I/O error.
///
/// # Panics
///
/// Panics if the database has not been initialized.
#[allow(dead_code)]
pub async fn delete_signature_token(token_hash: &str) -> Result<(), DatabaseError> {
    let token_hash = token_hash.to_owned();

    resilient_exec(Box::new(move || {
        let token_hash = token_hash.clone();

        Box::pin(async move {
            switchy_database::query::delete("signature_tokens")
                .where_eq("token_hash", token_hash)
                .execute(&**DB.lock().await.as_mut().expect("DB not initialized"))
                .await?;

            Ok(())
        })
    }))
    .await
}
