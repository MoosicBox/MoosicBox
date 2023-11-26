#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;

use rusqlite::Connection;
use serde_json::Value;
use uuid::Uuid;

use moosicbox_core::sqlite::db::{
    create_client_access_token, get_client_access_token, save_magic_token, DbError,
};

pub(crate) fn get_credentials_from_magic_token(
    db: &Connection,
    magic_token: &str,
) -> Result<Option<(String, String)>, DbError> {
    if let Some((client_id, access_token)) =
        moosicbox_core::sqlite::db::get_credentials_from_magic_token(db, &magic_token)?
    {
        Ok(Some((client_id, access_token)))
    } else {
        Ok(None)
    }
}

pub(crate) fn create_magic_token(db: &Connection) -> Result<String, DbError> {
    let magic_token = Uuid::new_v4().to_string();

    if let Some((client_id, access_token)) = get_client_access_token(db)? {
        save_magic_token(db, &magic_token, &client_id, &access_token)?;
    }

    Ok(magic_token)
}

fn create_client_id() -> String {
    Uuid::new_v4().to_string()
}

pub async fn get_client_id_and_access_token(
    db: &Connection,
    host: &str,
) -> Result<(String, String), DbError> {
    if let Ok(Some((client_id, token))) = get_client_access_token(db) {
        Ok((client_id, token))
    } else {
        let client_id = create_client_id();

        let token = match register_client(host, &client_id)
            .await
            .map_err(|_| DbError::Unknown)?
        {
            Some(token) => Ok(token),
            None => Err(DbError::Unknown),
        }?;

        create_client_access_token(db, &client_id, &token)?;

        Ok((client_id, token))
    }
}

async fn register_client(host: &str, client_id: &str) -> Result<Option<String>, reqwest::Error> {
    let url = format!("{host}/auth/register-client?clientId={client_id}");
    let value: Value = reqwest::Client::new()
        .post(url)
        .header(
            reqwest::header::AUTHORIZATION,
            std::env::var("TUNNEL_ACCESS_TOKEN").expect("TUNNEL_ACCESS_TOKEN not set"),
        )
        .send()
        .await
        .unwrap()
        .json()
        .await?;

    if let Some(token) = value.get("token") {
        Ok(token.as_str().map(|s| Some(s.to_string())).unwrap_or(None))
    } else {
        Ok(None)
    }
}
