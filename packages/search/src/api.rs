use std::str::FromStr;

use actix_web::{
    error::ErrorInternalServerError,
    get, post,
    web::{self, Json},
    Result,
};
use moosicbox_core::{
    app::AppState,
    sqlite::models::{ApiAlbumVersionQuality, TrackApiSource},
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
    reindex_global_search_index_from_db(&data.database)
        .await
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

impl ApiGlobalSearchResult {
    fn to_key(&self) -> String {
        match self {
            ApiGlobalSearchResult::Artist(artist) => format!("artist|{}", artist.title),
            ApiGlobalSearchResult::Album(album) => {
                format!("album|{}|{}", album.title, album.artist)
            }
            ApiGlobalSearchResult::Track(track) => {
                format!("track|{}|{}|{}", track.title, track.album, track.artist)
            }
        }
    }
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
                        format: format
                            .map(|format| {
                                AudioFormat::from_str(format).map_err(|_| {
                                    ParseError::ConvertType(format!("AudioFormat '{format}'"))
                                })
                            })
                            .transpose()?,
                        bit_depth: *bit_depth,
                        sample_rate: *sample_rate,
                        channels: *channels,
                        source: TrackApiSource::from_str(source)
                            .map_err(|_| ParseError::ConvertType("TrackSource".into()))?,
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        })
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
                        .map_err(|_| ParseError::ConvertType(format!("AudioFormat '{format}'")))
                })
                .transpose()?,
            bit_depth: self.to_value("version_bit_depths")?,
            sample_rate: self.to_value("version_sample_rates")?,
            channels: self.to_value("version_channels")?,
            source: TrackApiSource::from_str(self.to_value("version_sources")?)
                .map_err(|_| ParseError::ConvertType("TrackSource".into()))?,
        })
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
    pub source: TrackApiSource,
}

#[get("/search/global-search")]
pub async fn search_global_search_endpoint(
    query: web::Query<SearchGlobalSearchQuery>,
) -> Result<Json<Value>> {
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
                    log::trace!("Failed to parse search result: {err:?}");
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

    Ok(Json(serde_json::json!({
        "position": position,
        "results": results,
    })))
}
