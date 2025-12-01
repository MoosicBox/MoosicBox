//! HTTP API endpoints for authentication operations.
//!
//! This module provides Actix-web endpoints for creating and retrieving magic tokens,
//! which enable temporary credential exchange for authentication. These endpoints are
//! only available when the `api` feature is enabled.
//!
//! # Endpoints
//!
//! * `POST /magic-token` - Create a new magic token
//! * `GET /magic-token` - Retrieve credentials from a magic token
//!
//! # Example
//!
//! ```rust,no_run
//! # use actix_web::App;
//! # use moosicbox_auth::api;
//! let app = App::new()
//!     .service(api::bind_services(actix_web::web::scope("/auth")));
//! ```

#![allow(clippy::needless_for_each)]

use actix_web::{
    Result, Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorInternalServerError, ErrorUnauthorized},
    route,
    web::{self, Json},
};
use moosicbox_middleware::tunnel_info::TunnelInfo;
use serde::Deserialize;
use serde_json::{Value, json};
use switchy_database::config::ConfigDatabase;
use url::form_urlencoded;

use crate::{NonTunnelRequestAuthorized, create_magic_token, get_credentials_from_magic_token};

/// Binds authentication API endpoints to an Actix-web scope.
///
/// This function registers the magic token endpoints for creating and retrieving
/// authentication credentials.
#[must_use]
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(get_magic_token_endpoint)
        .service(create_magic_token_endpoint)
}

/// `OpenAPI` documentation structure for authentication endpoints.
#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Auth")),
    paths(get_magic_token_endpoint, create_magic_token_endpoint,),
    components(schemas(MagicTokenQuery, CreateMagicTokenQuery))
)]
pub struct Api;

/// Query parameters for retrieving credentials from a magic token.
#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct MagicTokenQuery {
    /// The magic token to exchange for credentials.
    magic_token: String,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Auth"],
        get,
        path = "/magic-token",
        description = "Get the credentials associated with a magic token",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("magicToken" = String, Query,
                description = "The magic token to fetch the credentials for"),
        ),
        responses(
            (status = 200, description = "The credentials for the magic token", body = Value)
        )
    )
)]
#[route("/magic-token", method = "GET")]
/// Endpoint to retrieve client credentials from a magic token.
///
/// Returns the client ID and access token associated with the provided magic token.
/// The magic token is consumed after successful retrieval.
///
/// # Errors
///
/// * If the magic token is invalid or expired
/// * If database operations fail
pub async fn get_magic_token_endpoint(
    query: web::Query<MagicTokenQuery>,
    db: ConfigDatabase,
) -> Result<Json<Value>> {
    if let Some((client_id, access_token)) =
        get_credentials_from_magic_token(&db, &query.magic_token)
            .await
            .map_err(|e| {
                log::error!("Failed to get magic token: {e:?}");
                ErrorInternalServerError("Failed to get magic token")
            })?
    {
        Ok(Json(
            json!({"clientId": client_id, "accessToken": access_token}),
        ))
    } else {
        log::warn!("Unauthorized get magic-token request");
        Err(ErrorUnauthorized("Unauthorized"))
    }
}

/// Query parameters for creating a magic token.
#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateMagicTokenQuery {
    /// Optional host URL to generate a complete link with the magic token.
    host: Option<String>,
}

#[cfg_attr(
    feature = "openapi", utoipa::path(
        tags = ["Auth"],
        post,
        path = "/magic-token",
        description = "Create a new magic token",
        params(
            ("moosicbox-profile" = String, Header, description = "MoosicBox profile"),
            ("host" = Option<String>, Query,
                description = "The host to generate a link with the magic token for"),
        ),
        responses(
            (status = 200, description = "The magic token", body = Value)
        )
    )
)]
#[route("/magic-token", method = "POST")]
/// Endpoint to create a new magic token for authentication.
///
/// Creates a magic token that can be exchanged for client credentials.
/// If a host is provided, returns a complete URL with the token embedded.
///
/// # Errors
///
/// * If database operations fail
/// * If tunnel synchronization fails
pub async fn create_magic_token_endpoint(
    query: web::Query<CreateMagicTokenQuery>,
    tunnel_info: TunnelInfo,
    db: ConfigDatabase,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    let token = create_magic_token(&db, tunnel_info.host.as_ref().clone())
        .await
        .map_err(|e| {
            log::error!("Failed to create magic token: {e:?}");
            ErrorInternalServerError("Failed to create magic token")
        })?;

    let mut query_string = form_urlencoded::Serializer::new(String::new());

    query_string.append_pair("magicToken", &token);

    if let Some(tunnel_host) = &*tunnel_info.host {
        query_string.append_pair("apiUrl", tunnel_host);
    }

    let query_string = query_string.finish();

    query.host.as_ref().map_or_else(
        || {
            Ok(Json(json!({
                "token": token,
            })))
        },
        |host| {
            Ok(Json(json!({
                "token": token,
                "url": format!("{host}?{query_string}")
            })))
        },
    )
}
