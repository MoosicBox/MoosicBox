use crate::{
    app::AppState,
    cache::{get_or_set_to_cache, CacheItemType, CacheRequest},
    sqlite::db::{get_albums, DbError},
};
use futures::{future, FutureExt};
use serde::{Deserialize, Serialize};
use std::{str::FromStr, time::Duration};
use thiserror::Error;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FullAlbum {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: Option<i16>,
    pub artwork: Option<String>,
    pub source: AlbumSource,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: Option<i16>,
    pub date_released: Option<String>,
    pub artwork: Option<String>,
    pub directory: Option<String>,
    pub source: AlbumSource,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq, Default)]
pub enum AlbumSource {
    #[default]
    Local,
    Tidal,
    Qobuz,
}

impl FromStr for AlbumSource {
    type Err = ();

    fn from_str(input: &str) -> Result<AlbumSource, Self::Err> {
        match input.to_lowercase().as_str() {
            "local" => Ok(AlbumSource::Local),
            "tidal" => Ok(AlbumSource::Tidal),
            "qobuz" => Ok(AlbumSource::Qobuz),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum AlbumSort {
    ArtistAsc,
    ArtistDesc,
    NameAsc,
    NameDesc,
    ReleaseDateAsc,
    ReleaseDateDesc,
}

impl FromStr for AlbumSort {
    type Err = ();

    fn from_str(input: &str) -> Result<AlbumSort, Self::Err> {
        match input.to_lowercase().as_str() {
            "artist-asc" | "artist" => Ok(AlbumSort::ArtistAsc),
            "artist-desc" => Ok(AlbumSort::ArtistDesc),
            "name-asc" | "name" => Ok(AlbumSort::NameAsc),
            "name-desc" => Ok(AlbumSort::NameDesc),
            "release-date-asc" | "release-date" => Ok(AlbumSort::ReleaseDateAsc),
            "release-date-desc" => Ok(AlbumSort::ReleaseDateDesc),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumResponseParams {
    #[serde(rename = "isContextMenu")]
    is_context_menu: i32,

    item_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumResponseActionsGoParams {
    item_id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumResponseActionsGo {
    params: AlbumResponseActionsGoParams,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumResponseActions {
    go: AlbumResponseActionsGo,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GetAlbumResponse {
    pub result: GetAlbumResponseResult,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GetAlbumResponseResultTitle {
    pub title: String,
    pub duration: String,
    pub disc: String,
    pub compilation: String,
    pub genre: String,
    pub artist_id: String,
    #[serde(rename = "tracknum")]
    pub track_num: String,
    pub url: String,
    #[serde(rename = "albumartist")]
    pub album_artist: String,
    #[serde(rename = "trackartist")]
    pub track_artist: String,
    #[serde(rename = "albumartist_ids")]
    pub album_artist_ids: String,
    #[serde(rename = "trackartist_ids")]
    pub track_artist_ids: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GetAlbumResponseResult {
    pub count: i32,
    pub titles_loop: Vec<GetAlbumResponseResultTitle>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumResponse {
    pub text: String,
    pub icon: Option<String>,
    pub params: Option<AlbumResponseParams>,
    pub actions: Option<AlbumResponseActions>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GetAlbumsResponseResult {
    pub item_loop: Vec<AlbumResponse>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumsResponse {
    pub method: String,
    pub result: GetAlbumsResponseResult,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalAlbumResponse {
    pub id: i32,
    pub artists: Option<String>,
    pub artist: String,
    pub album: String,
    pub artwork_track_id: Option<String>,
    pub extid: Option<String>,
    pub year: i16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GetLocalAlbumsResponseResult {
    pub albums_loop: Vec<LocalAlbumResponse>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetLocalAlbumsResponse {
    pub method: String,
    pub result: GetLocalAlbumsResponseResult,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumsRequest {
    pub sources: Option<Vec<AlbumSource>>,
    pub sort: Option<AlbumSort>,
    pub filters: AlbumFilters,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AlbumFilters {
    pub name: Option<String>,
    pub artist: Option<String>,
    pub year: Option<i16>,
    pub search: Option<String>,
}

pub fn filter_albums(albums: Vec<Album>, request: &AlbumsRequest) -> Vec<Album> {
    albums
        .into_iter()
        .filter(|album| {
            !request
                .sources
                .as_ref()
                .is_some_and(|s| !s.contains(&album.source))
        })
        .filter(|album| {
            !request
                .filters
                .name
                .as_ref()
                .is_some_and(|s| !album.title.to_lowercase().contains(s))
        })
        .filter(|album| {
            !request
                .filters
                .artist
                .as_ref()
                .is_some_and(|s| !album.artist.to_lowercase().contains(s))
        })
        .filter(|album| {
            !request
                .filters
                .year
                .as_ref()
                .is_some_and(|y| !album.year.is_some_and(|album_year| &album_year == y))
        })
        .filter(|album| {
            !request.filters.search.as_ref().is_some_and(|s| {
                !(album.title.to_lowercase().contains(s) || album.artist.to_lowercase().contains(s))
            })
        })
        .collect()
}

pub fn sort_albums(mut albums: Vec<Album>, request: &AlbumsRequest) -> Vec<Album> {
    match request.sort {
        Some(AlbumSort::ArtistAsc) => albums.sort_by(|a, b| a.artist.cmp(&b.artist)),
        Some(AlbumSort::NameAsc) => albums.sort_by(|a, b| a.title.cmp(&b.title)),
        Some(AlbumSort::ArtistDesc) => albums.sort_by(|a, b| b.artist.cmp(&a.artist)),
        Some(AlbumSort::NameDesc) => albums.sort_by(|a, b| b.title.cmp(&a.title)),
        _ => (),
    }
    match request.sort {
        Some(AlbumSort::ArtistAsc) => {
            albums.sort_by(|a, b| a.artist.to_lowercase().cmp(&b.artist.to_lowercase()))
        }
        Some(AlbumSort::NameAsc) => {
            albums.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        }
        Some(AlbumSort::ArtistDesc) => {
            albums.sort_by(|a, b| b.artist.to_lowercase().cmp(&a.artist.to_lowercase()))
        }
        Some(AlbumSort::NameDesc) => {
            albums.sort_by(|a, b| b.title.to_lowercase().cmp(&a.title.to_lowercase()))
        }
        Some(AlbumSort::ReleaseDateAsc) => albums.sort_by(|a, b| {
            a.clone()
                .date_released
                .or(a.year.map(|y| y.to_string()))
                .cmp(&b.clone().date_released.or(b.year.map(|y| y.to_string())))
        }),
        Some(AlbumSort::ReleaseDateDesc) => albums.sort_by(|b, a| {
            a.clone()
                .date_released
                .or(a.year.map(|y| y.to_string()))
                .cmp(&b.clone().date_released.or(b.year.map(|y| y.to_string())))
        }),
        None => (),
    }

    albums
}

#[derive(Debug, Error)]
pub enum GetAlbumsError {
    #[error(transparent)]
    Local(#[from] GetLocalAlbumsError),
    #[error(transparent)]
    Tidal(#[from] GetTidalAlbumsError),
    #[error(transparent)]
    Qobuz(#[from] GetQobuzAlbumsError),
    #[error(transparent)]
    DbError(#[from] DbError),
}

pub async fn get_all_albums(
    player_id: &str,
    data: &AppState,
    request: &AlbumsRequest,
) -> Result<Vec<Album>, GetAlbumsError> {
    #[allow(clippy::eq_op)]
    let albums = if 1 == 1 {
        get_albums(&data.db).await?
    } else if request.sources.as_ref().is_some_and(|s| s.len() == 1) {
        let source = request.sources.as_ref().unwrap();
        get_albums_from_source(player_id, data, &source[0])
            .await
            .unwrap()
    } else {
        let sources = match &request.sources {
            Some(s) => s.clone(),
            None => vec![AlbumSource::Local, AlbumSource::Tidal, AlbumSource::Qobuz],
        };

        let requests = sources
            .iter()
            .map(|s| get_albums_from_source(player_id, data, s).boxed_local())
            .collect::<Vec<_>>();

        future::join_all(requests)
            .await
            .into_iter()
            .map(|a: Result<Vec<Album>, GetAlbumsError>| {
                a.unwrap_or_else(|err| {
                    eprintln!("Failed to get albums: {err:?}");
                    vec![]
                })
            })
            .collect::<Vec<_>>()
            .concat()
    };

    Ok(sort_albums(filter_albums(albums, request), request))
}

pub async fn get_albums_from_source(
    player_id: &str,
    data: &AppState,
    source: &AlbumSource,
) -> Result<Vec<Album>, GetAlbumsError> {
    match source {
        AlbumSource::Local => get_local_albums(player_id, data)
            .await
            .map_err(GetAlbumsError::Local),
        AlbumSource::Tidal => get_tidal_albums(player_id, data)
            .await
            .map_err(GetAlbumsError::Tidal),
        AlbumSource::Qobuz => get_qobuz_albums(player_id, data)
            .await
            .map_err(GetAlbumsError::Qobuz),
    }
}

#[derive(Debug, Error)]
pub enum GetLocalAlbumsError {
    #[error(transparent)]
    RequestError(#[from] awc::error::SendRequestError),
    #[error(transparent)]
    JsonError(#[from] awc::error::JsonPayloadError),
}

pub async fn get_local_albums(
    player_id: &str,
    data: &AppState,
) -> Result<Vec<Album>, GetLocalAlbumsError> {
    let proxy_url = &data.proxy_url;
    let request = CacheRequest {
        key: format!("local_albums|{player_id}|{proxy_url}"),
        expiration: Duration::from_secs(60 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        let get_albums_url = format!("{proxy_url}/jsonrpc.js");

        let get_albums_request = serde_json::json!({
            "id": 4,
            "method": "slim.request",
            "params": [
                player_id,
                [
                    "albums",
                    "0",
                    25000,
                    "tags:aajlqsyKSSE",
                    "sort:album",
                    "menu:1",
                    "library_id:0",
                ]
            ]
        });

        Ok::<CacheItemType, GetLocalAlbumsError>(CacheItemType::Albums(
            data.proxy_client
                .post(get_albums_url)
                .send_json(&get_albums_request)
                .await?
                .json::<GetLocalAlbumsResponse>()
                .await?
                .result
                .albums_loop
                .into_iter()
                .filter(|item| item.extid.is_none())
                .map(|item| {
                    let icon = item
                        .artwork_track_id
                        .as_ref()
                        .map(|track_id| format!("/albums/{track_id}/300x300"));
                    Album {
                        id: format!("album_id:{:?}", item.id),
                        title: item.album.clone(),
                        artist: item.artist.clone(),
                        year: Some(item.year),
                        artwork: icon,
                        source: AlbumSource::Local,
                        ..Default::default()
                    }
                })
                .collect(),
        ))
    })
    .await?
    .into_albums()
    .unwrap())
}

#[derive(Debug, Error)]
pub enum GetTidalAlbumsError {
    #[error(transparent)]
    RequestError(#[from] awc::error::SendRequestError),
    #[error(transparent)]
    JsonError(#[from] awc::error::JsonPayloadError),
}

pub async fn get_tidal_albums(
    player_id: &str,
    data: &AppState,
) -> Result<Vec<Album>, GetTidalAlbumsError> {
    let proxy_url = &data.proxy_url;
    let request = CacheRequest {
        key: format!("tidal_albums|{player_id}|{proxy_url}"),
        expiration: Duration::from_secs(60 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        let get_albums_url = format!("{proxy_url}/jsonrpc.js");

        let get_albums_request = serde_json::json!({
            "id": 4,
            "method": "slim.request",
            "params": [
                player_id,
                [
                    "myapps",
                    "items",
                    0,
                    25000,
                    "menu:myapps",
                    "item_id:b26ac9e7.1.1.2"
                ]
            ]
        });

        Ok::<CacheItemType, GetTidalAlbumsError>(CacheItemType::Albums(
            data.proxy_client
                .post(get_albums_url)
                .send_json(&get_albums_request)
                .await?
                .json::<GetAlbumsResponse>()
                .await?
                .result
                .item_loop
                .into_iter()
                .filter(|item| item.params.is_some() || item.actions.is_some())
                .map(|item| {
                    let text_parts = item.text.split('\n').collect::<Vec<&str>>();
                    let id = if let Some(params) = &item.params {
                        format!("item_id:{}", params.item_id)
                    } else if let Some(actions) = &item.actions {
                        format!("item_id:{}", actions.go.params.item_id)
                    } else {
                        unreachable!()
                    };
                    Album {
                        id,
                        title: String::from(text_parts[0]),
                        artist: String::from(text_parts[1]),
                        year: None,
                        artwork: item.icon.clone(),
                        source: AlbumSource::Tidal,
                        ..Default::default()
                    }
                })
                .collect(),
        ))
    })
    .await?
    .into_albums()
    .unwrap())
}

#[derive(Debug, Error)]
pub enum GetQobuzAlbumsError {
    #[error(transparent)]
    RequestError(#[from] awc::error::SendRequestError),
    #[error(transparent)]
    JsonError(#[from] awc::error::JsonPayloadError),
}

pub async fn get_qobuz_albums(
    player_id: &str,
    data: &AppState,
) -> Result<Vec<Album>, GetQobuzAlbumsError> {
    let proxy_url = &data.proxy_url;
    let request = CacheRequest {
        key: format!("qobuz_albums|{player_id}|{proxy_url}"),
        expiration: Duration::from_secs(60 * 60),
    };

    Ok(get_or_set_to_cache(request, || async {
        let get_albums_url = format!("{proxy_url}/jsonrpc.js");

        let get_albums_request = serde_json::json!({
            "id": 4,
            "method": "slim.request",
            "params": [
                player_id,
                [
                    "qobuz",
                    "items",
                    0,
                    25000,
                    "menu:qobuz",
                    "item_id:2.0"
                ]
            ]
        });

        Ok::<CacheItemType, GetQobuzAlbumsError>(CacheItemType::Albums(
            data.proxy_client
                .post(get_albums_url)
                .send_json(&get_albums_request)
                .await?
                .json::<GetAlbumsResponse>()
                .await?
                .result
                .item_loop
                .into_iter()
                .filter(|item| item.params.is_some() || item.actions.is_some())
                .map(|item| {
                    let text_parts = item.text.split('\n').collect::<Vec<&str>>();
                    let artist_and_year = String::from(text_parts[1]);
                    let artist = &artist_and_year[..artist_and_year.len() - 7];
                    let year = &artist_and_year[artist.len() + 2..artist_and_year.len() - 1];
                    let proxy_icon_url = item.icon.clone();
                    let title_and_maybe_star = String::from(text_parts[0]);
                    let title = match title_and_maybe_star.strip_prefix("* ") {
                        Some(title) => String::from(title),
                        None => title_and_maybe_star,
                    };
                    let icon = proxy_icon_url.map(|url| format!("{proxy_url}{url}"));
                    let id = if let Some(params) = &item.params {
                        format!("item_id:{}", params.item_id)
                    } else if let Some(actions) = &item.actions {
                        format!("item_id:{}", actions.go.params.item_id)
                    } else {
                        unreachable!()
                    };
                    Album {
                        id,
                        title,
                        artist: String::from(artist),
                        year: String::from(year).parse::<i16>().ok(),
                        artwork: icon,
                        source: AlbumSource::Qobuz,
                        ..Default::default()
                    }
                })
                .collect(),
        ))
    })
    .await?
    .into_albums()
    .unwrap())
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn filter_albums_empty_albums_returns_empty_albums() {
        let albums = vec![];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    year: None,
                    search: None,
                },
            },
        );
        assert_eq!(result, vec![]);
    }

    #[test]
    fn filter_albums_filters_albums_of_sources_that_dont_match() {
        let local = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "".to_string(),
            year: None,
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let tidal = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "".to_string(),
            year: None,
            artwork: None,
            source: AlbumSource::Tidal,
            ..Default::default()
        };
        let qobuz = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "".to_string(),
            year: None,
            artwork: None,
            source: AlbumSource::Qobuz,
            ..Default::default()
        };
        let albums = vec![local.clone(), tidal, qobuz];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: Some(vec![AlbumSource::Local]),
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    year: None,
                    search: None,
                },
            },
        );
        assert_eq!(result, vec![local]);
    }

    #[test]
    fn filter_albums_filters_albums_of_year_that_dont_match() {
        let album_2020 = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "".to_string(),
            year: Some(2020),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let album_2021 = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "".to_string(),
            year: Some(2021),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let album_2022 = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "".to_string(),
            year: Some(2022),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![album_2020, album_2021.clone(), album_2022];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    year: Some(2021),
                    search: None,
                },
            },
        );
        assert_eq!(result, vec![album_2021]);
    }

    #[test]
    fn filter_albums_filters_albums_of_name_that_dont_match() {
        let bob = Album {
            id: "".to_string(),
            title: "bob".to_string(),
            artist: "".to_string(),
            year: Some(2020),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: "".to_string(),
            title: "sally".to_string(),
            artist: "".to_string(),
            year: Some(2021),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: "".to_string(),
            title: "test".to_string(),
            artist: "".to_string(),
            year: Some(2022),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: Some("test".to_string()),
                    artist: None,
                    year: None,
                    search: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_name_that_dont_match_and_searches_multiple_words() {
        let bob = Album {
            id: "".to_string(),
            title: "bob".to_string(),
            artist: "".to_string(),
            year: Some(2020),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: "".to_string(),
            title: "sally".to_string(),
            artist: "".to_string(),
            year: Some(2021),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: "".to_string(),
            title: "one test two".to_string(),
            artist: "".to_string(),
            year: Some(2022),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: Some("test".to_string()),
                    artist: None,
                    year: None,
                    search: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_artist_that_dont_match() {
        let bob = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "bob".to_string(),
            year: Some(2020),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "sally".to_string(),
            year: Some(2021),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "test".to_string(),
            year: Some(2022),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: Some("test".to_string()),
                    year: None,
                    search: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_artist_that_dont_match_and_searches_multiple_words() {
        let bob = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "bob".to_string(),
            year: Some(2020),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "sally".to_string(),
            year: Some(2021),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "one test two".to_string(),
            year: Some(2022),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: Some("test".to_string()),
                    year: None,
                    search: None,
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_artist() {
        let bob = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "bob".to_string(),
            year: Some(2020),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "sally".to_string(),
            year: Some(2021),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "test".to_string(),
            year: Some(2022),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    year: None,
                    search: Some("test".to_string()),
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_artist_and_searches_multiple_words() {
        let bob = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "bob".to_string(),
            year: Some(2020),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "sally".to_string(),
            year: Some(2021),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: "".to_string(),
            title: "".to_string(),
            artist: "one test two".to_string(),
            year: Some(2022),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    year: None,
                    search: Some("test".to_string()),
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_name() {
        let bob = Album {
            id: "".to_string(),
            title: "bob".to_string(),
            artist: "".to_string(),
            year: Some(2020),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: "".to_string(),
            title: "sally".to_string(),
            artist: "".to_string(),
            year: Some(2021),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: "".to_string(),
            title: "test".to_string(),
            artist: "".to_string(),
            year: Some(2022),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    year: None,
                    search: Some("test".to_string()),
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_name_and_searches_multiple_words() {
        let bob = Album {
            id: "".to_string(),
            title: "bob".to_string(),
            artist: "".to_string(),
            year: Some(2020),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: "".to_string(),
            title: "sally".to_string(),
            artist: "".to_string(),
            year: Some(2021),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: "".to_string(),
            title: "one test two".to_string(),
            artist: "".to_string(),
            year: Some(2022),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob, sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    year: None,
                    search: Some("test".to_string()),
                },
            },
        );
        assert_eq!(result, vec![test]);
    }

    #[test]
    fn filter_albums_filters_albums_of_search_that_dont_match_and_searches_across_properties() {
        let bob = Album {
            id: "".to_string(),
            title: "bob".to_string(),
            artist: "test".to_string(),
            year: Some(2020),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let sally = Album {
            id: "".to_string(),
            title: "sally".to_string(),
            artist: "".to_string(),
            year: Some(2021),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let test = Album {
            id: "".to_string(),
            title: "one test two".to_string(),
            artist: "".to_string(),
            year: Some(2022),
            artwork: None,
            source: AlbumSource::Local,
            ..Default::default()
        };
        let albums = vec![bob.clone(), sally, test.clone()];
        let result = filter_albums(
            albums,
            &AlbumsRequest {
                sources: None,
                sort: None,
                filters: AlbumFilters {
                    name: None,
                    artist: None,
                    year: None,
                    search: Some("test".to_string()),
                },
            },
        );
        assert_eq!(result, vec![bob, test]);
    }
}
