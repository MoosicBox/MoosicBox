use std::collections::HashMap;
use std::sync::Mutex;

use actix_web::dev::Payload;
use actix_web::error::ErrorUnauthorized;
use actix_web::{http, FromRequest, HttpRequest};
use futures_util::future::{err, ok, Ready};
use once_cell::sync::Lazy;
use qstring::QString;
use sha2::{Digest, Sha256};

use crate::ws::db::{valid_client_access_token, valid_signature_token};

static TUNNEL_ACCESS_TOKEN: &str = std::env!("TUNNEL_ACCESS_TOKEN");

pub struct GeneralHeaderAuthorized;

impl FromRequest for GeneralHeaderAuthorized {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, actix_web::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        if is_authorized(req) {
            ok(GeneralHeaderAuthorized)
        } else {
            err(ErrorUnauthorized("Unauthorized"))
        }
    }
}

fn is_authorized(req: &HttpRequest) -> bool {
    if let Some(auth) = req.headers().get(http::header::AUTHORIZATION) {
        if let Ok(auth) = auth.to_str() {
            let token = if auth.to_lowercase().starts_with("bearer") {
                auth[6..].trim_start()
            } else {
                auth
            };

            return token == TUNNEL_ACCESS_TOKEN;
        }
    }

    false
}

pub struct ClientHeaderAuthorized;

impl FromRequest for ClientHeaderAuthorized {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, actix_web::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        if client_is_authorized(req) {
            ok(ClientHeaderAuthorized)
        } else {
            err(ErrorUnauthorized("Unauthorized"))
        }
    }
}

fn client_is_authorized(req: &HttpRequest) -> bool {
    let query: Vec<_> = QString::from(req.query_string()).into();
    let client_id = query
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("clientId"))
        .map(|(_, value)| value);

    if let Some(client_id) = client_id {
        if let Some(auth) = req.headers().get(http::header::AUTHORIZATION) {
            if let Ok(auth) = auth.to_str() {
                let token = if auth.to_lowercase().starts_with("bearer") {
                    auth[6..].trim_start()
                } else {
                    auth
                };

                let token_hash = &hash_token(token);
                return valid_client_access_token(client_id, token_hash);
            }
        }
    }

    false
}

pub struct QueryAuthorized;

impl FromRequest for QueryAuthorized {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, actix_web::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        if is_query_authorized(req) {
            ok(QueryAuthorized)
        } else {
            err(ErrorUnauthorized("Unauthorized"))
        }
    }
}

fn is_query_authorized(req: &HttpRequest) -> bool {
    let query: Vec<_> = QString::from(req.query_string()).into();
    let query: HashMap<_, _> = query.into_iter().collect();
    let authorization = query
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(http::header::AUTHORIZATION.as_str()))
        .map(|(_, value)| value);

    if let Some(token) = authorization {
        return token == TUNNEL_ACCESS_TOKEN;
    }

    false
}

pub struct SignatureAuthorized;

impl FromRequest for SignatureAuthorized {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, actix_web::Error>>;

    fn from_request(req: &HttpRequest, _: &mut Payload) -> Self::Future {
        if is_signature_authorized(req) {
            ok(SignatureAuthorized)
        } else {
            err(ErrorUnauthorized("Unauthorized"))
        }
    }
}

fn is_signature_authorized(req: &HttpRequest) -> bool {
    let query: Vec<_> = QString::from(req.query_string()).into();
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
            return valid_signature_token(client_id, token_hash);
        }
    }

    false
}

static HASH_CACHE: Lazy<Mutex<HashMap<String, String>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub fn hash_token(token: &str) -> String {
    if let Some(existing) = HASH_CACHE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
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
        .unwrap_or_else(|e| e.into_inner())
        .insert(token.to_string(), hash.clone());

    hash
}
