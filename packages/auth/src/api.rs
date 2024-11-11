use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::{ErrorInternalServerError, ErrorUnauthorized},
    route,
    web::{self, Json},
    Result, Scope,
};
use moosicbox_core::app::AppState;
use moosicbox_database::config::ConfigDatabase;
use serde::Deserialize;
use serde_json::{json, Value};
use url::form_urlencoded;

use crate::{create_magic_token, get_credentials_from_magic_token, NonTunnelRequestAuthorized};

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(get_magic_token_endpoint)
        .service(create_magic_token_endpoint)
}

#[cfg(feature = "openapi")]
#[derive(utoipa::OpenApi)]
#[openapi(
    tags((name = "Auth")),
    paths(get_magic_token_endpoint, create_magic_token_endpoint,),
    components(schemas(MagicTokenQuery, CreateMagicTokenQuery))
)]
pub struct Api;

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct MagicTokenQuery {
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
pub async fn get_magic_token_endpoint(
    query: web::Query<MagicTokenQuery>,
    db: ConfigDatabase,
) -> Result<Json<Value>> {
    if let Some((client_id, access_token)) =
        get_credentials_from_magic_token(&db, &query.magic_token)
            .await
            .map_err(|e| ErrorInternalServerError(format!("Failed to get magic token: {e:?}")))?
    {
        Ok(Json(
            json!({"clientId": client_id, "accessToken": access_token}),
        ))
    } else {
        log::warn!("Unauthorized get magic-token request");
        Err(ErrorUnauthorized("Unauthorized"))
    }
}

#[derive(Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CreateMagicTokenQuery {
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
pub async fn create_magic_token_endpoint(
    query: web::Query<CreateMagicTokenQuery>,
    data: web::Data<AppState>,
    db: ConfigDatabase,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    let token = create_magic_token(&db, data.tunnel_host.clone())
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to create magic token: {e:?}")))?;

    let mut query_string = form_urlencoded::Serializer::new(String::new());

    query_string.append_pair("magicToken", &token);

    if let Some(tunnel_host) = &data.tunnel_host {
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
