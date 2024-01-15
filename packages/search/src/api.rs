use std::str::FromStr;

use actix_web::{
    error::ErrorInternalServerError,
    get, post,
    web::{self, Json},
    Result,
};
use moosicbox_core::{
    app::AppState,
    sqlite::models::{ApiAlbumVersionQuality, TrackSource},
    types::AudioFormat,
};
use moosicbox_json_utils::{
    tantivy::{ToValue, ToValueType},
    ParseError,
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

impl ToValueType<ApiGlobalArtistSearchResult> for &NamedFieldDocument {
    fn to_value_type(self) -> std::result::Result<ApiGlobalArtistSearchResult, ParseError> {
        Ok(ApiGlobalArtistSearchResult {
            artist_id: self.to_value("artist_id")?,
            title: self.to_value("artist_title")?,
            contains_cover: self
                .to_value::<Option<&str>>("cover")?
                .is_some_and(|cover| !cover.is_empty()),
            blur: self.to_value("blur")?,
        })
    }

    fn missing_value(
        self,
        error: ParseError,
    ) -> std::result::Result<ApiGlobalArtistSearchResult, ParseError> {
        Err(error)
    }
}

impl ToValueType<ApiGlobalAlbumSearchResult> for &NamedFieldDocument {
    fn to_value_type(self) -> std::result::Result<ApiGlobalAlbumSearchResult, ParseError> {
        Ok(ApiGlobalAlbumSearchResult {
            artist_id: self.to_value("artist_id")?,
            artist: self.to_value("artist_title")?,
            album_id: self.to_value("album_id")?,
            title: self.to_value("album_title")?,
            contains_cover: self
                .to_value::<Option<&str>>("cover")?
                .is_some_and(|cover| !cover.is_empty()),
            blur: self.to_value("blur")?,
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            versions: self
                .to_value::<Vec<Option<&str>>>("version_formats")?
                .iter()
                .zip(self.to_value::<Vec<&str>>("version_sources")?.iter())
                .zip(
                    self.to_value::<Vec<Option<u8>>>("version_bit_depths")?
                        .iter(),
                )
                .zip(
                    self.to_value::<Vec<Option<u32>>>("version_sample_rates")?
                        .iter(),
                )
                .zip(self.to_value::<Vec<Option<u8>>>("version_channels")?.iter())
                .map(|((((format, source), bit_depth), sample_rate), channels)| {
                    Ok(ApiAlbumVersionQuality {
                        format: format.map(|format| {
                            AudioFormat::from_str(format)
                                .unwrap_or_else(|_| panic!("Invalid AudioFormat: {format}"))
                        }),
                        bit_depth: *bit_depth,
                        sample_rate: *sample_rate,
                        channels: *channels,
                        source: TrackSource::from_str(source)
                            .unwrap_or_else(|_| panic!("Invalid TrackSource: {source}")),
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }

    fn missing_value(
        self,
        error: ParseError,
    ) -> std::result::Result<ApiGlobalAlbumSearchResult, ParseError> {
        Err(error)
    }
}

impl ToValueType<ApiGlobalTrackSearchResult> for &NamedFieldDocument {
    fn to_value_type(self) -> std::result::Result<ApiGlobalTrackSearchResult, ParseError> {
        Ok(ApiGlobalTrackSearchResult {
            artist_id: self.to_value("artist_id")?,
            artist: self.to_value("artist_title")?,
            album_id: self.to_value("album_id")?,
            album: self.to_value("album_title")?,
            track_id: self.to_value("track_id")?,
            title: self.to_value("track_title")?,
            contains_cover: self
                .to_value::<Option<&str>>("cover")?
                .is_some_and(|cover| !cover.is_empty()),
            blur: self.to_value("blur")?,
            date_released: self.to_value("date_released")?,
            date_added: self.to_value("date_added")?,
            format: self
                .to_value::<Option<&str>>("version_formats")?
                .map(|format| {
                    AudioFormat::from_str(format)
                        .unwrap_or_else(|_| panic!("Invalid AudioFormat: {format}"))
                }),
            bit_depth: self.to_value("version_bit_depths")?,
            sample_rate: self.to_value("version_sample_rates")?,
            channels: self.to_value("version_channels")?,
            source: TrackSource::from_str(self.to_value("version_sources")?)
                .map_err(|_| ParseError::ConvertType("TrackSource".into()))?,
        })
    }

    fn missing_value(
        self,
        error: ParseError,
    ) -> std::result::Result<ApiGlobalTrackSearchResult, ParseError> {
        Err(error)
    }
}

impl ToValueType<ApiGlobalSearchResult> for &NamedFieldDocument {
    fn to_value_type(self) -> std::result::Result<ApiGlobalSearchResult, ParseError> {
        Ok(match self.to_value("document_type")? {
            "artists" => ApiGlobalSearchResult::Artist(self.to_value_type()?),
            "albums" => ApiGlobalSearchResult::Album(self.to_value_type()?),
            "tracks" => ApiGlobalSearchResult::Track(self.to_value_type()?),
            _ => {
                return Err(ParseError::ConvertType("document_type".into()));
            }
        })
    }

    fn missing_value(
        self,
        error: ParseError,
    ) -> std::result::Result<ApiGlobalSearchResult, ParseError> {
        Err(error)
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

    let api_results = results
        .iter()
        .map(|doc| doc.to_value_type())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            ErrorInternalServerError(format!("Failed to search global search index: {e:?}"))
        })?;

    Ok(Json(api_results))
}
