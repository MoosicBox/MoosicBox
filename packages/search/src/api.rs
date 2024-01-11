use std::str::FromStr;

use actix_web::{
    error::ErrorInternalServerError,
    get, post,
    web::{self, Json},
    Result,
};
use moosicbox_core::{
    app::AppState,
    sqlite::models::{ApiAlbumVersionQuality, ToApi, TrackSource},
    types::AudioFormat,
};
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
                contains_cover: self
                    .0
                    .get("cover")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_text()
                    .is_some_and(|cover| !cover.is_empty()),
                blur: self
                    .0
                    .get("blur")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_bool()
                    .unwrap(),
            }),
            "albums" => ApiGlobalSearchResult::Album(ApiGlobalAlbumSearchResult {
                artist_id: self
                    .0
                    .get("artist_id")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_u64()
                    .unwrap(),
                artist: self
                    .0
                    .get("artist_title")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_text()
                    .unwrap()
                    .to_string(),
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
                contains_cover: self
                    .0
                    .get("cover")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_text()
                    .is_some_and(|cover| !cover.is_empty()),
                blur: self
                    .0
                    .get("blur")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_bool()
                    .unwrap(),
                date_released: self
                    .0
                    .get("date_released")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_text()
                    .map(|s| s.to_string()),
                date_added: self
                    .0
                    .get("date_added")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_text()
                    .map(|s| s.to_string()),
                versions: self
                    .0
                    .get("version_formats")
                    .unwrap()
                    .iter()
                    .enumerate()
                    .map(|(i, format)| {
                        let source = self
                            .0
                            .get("version_sources")
                            .unwrap()
                            .iter()
                            .nth(i)
                            .unwrap()
                            .as_text()
                            .unwrap();

                        ApiAlbumVersionQuality {
                            format: format.as_text().map(|format| {
                                AudioFormat::from_str(format)
                                    .unwrap_or_else(|_| panic!("Invalid AudioFormat: {format}"))
                            }),
                            bit_depth: self
                                .0
                                .get("version_bit_depths")
                                .unwrap()
                                .iter()
                                .nth(i)
                                .unwrap()
                                .as_u64()
                                .map(|depth| depth as u8),
                            sample_rate: self
                                .0
                                .get("version_sample_rates")
                                .unwrap()
                                .iter()
                                .nth(i)
                                .unwrap()
                                .as_u64()
                                .map(|rate| rate as u32),
                            channels: self
                                .0
                                .get("version_channels")
                                .unwrap()
                                .iter()
                                .nth(i)
                                .unwrap()
                                .as_u64()
                                .map(|channels| channels as u8),
                            source: TrackSource::from_str(source)
                                .unwrap_or_else(|_| panic!("Invalid TrackSource: {source}")),
                        }
                    })
                    .collect::<Vec<_>>(),
            }),
            "tracks" => {
                let format = self
                    .0
                    .get("version_formats")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_text();

                let source = self
                    .0
                    .get("version_sources")
                    .unwrap()
                    .first()
                    .unwrap()
                    .as_text()
                    .unwrap();

                ApiGlobalSearchResult::Track(ApiGlobalTrackSearchResult {
                    artist_id: self
                        .0
                        .get("artist_id")
                        .unwrap()
                        .first()
                        .unwrap()
                        .as_u64()
                        .unwrap(),
                    artist: self
                        .0
                        .get("artist_title")
                        .unwrap()
                        .first()
                        .unwrap()
                        .as_text()
                        .unwrap()
                        .to_string(),
                    album_id: self
                        .0
                        .get("album_id")
                        .unwrap()
                        .first()
                        .unwrap()
                        .as_u64()
                        .unwrap(),
                    album: self
                        .0
                        .get("album_title")
                        .unwrap()
                        .first()
                        .unwrap()
                        .as_text()
                        .unwrap()
                        .to_string(),
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
                    contains_cover: self
                        .0
                        .get("cover")
                        .unwrap()
                        .first()
                        .unwrap()
                        .as_text()
                        .is_some_and(|cover| !cover.is_empty()),
                    blur: self
                        .0
                        .get("blur")
                        .unwrap()
                        .first()
                        .unwrap()
                        .as_bool()
                        .unwrap(),
                    date_released: self
                        .0
                        .get("date_released")
                        .unwrap()
                        .first()
                        .unwrap()
                        .as_text()
                        .map(|s| s.to_string()),
                    date_added: self
                        .0
                        .get("date_added")
                        .unwrap()
                        .first()
                        .unwrap()
                        .as_text()
                        .map(|s| s.to_string()),
                    format: format.map(|format| {
                        AudioFormat::from_str(format)
                            .unwrap_or_else(|_| panic!("Invalid AudioFormat: {format}"))
                    }),
                    bit_depth: self
                        .0
                        .get("version_bit_depths")
                        .unwrap()
                        .first()
                        .unwrap()
                        .as_u64()
                        .map(|depth| depth as u8),
                    sample_rate: self
                        .0
                        .get("version_sample_rates")
                        .unwrap()
                        .first()
                        .unwrap()
                        .as_u64()
                        .map(|rate| rate as u32),
                    channels: self
                        .0
                        .get("version_channels")
                        .unwrap()
                        .first()
                        .unwrap()
                        .as_u64()
                        .map(|channels| channels as u8),
                    source: TrackSource::from_str(source)
                        .unwrap_or_else(|_| panic!("Invalid TrackSource: {source}")),
                })
            }
            _ => unreachable!(),
        }
    }
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiGlobalArtistSearchResult {
    pub artist_id: u64,
    pub title: String,
    pub contains_cover: bool,
    pub blur: bool,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiGlobalAlbumSearchResult {
    pub artist_id: u64,
    pub artist: String,
    pub album_id: u64,
    pub title: String,
    pub contains_cover: bool,
    pub blur: bool,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub versions: Vec<ApiAlbumVersionQuality>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiGlobalTrackSearchResult {
    pub artist_id: u64,
    pub artist: String,
    pub album_id: u64,
    pub album: String,
    pub track_id: u64,
    pub title: String,
    pub contains_cover: bool,
    pub blur: bool,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub format: Option<AudioFormat>,
    pub bit_depth: Option<u8>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub source: TrackSource,
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
