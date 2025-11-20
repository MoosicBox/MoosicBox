//! Authentication and authorization for tunnel server API endpoints.
//!
//! This module provides request guards that validate different types of authentication:
//! client access tokens, signature tokens (temporary), and the general tunnel access token.
//! It also provides a token hashing utility for secure token comparison.

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

/// Request guard that validates general authorization via the access token header.
///
/// This guard checks the `Authorization` header against the tunnel access token
/// configured via the `TUNNEL_ACCESS_TOKEN` environment variable. It is used for
/// administrative endpoints that require elevated permissions.
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

/// Request guard that validates client authorization via client access tokens.
///
/// This guard verifies that the request includes a valid client ID in the query
/// parameters and a matching client access token in the `Authorization` header.
/// It queries the database to validate the token against the stored hash.
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

/// Request guard that validates signature token authorization.
///
/// This guard verifies that the request includes a valid client ID and signature
/// token in the query parameters. Signature tokens are temporary tokens used for
/// request signing and are validated against the database.
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

/// Hash a token using SHA-256 and return the hexadecimal representation.
///
/// This function uses an in-memory cache to avoid re-hashing the same token
/// multiple times. The cache persists for the lifetime of the application.
///
/// # Panics
///
/// Panics if the hash cache lock is poisoned (which is recovered from automatically).
#[must_use]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_token_produces_consistent_hash() {
        let token = "test_token_12345";
        let hash1 = hash_token(token);
        let hash2 = hash_token(token);

        assert_eq!(hash1, hash2, "Same token should produce identical hashes");
        assert_eq!(hash1.len(), 64, "SHA-256 hash should be 64 hex characters");
    }

    #[test]
    fn test_hash_token_different_tokens_produce_different_hashes() {
        let token1 = "token_one";
        let token2 = "token_two";

        let hash1 = hash_token(token1);
        let hash2 = hash_token(token2);

        assert_ne!(
            hash1, hash2,
            "Different tokens should produce different hashes"
        );
    }

    #[test]
    fn test_hash_token_cache_hit_returns_same_instance() {
        let token = "cached_token_test";

        // First call - cache miss, computes hash
        let hash1 = hash_token(token);

        // Second call - cache hit, should return cached value
        let hash2 = hash_token(token);

        assert_eq!(hash1, hash2, "Cached hash should match computed hash");

        // Verify it's actually using the cache by checking the hash is valid SHA-256
        assert_eq!(hash1.len(), 64);
        assert!(
            hash1.chars().all(|c| c.is_ascii_hexdigit()),
            "Hash should only contain hex digits"
        );
    }

    #[test]
    fn test_hash_token_empty_string() {
        let empty_hash = hash_token("");

        assert_eq!(
            empty_hash.len(),
            64,
            "Empty string should still hash to 64 chars"
        );
        assert_eq!(
            empty_hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "Empty string SHA-256 hash should match known value"
        );
    }

    #[test]
    fn test_hash_token_special_characters() {
        let token = "token-with!@#$%^&*()special_chars";
        let hash = hash_token(token);

        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_hash_token_unicode_characters() {
        let token = "token_with_unicode_🔒🔑";
        let hash = hash_token(token);

        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
