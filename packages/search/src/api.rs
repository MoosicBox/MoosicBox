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

/// Binds the search API endpoints to the provided Actix-web scope.
pub fn bind_services<
    T: ServiceFactory<ServiceRequest, Config = (), Error = actix_web::Error, InitError = ()>,
>(
    scope: Scope<T>,
) -> Scope<T> {
    scope
        .service(search_global_search_endpoint)
        .service(search_raw_global_search_endpoint)
}

/// Query parameters for the global search endpoint.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchGlobalSearchQuery {
    /// The search query string
    query: String,
    /// Optional offset for pagination
    offset: Option<u32>,
    /// Optional limit for the number of results
    limit: Option<u32>,
}

/// API endpoint for performing a global search and returning structured results.
///
/// # Errors
///
/// * If the search operation fails
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

/// Query parameters for the raw global search endpoint.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchRawGlobalSearchQuery {
    /// The search query string
    query: String,
    /// Optional offset for pagination
    offset: Option<u32>,
    /// Optional limit for the number of results
    limit: Option<u32>,
}

/// API endpoint for performing a global search and returning raw index documents.
///
/// # Errors
///
/// * If the search operation fails
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
