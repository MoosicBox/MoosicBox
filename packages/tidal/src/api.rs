use actix_web::{
    error::ErrorInternalServerError,
    route,
    web::{self, Json},
    Result,
};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalDeviceAuthorizationQuery {
    client_id: String,
}

#[route("/tidal/auth/device-authorization", method = "POST")]
pub async fn tidal_device_authorization_endpoint(
    query: web::Query<TidalDeviceAuthorizationQuery>,
) -> Result<Json<Value>> {
    let url = "https://auth.tidal.com/v1/oauth2/device_authorization";

    let params = [
        ("client_id", query.client_id.clone()),
        ("scope", "r_usr w_usr w_sub".to_string()),
    ];

    let value: Value = reqwest::Client::new()
        .post(url)
        .form(&params)
        .send()
        .await
        .map_err(|_| ErrorInternalServerError("Failed to get device authorization link"))?
        .json()
        .await
        .map_err(|_| ErrorInternalServerError("Failed to get device authorization link"))?;

    let verification_uri_complete = value
        .get("verificationUriComplete")
        .unwrap()
        .as_str()
        .unwrap();

    let device_code = value.get("deviceCode").unwrap().as_str().unwrap();

    Ok(Json(serde_json::json!({
        "url": format!("https://{verification_uri_complete}"),
        "device_code": device_code,
    })))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TidalDeviceAuthorizationTokenQuery {
    client_id: String,
    client_secret: String,
    device_code: String,
}

#[route("/tidal/auth/device-authorization/token", method = "POST")]
pub async fn tidal_device_authorization_token_endpoint(
    query: web::Query<TidalDeviceAuthorizationTokenQuery>,
) -> Result<Json<Value>> {
    let url = "https://auth.tidal.com/v1/oauth2/token";

    let params = [
        ("client_id", query.client_id.clone()),
        ("client_secret", query.client_secret.clone()),
        ("device_code", query.device_code.clone()),
        (
            "grant_type",
            "urn:ietf:params:oauth:grant-type:device_code".to_string(),
        ),
        ("scope", "r_usr w_usr w_sub".to_string()),
    ];

    let value: Value = reqwest::Client::new()
        .post(url)
        .form(&params)
        .send()
        .await
        .map_err(|_| ErrorInternalServerError("Failed to get device authorization token"))?
        .json()
        .await
        .map_err(|_| ErrorInternalServerError("Failed to get device authorization token"))?;

    let access_token = value.get("access_token").unwrap().as_str().unwrap();
    let refresh_token = value.get("refresh_token").unwrap().as_str().unwrap();

    Ok(Json(serde_json::json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
    })))
}
