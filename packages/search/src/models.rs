use moosicbox_core::{
    sqlite::models::{ApiAlbumVersionQuality, Id, TrackApiSource},
    types::AudioFormat,
};
use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiGlobalArtistSearchResult {
    pub artist_id: Id,
    pub title: String,
    pub contains_cover: bool,
    pub blur: bool,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiGlobalAlbumSearchResult {
    pub artist_id: Id,
    pub artist: String,
    pub album_id: Id,
    pub title: String,
    pub contains_cover: bool,
    pub blur: bool,
    pub date_released: Option<String>,
    pub date_added: Option<String>,
    pub versions: Vec<ApiAlbumVersionQuality>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiGlobalTrackSearchResult {
    pub artist_id: Id,
    pub artist: String,
    pub album_id: Id,
    pub album: String,
    pub track_id: Id,
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

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[serde(tag = "type")]
pub enum ApiGlobalSearchResult {
    Artist(ApiGlobalArtistSearchResult),
    Album(ApiGlobalAlbumSearchResult),
    Track(ApiGlobalTrackSearchResult),
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApiSearchResultsResponse {
    pub position: usize,
    pub results: Vec<ApiGlobalSearchResult>,
}

impl From<Vec<ApiGlobalSearchResult>> for ApiSearchResultsResponse {
    fn from(value: Vec<ApiGlobalSearchResult>) -> Self {
        ApiSearchResultsResponse {
            position: 0,
            results: value,
        }
    }
}
