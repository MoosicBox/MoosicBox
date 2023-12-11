#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;

use actix::fut::{err, ok, Ready};
use actix_web::{dev::Payload, error::ErrorUnauthorized, http, FromRequest, HttpRequest};
use rusqlite::Connection;
use serde_json::Value;
use uuid::Uuid;

use moosicbox_core::sqlite::db::{create_client_access_token, get_client_access_token, DbError};

#[cfg(feature = "api")]
pub(crate) fn get_credentials_from_magic_token(
    db: &Connection,
    magic_token: &str,
) -> Result<Option<(String, String)>, DbError> {
    if let Some((client_id, access_token)) =
        moosicbox_core::sqlite::db::get_credentials_from_magic_token(db, magic_token)?
    {
        Ok(Some((client_id, access_token)))
    } else {
        Ok(None)
    }
}

#[cfg(feature = "api")]
async fn tunnel_magic_token(
    tunnel_host: &str,
    client_id: &str,
    access_token: &str,
    magic_token: &str,
) -> Result<bool, reqwest::Error> {
    let url =
        format!("{tunnel_host}/auth/magic-token?clientId={client_id}&magicToken={magic_token}");
    let value: Value = reqwest::Client::new()
        .post(url)
        .header(reqwest::header::AUTHORIZATION, access_token)
        .send()
        .await?
        .json()
        .await?;

    if let Some(success) = value.get("success") {
        Ok(success.as_bool().unwrap_or(false))
    } else {
        Ok(false)
    }
}

#[cfg(feature = "api")]
pub(crate) async fn create_magic_token(
    db: &moosicbox_core::app::Db,
    tunnel_host: Option<String>,
) -> Result<String, DbError> {
    let magic_token = Uuid::new_v4().to_string();

    if let Some((client_id, access_token)) = {
        let lock = db.library.lock();
        let db = lock.as_ref().unwrap();
        get_client_access_token(db)?
    } {
        if let Some(tunnel_host) = tunnel_host {
            if let Err(err) =
                tunnel_magic_token(&tunnel_host, &client_id, &access_token, &magic_token).await
            {
                log::error!("Failed to register magic token to the tunnel: {err:?}");
                return Err(DbError::Unknown);
            }
        }
        moosicbox_core::sqlite::db::save_magic_token(
            db.library.lock().as_ref().unwrap(),
            &magic_token,
            &client_id,
            &access_token,
        )?;
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

pub struct NonTunnelRequestAuthorized;

impl FromRequest for NonTunnelRequestAuthorized {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, actix_web::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        if is_authorized(req) {
            ok(NonTunnelRequestAuthorized)
        } else {
            log::warn!(
                "Unauthorized NonTunnelRequestAuthorized request to '{}'",
                req.path()
            );
            err(ErrorUnauthorized("Unauthorized"))
        }
    }
}

fn is_authorized(req: &HttpRequest) -> bool {
    if let Some(user_agent) = req.headers().get(http::header::USER_AGENT) {
        if let Ok(user_agent) = user_agent.to_str() {
            return user_agent != "MOOSICBOX_TUNNEL";
        }
    }

    true
}

pub async fn fetch_signature_token(
    host: &str,
    client_id: &str,
    access_token: &str,
) -> Result<Option<String>, reqwest::Error> {
    let url = format!("{host}/auth/signature-token?clientId={client_id}");
    let value: Value = reqwest::Client::new()
        .post(url)
        .header(reqwest::header::AUTHORIZATION, access_token)
        .send()
        .await?
        .json()
        .await?;

    if let Some(token) = value.get("token") {
        Ok(token.as_str().map(|s| Some(s.to_string())).unwrap_or(None))
    } else {
        Ok(None)
    }
}
