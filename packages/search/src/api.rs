use actix_web::{
    Result, Scope,
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    get,
    web::{self, Json},
};
use moosicbox_json_utils::ToValueType;
use serde::Deserialize;
use tantivy::schema::NamedFieldDocument;

use crate::models::api::{
    ApiGlobalSearchResult, ApiRawSearchResultsResponse, ApiSearchResultsResponse,
};
use crate::search_global_search_index;

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
    offset: Option<usize>,
    limit: Option<usize>,
}

#[get("/global-search")]
pub async fn search_global_search_endpoint(
    query: web::Query<SearchGlobalSearchQuery>,
) -> Result<Json<ApiSearchResultsResponse>> {
    let limit = query.limit.unwrap_or(10);
    let offset = query.offset.unwrap_or(0);

    let mut position = offset;
    let mut results: Vec<ApiGlobalSearchResult> = vec![];

    while results.len() < limit {
        let values = search_global_search_index(&query.query, position, limit).map_err(|e| {
            ErrorInternalServerError(format!("Failed to search global search index: {e:?}"))
        })?;

        if values.is_empty() {
            break;
        }

        for value in values {
            position += 1;

            let value: ApiGlobalSearchResult = match value.to_value_type() {
                Ok(value) => value,
                Err(err) => {
                    log::error!("Failed to parse search result: {err:?}");
                    continue;
                }
            };

            if !results.iter().any(|r| r.to_key() == value.to_key()) {
                results.push(value);

                if results.len() >= limit {
                    break;
                }
            }
        }
    }

    Ok(Json(ApiSearchResultsResponse { position, results }))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchRawGlobalSearchQuery {
    query: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[get("/raw-global-search")]
pub async fn search_raw_global_search_endpoint(
    query: web::Query<SearchRawGlobalSearchQuery>,
) -> Result<Json<ApiRawSearchResultsResponse>> {
    let limit = query.limit.unwrap_or(10);
    let offset = query.offset.unwrap_or(0);

    let mut position = offset;
    let mut results: Vec<NamedFieldDocument> = vec![];

    while results.len() < limit {
        let values = search_global_search_index(&query.query, position, limit).map_err(|e| {
            ErrorInternalServerError(format!("Failed to search global search index: {e:?}"))
        })?;

        if values.is_empty() {
            break;
        }

        for value in values {
            position += 1;

            results.push(value);

            if results.len() >= limit {
                break;
            }
        }
    }

    Ok(Json(ApiRawSearchResultsResponse { position, results }))
}
