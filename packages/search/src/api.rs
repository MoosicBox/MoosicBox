use actix_web::{
    error::ErrorInternalServerError,
    get, post,
    web::{self, Json},
    Result,
};
use moosicbox_core::app::AppState;
use serde::Deserialize;
use serde_json::Value;
use tantivy::schema::NamedFieldDocument;

use crate::{data::reindex_global_search_index_from_db, search_global_search_index};

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ReindexQuery {}

#[post("/search/reindex")]
pub async fn reindex_endpoint(
    _query: web::Query<ReindexQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    reindex_global_search_index_from_db(&data.db.as_ref().unwrap().library.lock().unwrap())
        .map_err(|e| ErrorInternalServerError(format!("Failed to reindex from database: {e:?}")))?;

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SearchGlobalSearchQuery {
    query: String,
    offset: Option<usize>,
    limit: Option<usize>,
}

#[get("/search/global-search")]
pub async fn search_global_search_endpoint(
    query: web::Query<SearchGlobalSearchQuery>,
) -> Result<Json<Vec<NamedFieldDocument>>> {
    let results = search_global_search_index(
        &query.query,
        query.offset.unwrap_or(0),
        query.limit.unwrap_or(10),
    )
    .map_err(|e| {
        ErrorInternalServerError(format!("Failed to search global search index: {e:?}"))
    })?;

    Ok(Json(results))
}
