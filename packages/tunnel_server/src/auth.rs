use std::collections::BTreeMap;
use std::pin::Pin;
use std::sync::{LazyLock, Mutex};

use actix_web::dev::Payload;
use actix_web::error::ErrorUnauthorized;
use actix_web::http::header::HeaderValue;
use actix_web::{FromRequest, HttpRequest, http};
use futures_util::Future;
use futures_util::future::{Ready, err, ok};
use qstring::QString;
use sha2::{Digest, Sha256};

use crate::db::{DatabaseError, valid_client_access_token, valid_signature_token};

static TUNNEL_ACCESS_TOKEN: &str = std::env!("TUNNEL_ACCESS_TOKEN");

pub struct GeneralHeaderAuthorized;

impl FromRequest for GeneralHeaderAuthorized {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, actix_web::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        log::trace!("GeneralHeaderAuthorized from_request {}", req.path());
        if is_authorized(req) {
            ok(Self)
        } else {
            log::warn!(
                "Unauthorized GeneralHeaderAuthorized request to '{}'",
                req.path()
            );
            err(ErrorUnauthorized("Unauthorized"))
        }
    }
}

fn is_authorized(req: &HttpRequest) -> bool {
    if let Some(auth) = req.headers().get(http::header::AUTHORIZATION)
        && let Ok(auth) = auth.to_str()
    {
        let token = if auth.to_lowercase().starts_with("bearer") {
            auth[6..].trim_start()
        } else {
            auth
        };

        return token == TUNNEL_ACCESS_TOKEN;
    }

    false
}

pub struct ClientHeaderAuthorized;

impl FromRequest for ClientHeaderAuthorized {
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, actix_web::Error>>>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        log::trace!("ClientHeaderAuthorized from_request {}", req.path());
        let path = req.path().to_owned();
        let query_string = req.query_string().to_owned();
        let auth_header = req.headers().get(http::header::AUTHORIZATION).cloned();
        Box::pin(async move {
            match client_is_authorized(&query_string, auth_header).await {
                Ok(true) => return Ok(Self),
                Ok(false) => log::warn!("Unauthorized ClientHeaderAuthorized request to '{path}'"),
                Err(err) => log::error!("ClientHeaderAuthorized Database error: {err:?}"),
            }

            Err(ErrorUnauthorized("Unauthorized"))
        })
    }
}

async fn client_is_authorized(
    query_string: &str,
    auth_header: Option<HeaderValue>,
) -> Result<bool, DatabaseError> {
    let query: Vec<_> = QString::from(query_string).into();
    let client_id = query
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("clientId"))
        .map(|(_, value)| value);

    if let Some(client_id) = client_id {
        if let Some(auth) = auth_header {
            if let Ok(auth) = auth.to_str() {
                let token = if auth.to_lowercase().starts_with("bearer") {
                    auth[6..].trim_start()
                } else {
                    auth
                };

                let token_hash = &hash_token(token);
                return valid_client_access_token(client_id, token_hash).await;
            }

            log::debug!("UNAUTHORIZED: Invalid auth header");
        } else {
            log::debug!("UNAUTHORIZED: No auth header");
        }
    } else {
        log::debug!("UNAUTHORIZED: No client_id in query params");
    }

    Ok(false)
}

pub struct SignatureAuthorized;

impl FromRequest for SignatureAuthorized {
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, actix_web::Error>>>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        log::trace!("SignatureAuthorized from_request {}", req.path());
        let path = req.path().to_owned();
        let query_string = req.query_string().to_owned();
        Box::pin(async move {
            match is_signature_authorized(&query_string).await {
                Ok(true) => Ok(Self),
                Ok(false) => {
                    log::warn!("Unauthorized SignatureAuthorized request to '{path}'");
                    Err(ErrorUnauthorized("Unauthorized"))
                }
                Err(error) => {
                    log::error!(
                        "Unauthorized SignatureAuthorized request to '{path}', error: {error:?}"
                    );
                    Err(ErrorUnauthorized("Unauthorized"))
                }
            }
        })
    }
}

async fn is_signature_authorized(query_string: &str) -> Result<bool, DatabaseError> {
    let query: Vec<_> = QString::from(query_string).into();
    let client_id = query
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("clientId"))
        .map(|(_, value)| value);

    if let Some(client_id) = client_id {
        let signature = query
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case("signature"))
            .map(|(_, value)| value);

        if let Some(token) = signature {
            let token_hash = &hash_token(token);
            return valid_signature_token(client_id, token_hash).await;
        }
    }

    Ok(false)
}

static HASH_CACHE: LazyLock<Mutex<BTreeMap<String, String>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

pub fn hash_token(token: &str) -> String {
    if let Some(existing) = HASH_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(token)
    {
        return existing.clone();
    }

    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let token_hex = hasher.finalize();
    let hash = hex::encode(token_hex);

    HASH_CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(token.to_string(), hash.clone());

    hash
}
