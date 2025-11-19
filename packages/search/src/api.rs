//! API endpoints for search functionality.
//!
//! This module provides Actix-web REST API endpoints for performing global search
//! operations. It includes endpoints for both structured search results and raw
//! Tantivy document results.

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
///
/// This function registers the `/global-search` and `/raw-global-search` endpoints
/// with the provided scope for use in an Actix-web application.
#[must_use]
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
///
/// This structure defines the parameters accepted by the `/global-search` endpoint
/// for performing structured searches across the music library.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchGlobalSearchQuery {
    /// The search query string to match against artists, albums, and tracks
    query: String,
    /// Optional offset for pagination (default: 0)
    offset: Option<u32>,
    /// Optional maximum number of results to return (default: 10)
    limit: Option<u32>,
}

/// API endpoint for performing a global search and returning structured results.
///
/// This endpoint performs a full-text search across the music library and returns
/// results as structured API types with automatic deduplication.
///
/// # Errors
///
/// * `ErrorInternalServerError` if the search operation fails
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
///
/// This structure defines the parameters accepted by the `/raw-global-search` endpoint
/// for performing searches that return raw Tantivy index documents.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchRawGlobalSearchQuery {
    /// The search query string to match against artists, albums, and tracks
    query: String,
    /// Optional offset for pagination (default: 0)
    offset: Option<u32>,
    /// Optional maximum number of results to return (default: 10)
    limit: Option<u32>,
}

/// API endpoint for performing a global search and returning raw index documents.
///
/// This endpoint performs a full-text search across the music library and returns
/// raw Tantivy documents without conversion to structured API types.
///
/// # Errors
///
/// * `ErrorInternalServerError` if the search operation fails
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
