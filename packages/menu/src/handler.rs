use crate::MenuError;
use actix_web::error::ErrorBadRequest;
use moosicbox_core::{
    app::AppState,
    slim::menu::{get_all_albums, Album, AlbumFilters, AlbumSort, AlbumSource},
};
use serde_json::{Map, Value};
use std::{env, str::FromStr, time::Duration};

fn get_query_param_string(
    query: &Map<String, Value>,
    name: &str,
) -> Result<Option<String>, MenuError> {
    query
        .get(name)
        .map(|value| {
            value
                .as_str()
                .ok_or(MenuError::BadRequest(ErrorBadRequest(format!(
                    "{name} query param must be a string"
                ))))
                .map(|s| s.to_string())
        })
        .transpose()
}

pub async fn albums(
    event: &serde_json::Value,
    _context: &lambda_runtime::Context,
) -> Result<Vec<Album>, MenuError> {
    let query = event["queryStringParameters"]
        .as_object()
        .ok_or(MenuError::BadRequest(ErrorBadRequest(
            "Missing query string",
        )))?;

    let player_id = get_query_param_string(query, "playerId")?.ok_or(MenuError::BadRequest(
        ErrorBadRequest("Missing playerId query param"),
    ))?;

    let sources = get_query_param_string(query, "sources")?;
    let sort = get_query_param_string(query, "sort")?;

    let filters = AlbumFilters {
        sources: sources
            .as_ref()
            .map(|sources| {
                sources
                    .split(',')
                    .map(|s| s.trim())
                    .map(|s| {
                        AlbumSource::from_str(s)
                            .map_err(|_e| ErrorBadRequest(format!("Invalid sort value: {s}")))
                    })
                    .collect()
            })
            .transpose()?,
        sort: sort
            .as_ref()
            .map(|sort| {
                AlbumSort::from_str(sort)
                    .map_err(|_e| ErrorBadRequest(format!("Invalid sort value: {sort}")))
            })
            .transpose()?,
    };

    let proxy_url = env::var("PROXY_HOST").map_err(|_e| MenuError::InternalServerError {
        error: "Missing PROXY_HOST environment variable".to_string(),
    })?;

    let proxy_client = awc::Client::builder()
        .timeout(Duration::from_secs(120))
        .finish();

    let image_client = awc::Client::builder()
        .timeout(Duration::from_secs(120))
        .finish();

    let state = AppState {
        service_port: 9000,
        proxy_url,
        proxy_client,
        image_client,
        db: None,
    };

    get_all_albums(&player_id, &state, &filters)
        .await
        .map_err(|e| MenuError::InternalServerError {
            error: format!("{e:?}"),
        })
}
