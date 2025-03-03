#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

#[cfg(feature = "api")]
pub mod api;

mod db;

use actix_web::{FromRequest, HttpRequest, dev::Payload, error::ErrorUnauthorized, http};
use futures::future::{Ready, err, ok};
use moosicbox_database::config::ConfigDatabase;
use moosicbox_json_utils::{ParseError, database::DatabaseFetchError, serde_json::ToValue};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::db::{create_client_access_token, get_client_access_token};

#[derive(Debug, Error)]
pub enum AuthError {
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    #[error(transparent)]
    Parse(#[from] ParseError),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error("Failed to register client")]
    RegisterClient,
    #[error("Unauthorized")]
    Unauthorized,
}

#[cfg(feature = "api")]
pub(crate) async fn get_credentials_from_magic_token(
    db: &ConfigDatabase,
    magic_token: &str,
) -> Result<Option<(String, String)>, DatabaseFetchError> {
    if let Some((client_id, access_token)) =
        crate::db::get_credentials_from_magic_token(db, magic_token).await?
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
) -> Result<bool, AuthError> {
    let url =
        format!("{tunnel_host}/auth/magic-token?clientId={client_id}&magicToken={magic_token}");
    let value: Value = reqwest::Client::new()
        .post(url)
        .header(reqwest::header::AUTHORIZATION, access_token)
        .send()
        .await?
        .json()
        .await?;

    Ok(value
        .get("success")
        .is_some_and(|success| success.as_bool().unwrap_or(false)))
}

#[cfg(feature = "api")]
pub(crate) async fn create_magic_token(
    db: &ConfigDatabase,
    tunnel_host: Option<String>,
) -> Result<String, AuthError> {
    let magic_token = Uuid::new_v4().to_string();

    if let Some((client_id, access_token)) = { get_client_access_token(db).await? } {
        if let Some(tunnel_host) = tunnel_host {
            tunnel_magic_token(&tunnel_host, &client_id, &access_token, &magic_token).await?;
        }
        crate::db::save_magic_token(db, &magic_token, &client_id, &access_token).await?;
    }

    Ok(magic_token)
}

fn create_client_id() -> String {
    Uuid::new_v4().to_string()
}

/// # Errors
///
/// Will error if there is a database error
pub async fn get_client_id_and_access_token(
    db: &ConfigDatabase,
    host: &str,
) -> Result<(String, String), AuthError> {
    if let Ok(Some((client_id, token))) = get_client_access_token(db).await {
        Ok((client_id, token))
    } else {
        let client_id = create_client_id();

        let token = register_client(host, &client_id)
            .await?
            .ok_or(AuthError::RegisterClient)?;

        create_client_access_token(db, &client_id, &token).await?;

        Ok((client_id, token))
    }
}

async fn register_client(host: &str, client_id: &str) -> Result<Option<String>, AuthError> {
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
            ok(Self)
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

/// # Errors
///
/// * If the request is unauthorized
/// * If there was a generic http request error
/// * If there was an error parsing the json response
pub async fn fetch_signature_token(
    host: &str,
    client_id: &str,
    access_token: &str,
) -> Result<Option<String>, AuthError> {
    let url = format!("{host}/auth/signature-token?clientId={client_id}");

    log::debug!("Fetching signature token for client_id={client_id}");
    let response = reqwest::Client::new()
        .post(url)
        .header(reqwest::header::AUTHORIZATION, access_token)
        .send()
        .await?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AuthError::Unauthorized);
    }

    Ok(response.json::<Value>().await?.to_value("token")?)
}
