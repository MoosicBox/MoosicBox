use actix_web::{
    error::{ErrorInternalServerError, ErrorUnauthorized},
    route,
    web::{self, Json},
    Result,
};
use moosicbox_core::app::AppState;
use serde::Deserialize;
use serde_json::{json, Value};
use url::form_urlencoded;

use crate::{create_magic_token, get_credentials_from_magic_token, NonTunnelRequestAuthorized};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MagicTokenQuery {
    magic_token: String,
}

#[route("/auth/magic-token", method = "GET")]
pub async fn get_magic_token_endpoint(
    query: web::Query<MagicTokenQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    if let Some((client_id, access_token)) =
        get_credentials_from_magic_token(&data.database, &query.magic_token)
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
pub struct CreateMagicTokenQuery {
    host: Option<String>,
}

#[route("/auth/magic-token", method = "POST")]
pub async fn create_magic_token_endpoint(
    query: web::Query<CreateMagicTokenQuery>,
    data: web::Data<AppState>,
    _: NonTunnelRequestAuthorized,
) -> Result<Json<Value>> {
    let token = create_magic_token(&data.database, data.tunnel_host.clone())
        .await
        .map_err(|e| ErrorInternalServerError(format!("Failed to create magic token: {e:?}")))?;

    let mut query_string = form_urlencoded::Serializer::new(String::new());

    query_string.append_pair("magicToken", &token);

    if let Some(tunnel_host) = &data.tunnel_host {
        query_string.append_pair("apiUrl", tunnel_host);
    }

    let query_string = query_string.finish();

    if let Some(host) = &query.host {
        Ok(Json(json!({
            "token": token,
            "url": format!("{host}/auth?{query_string}")
        })))
    } else {
        Ok(Json(json!({
            "token": token,
        })))
    }
}
