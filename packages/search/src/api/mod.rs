use std::str::FromStr;

use actix_web::{
    dev::{ServiceFactory, ServiceRequest},
    error::ErrorInternalServerError,
    get,
    web::{self, Json},
    Result, Scope,
};
use moosicbox_json_utils::{tantivy::ToValue, ParseError, ToValueType};
use moosicbox_music_models::{api::ApiAlbumVersionQuality, AudioFormat, TrackApiSource};
use serde::Deserialize;
use tantivy::schema::NamedFieldDocument;

pub mod models;

use crate::search_global_search_index;
use models::{
    ApiGlobalAlbumSearchResult, ApiGlobalArtistSearchResult, ApiGlobalSearchResult,
    ApiGlobalTrackSearchResult, ApiRawSearchResultsResponse, ApiSearchResultsResponse,
};

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

impl ApiGlobalSearchResult {
    fn to_key(&self) -> String {
        match self {
            Self::Artist(artist) => format!("artist|{}", artist.title),
            Self::Album(album) => {
                format!("album|{}|{}", album.title, album.artist)
            }
            Self::Track(track) => {
                format!("track|{}|{}|{}", track.title, track.album, track.artist)
            }
        }
    }
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
