#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

#[cfg(feature = "api")]
pub mod api;

use actix::fut::{err, ok, Ready};
use actix_web::{dev::Payload, error::ErrorUnauthorized, http, FromRequest, HttpRequest};
use moosicbox_database::config::ConfigDatabase;
use moosicbox_json_utils::{serde_json::ToValue, ParseError};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use moosicbox_core::sqlite::db::{create_client_access_token, get_client_access_token, DbError};

#[cfg(feature = "api")]
pub(crate) async fn get_credentials_from_magic_token(
    db: &ConfigDatabase,
    magic_token: &str,
) -> Result<Option<(String, String)>, DbError> {
    if let Some((client_id, access_token)) =
        moosicbox_core::sqlite::db::get_credentials_from_magic_token(db, magic_token).await?
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
    db: &ConfigDatabase,
    tunnel_host: Option<String>,
) -> Result<String, DbError> {
    let magic_token = Uuid::new_v4().to_string();

    if let Some((client_id, access_token)) = { get_client_access_token(db).await? } {
        if let Some(tunnel_host) = tunnel_host {
            if let Err(err) =
                tunnel_magic_token(&tunnel_host, &client_id, &access_token, &magic_token).await
            {
                log::error!("Failed to register magic token to the tunnel: {err:?}");
                return Err(DbError::Unknown);
            }
        }
        moosicbox_core::sqlite::db::save_magic_token(db, &magic_token, &client_id, &access_token)
            .await?;
    }

    Ok(magic_token)
}

fn create_client_id() -> String {
    Uuid::new_v4().to_string()
}

pub async fn get_client_id_and_access_token(
    db: &ConfigDatabase,
    host: &str,
) -> Result<(String, String), DbError> {
    if let Ok(Some((client_id, token))) = get_client_access_token(db).await {
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

        create_client_access_token(db, &client_id, &token).await?;

        Ok((client_id, token))
    }
}

#[derive(Debug, Error)]
pub enum RegisterClientError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
}

async fn register_client(
    host: &str,
    client_id: &str,
) -> Result<Option<String>, RegisterClientError> {
    let url = format!("{host}/auth/register-client?clientId={client_id}");

    Ok(reqwest::Client::new()
        .post(url)
        .header(
            reqwest::header::AUTHORIZATION,
            std::env::var("TUNNEL_ACCESS_TOKEN").expect("TUNNEL_ACCESS_TOKEN not set"),
        )
        .send()
        .await?
        .json::<Value>()
        .await?
        .to_value("token")?)
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

#[derive(Debug, Error)]
pub enum FetchSignatureError {
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error("Unauthorized")]
    Unauthorized,
}

pub async fn fetch_signature_token(
    host: &str,
    client_id: &str,
    access_token: &str,
) -> Result<Option<String>, FetchSignatureError> {
    let url = format!("{host}/auth/signature-token?clientId={client_id}");

    log::debug!("Fetching signature token for client_id={client_id}");
    let response = reqwest::Client::new()
        .post(url)
        .header(reqwest::header::AUTHORIZATION, access_token)
        .send()
        .await?;

    if let reqwest::StatusCode::UNAUTHORIZED = response.status() {
        return Err(FetchSignatureError::Unauthorized);
    }

    Ok(response.json::<Value>().await?.to_value("token")?)
}
