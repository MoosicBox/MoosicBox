use actix_web::{
    error::ErrorInternalServerError,
    get, post,
    web::{self, Json},
    Result,
};
use moosicbox_core::{app::AppState, sqlite::models::ToApi};
use serde::{Deserialize, Serialize};
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

#[derive(Serialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiGlobalSearchResult {
    Artist(ApiGlobalArtistSearchResult),
    Album(ApiGlobalAlbumSearchResult),
    Track(ApiGlobalTrackSearchResult),
}

impl ToApi<ApiGlobalSearchResult> for NamedFieldDocument {
    fn to_api(&self) -> ApiGlobalSearchResult {
        match self
            .0
            .get("document_type")
            .unwrap()
            .first()
            .unwrap()
            .as_text()
            .unwrap()
        {
            "artists" => ApiGlobalSearchResult::Artist(ApiGlobalArtistSearchResult {
                artist_id: self
                    .0
                    .get("artist_id")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_u64()
                    .unwrap(),
                title: self
                    .0
                    .get("artist_title")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_text()
                    .unwrap()
                    .to_string(),
            }),
            "albums" => ApiGlobalSearchResult::Album(ApiGlobalAlbumSearchResult {
                album_id: self
                    .0
                    .get("album_id")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_u64()
                    .unwrap(),
                title: self
                    .0
                    .get("album_title")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_text()
                    .unwrap()
                    .to_string(),
            }),
            "tracks" => ApiGlobalSearchResult::Track(ApiGlobalTrackSearchResult {
                track_id: self
                    .0
                    .get("track_id")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_u64()
                    .unwrap(),
                title: self
                    .0
                    .get("track_title")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_text()
                    .unwrap()
                    .to_string(),
            }),
            _ => unreachable!(),
        }
    }
}

#[derive(Serialize, Clone)]
pub struct ApiGlobalArtistSearchResult {
    pub artist_id: u64,
    pub title: String,
}

#[derive(Serialize, Clone)]
pub struct ApiGlobalAlbumSearchResult {
    pub album_id: u64,
    pub title: String,
}

#[derive(Serialize, Clone)]
pub struct ApiGlobalTrackSearchResult {
    pub track_id: u64,
    pub title: String,
}

#[get("/search/global-search")]
pub async fn search_global_search_endpoint(
    query: web::Query<SearchGlobalSearchQuery>,
) -> Result<Json<Vec<ApiGlobalSearchResult>>> {
    let results = search_global_search_index(
        &query.query,
        query.offset.unwrap_or(0),
        query.limit.unwrap_or(10),
    )
    .map_err(|e| {
        ErrorInternalServerError(format!("Failed to search global search index: {e:?}"))
    })?;

    let api_results = results.iter().map(|doc| doc.to_api()).collect::<Vec<_>>();

    Ok(Json(api_results))
}
