//! Authentication and authorization for `MoosicBox` applications.
//!
//! This crate provides client authentication using access tokens and magic tokens for secure,
//! temporary credential exchange. It supports client registration, token management, and
//! request authorization guards for Actix-web applications.
//!
//! # Features
//!
//! * Client ID and access token generation and storage
//! * Magic token creation and exchange for temporary authentication
//! * Actix-web request guards for authorization
//! * Optional tunnel integration for distributed authentication
//! * `OpenAPI` documentation support
//!
//! # Main Entry Points
//!
//! * [`get_client_id_and_access_token`] - Obtain or create client credentials
//! * [`fetch_signature_token`] - Get a signature token from the auth server
//! * [`NonTunnelRequestAuthorized`] - Request guard to block tunnel requests
//! * [`api::bind_services`] - Register authentication API endpoints (requires `api` feature)
//!
//! # Example
//!
//! ```rust,no_run
//! # use moosicbox_auth::{get_client_id_and_access_token, fetch_signature_token};
//! # use switchy_database::config::ConfigDatabase;
//! # async fn example(db: &ConfigDatabase) -> Result<(), Box<dyn std::error::Error>> {
//! // Get or create client credentials
//! let (client_id, access_token) = get_client_id_and_access_token(
//!     db,
//!     "https://api.example.com"
//! ).await?;
//!
//! // Fetch a signature token for signing requests
//! let signature_token = fetch_signature_token(
//!     "https://api.example.com",
//!     &client_id,
//!     &access_token
//! ).await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "api")]
pub mod api;

mod db;

use actix_web::{FromRequest, HttpRequest, dev::Payload, error::ErrorUnauthorized, http};
use futures::future::{Ready, err, ok};
use moosicbox_json_utils::{ParseError, database::DatabaseFetchError, serde_json::ToValue};
use serde_json::Value;
use switchy_database::config::ConfigDatabase;
use switchy_uuid::new_v4_string;
use thiserror::Error;

use crate::db::{create_client_access_token, get_client_access_token};

/// Authentication errors that can occur during auth operations.
#[derive(Debug, Error)]
pub enum AuthError {
    /// Database fetch operation failed.
    #[error(transparent)]
    DatabaseFetch(#[from] DatabaseFetchError),
    /// Failed to parse data.
    #[error(transparent)]
    Parse(#[from] ParseError),
    /// HTTP request failed.
    #[error(transparent)]
    Http(#[from] switchy_http::Error),
    /// Client registration failed.
    #[error("Failed to register client")]
    RegisterClient,
    /// Request was unauthorized.
    #[error("Unauthorized")]
    Unauthorized,
}

/// Retrieves credentials from a magic token (public crate wrapper).
///
/// This is a thin wrapper around the database function for retrieving
/// credentials from a magic token.
///
/// # Errors
///
/// * Database query fails
/// * Token deletion fails
/// * Credential data cannot be parsed
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

/// Sends a magic token to the tunnel server for synchronization.
///
/// Registers the magic token with the tunnel server so it can be used
/// across distributed authentication endpoints.
///
/// # Errors
///
/// * HTTP request to the tunnel server fails
/// * Response parsing fails
#[cfg(feature = "api")]
async fn tunnel_magic_token(
    tunnel_host: &str,
    client_id: &str,
    access_token: &str,
    magic_token: &str,
) -> Result<bool, AuthError> {
    let url =
        format!("{tunnel_host}/auth/magic-token?clientId={client_id}&magicToken={magic_token}");
    let value: Value = switchy_http::Client::new()
        .post(&url)
        .header(switchy_http::Header::Authorization.as_ref(), access_token)
        .send()
        .await?
        .json()
        .await?;

    Ok(value
        .get("success")
        .is_some_and(|success| success.as_bool().unwrap_or(false)))
}

/// Creates a new magic token for temporary authentication.
///
/// Generates a UUID-based magic token and optionally synchronizes it with
/// a tunnel server. The token is stored in the database with the current
/// client credentials.
///
/// # Errors
///
/// * Database operations fail
/// * Tunnel synchronization fails
/// * Client credentials are not available
#[cfg(feature = "api")]
pub(crate) async fn create_magic_token(
    db: &ConfigDatabase,
    tunnel_host: Option<String>,
) -> Result<String, AuthError> {
    let magic_token = new_v4_string();

    if let Some((client_id, access_token)) = { get_client_access_token(db).await? } {
        if let Some(tunnel_host) = tunnel_host {
            tunnel_magic_token(&tunnel_host, &client_id, &access_token, &magic_token).await?;
        }
        crate::db::save_magic_token(db, &magic_token, &client_id, &access_token).await?;
    }

    Ok(magic_token)
}

/// Generates a new client ID.
///
/// Creates a UUID v4 string to uniquely identify a client.
#[must_use]
fn create_client_id() -> String {
    new_v4_string()
}

/// Gets or creates a client ID and access token for authentication.
///
/// If credentials already exist in the database, they are returned. Otherwise,
/// a new client ID is created and registered with the host.
///
/// # Errors
///
/// * Database fetch operations fail
/// * Client registration with the host fails
/// * HTTP requests to the host fail
/// * Response parsing fails
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

/// Registers a new client with the authentication host.
///
/// Sends a registration request to the host with the provided client ID
/// and returns the access token if successful.
///
/// # Errors
///
/// * HTTP request to the host fails
/// * Response parsing fails
/// * Authorization token is not set in the environment
///
/// # Panics
///
/// Panics if the `TUNNEL_ACCESS_TOKEN` environment variable is not set.
async fn register_client(host: &str, client_id: &str) -> Result<Option<String>, AuthError> {
    let url = format!("{host}/auth/register-client?clientId={client_id}");

    Ok(switchy_http::Client::new()
        .post(&url)
        .header(
            switchy_http::Header::Authorization.as_ref(),
            &switchy_env::var("TUNNEL_ACCESS_TOKEN").expect("TUNNEL_ACCESS_TOKEN not set"),
        )
        .send()
        .await?
        .json::<Value>()
        .await?
        .to_value("token")?)
}

/// Actix-web request guard that ensures requests are not from the `MoosicBox` tunnel.
///
/// This guard checks the User-Agent header and rejects requests from `MOOSICBOX_TUNNEL`,
/// allowing only non-tunnel requests to proceed.
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

/// Checks if a request is authorized (not from the `MoosicBox` tunnel).
///
/// Returns `true` if the request does not have a User-Agent header or if
/// the User-Agent is not `"MOOSICBOX_TUNNEL"`.
#[must_use]
fn is_authorized(req: &HttpRequest) -> bool {
    if let Some(user_agent) = req.headers().get(http::header::USER_AGENT)
        && let Ok(user_agent) = user_agent.to_str()
    {
        return user_agent != "MOOSICBOX_TUNNEL";
    }

    true
}

/// Fetches a signature token from the authentication host.
///
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
    let response = switchy_http::Client::new()
        .post(&url)
        .header(switchy_http::Header::Authorization.as_ref(), access_token)
        .send()
        .await?;

    if response.status() == switchy_http::models::StatusCode::Unauthorized {
        return Err(AuthError::Unauthorized);
    }

    Ok(response.json::<Value>().await?.to_value("token")?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use actix_web::test::TestRequest;

    #[test_log::test]
    fn test_is_authorized_without_user_agent() {
        let req = TestRequest::default().to_http_request();
        assert!(is_authorized(&req));
    }

    #[test_log::test]
    fn test_is_authorized_with_regular_user_agent() {
        let req = TestRequest::default()
            .insert_header(("User-Agent", "Mozilla/5.0"))
            .to_http_request();
        assert!(is_authorized(&req));
    }

    #[test_log::test]
    fn test_is_authorized_with_tunnel_user_agent() {
        let req = TestRequest::default()
            .insert_header(("User-Agent", "MOOSICBOX_TUNNEL"))
            .to_http_request();
        assert!(!is_authorized(&req));
    }

    #[test_log::test]
    fn test_is_authorized_with_empty_user_agent() {
        let req = TestRequest::default()
            .insert_header(("User-Agent", ""))
            .to_http_request();
        assert!(is_authorized(&req));
    }

    #[test_log::test]
    fn test_is_authorized_with_case_sensitive_tunnel() {
        // Test that the check is case-sensitive
        let req = TestRequest::default()
            .insert_header(("User-Agent", "moosicbox_tunnel"))
            .to_http_request();
        assert!(is_authorized(&req));
    }

    #[test_log::test]
    fn test_is_authorized_with_partial_match() {
        // Test that partial matches don't trigger unauthorized
        let req = TestRequest::default()
            .insert_header(("User-Agent", "MOOSICBOX_TUNNEL_PROXY"))
            .to_http_request();
        assert!(is_authorized(&req));
    }

    #[test_log::test]
    fn test_is_authorized_with_non_utf8_user_agent() {
        // Test that non-UTF8 User-Agent headers are treated as authorized
        // (the code returns true when to_str() fails)
        let req = TestRequest::default()
            .insert_header((
                actix_web::http::header::USER_AGENT,
                actix_web::http::header::HeaderValue::from_bytes(&[0x80, 0x81, 0x82]).unwrap(),
            ))
            .to_http_request();
        assert!(is_authorized(&req));
    }

    #[test_log::test]
    fn test_is_authorized_with_prefix_only() {
        // Test that a prefix of the blocked User-Agent is authorized
        let req = TestRequest::default()
            .insert_header(("User-Agent", "MOOSICBOX_TUNNE"))
            .to_http_request();
        assert!(is_authorized(&req));
    }

    #[test_log::test]
    fn test_non_tunnel_request_authorized_blocks_tunnel() {
        let req = TestRequest::default()
            .insert_header(("User-Agent", "MOOSICBOX_TUNNEL"))
            .to_http_request();

        let mut payload = Payload::None;
        let result = NonTunnelRequestAuthorized::from_request(&req, &mut payload).into_inner();

        assert!(result.is_err());
        if let Err(e) = result {
            assert_eq!(e.to_string(), "Unauthorized");
        }
    }

    #[test_log::test]
    fn test_non_tunnel_request_authorized_allows_regular_requests() {
        let req = TestRequest::default()
            .insert_header(("User-Agent", "Mozilla/5.0"))
            .to_http_request();

        let mut payload = Payload::None;
        let result = NonTunnelRequestAuthorized::from_request(&req, &mut payload).into_inner();

        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_non_tunnel_request_authorized_allows_no_user_agent() {
        let req = TestRequest::default().to_http_request();

        let mut payload = Payload::None;
        let result = NonTunnelRequestAuthorized::from_request(&req, &mut payload).into_inner();

        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_create_client_id_returns_valid_uuid() {
        let client_id = create_client_id();
        // UUID v4 format: xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx
        assert_eq!(client_id.len(), 36);
        assert_eq!(client_id.chars().filter(|&c| c == '-').count(), 4);
    }

    #[test_log::test]
    fn test_create_client_id_generates_unique_ids() {
        let id1 = create_client_id();
        let id2 = create_client_id();
        assert_ne!(id1, id2);
    }
}
