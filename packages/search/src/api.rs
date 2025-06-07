use actix_web::{
    Result, Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    get,
    web::{self, Json},
};
use moosicbox_music_api_models::search::api::{
    ApiRawSearchResultsResponse, ApiSearchResultsResponse,
};
use serde::Deserialize;
use tantivy::schema::NamedFieldDocument;

use crate::{global_search, search_global_search_index};

pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(search_global_search_endpoint)
        .service(search_raw_global_search_endpoint)
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchGlobalSearchQuery {
    query: String,
    offset: Option<u32>,
    limit: Option<u32>,
}

#[get("/global-search")]
pub async fn search_global_search_endpoint(
    query: web::Query<SearchGlobalSearchQuery>,
) -> Result<Json<ApiSearchResultsResponse>> {
    Ok(Json(
        global_search(&query.query, query.offset, query.limit).map_err(|e| {
            ErrorInternalServerError(format!("Failed to search global search index: {e:?}"))
        })?,
    ))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchRawGlobalSearchQuery {
    query: String,
    offset: Option<u32>,
    limit: Option<u32>,
}

#[get("/raw-global-search")]
pub async fn search_raw_global_search_endpoint(
    query: web::Query<SearchRawGlobalSearchQuery>,
) -> Result<Json<ApiRawSearchResultsResponse>> {
    let limit = query.limit.unwrap_or(10);
    let offset = query.offset.unwrap_or(0);

    let mut position = offset;
    let mut results: Vec<NamedFieldDocument> = vec![];

    while results.len() < limit as usize {
        let values = search_global_search_index(&query.query, position, limit).map_err(|e| {
            ErrorInternalServerError(format!("Failed to search global search index: {e:?}"))
        })?;

        if values.is_empty() {
            break;
        }

        for value in values {
            position += 1;

            results.push(value);

            if results.len() >= limit as usize {
                break;
            }
        }
    }

    Ok(Json(ApiRawSearchResultsResponse { position, results }))
}
