use crate::{
    app::AppState,
    cache::{get_or_set_to_cache, CacheItemType, CacheRequest},
};

use core::panic;
use std::time::Duration;

use actix_web::web;
use futures::future;
use serde::{de::Error, Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: Option<i32>,
    pub icon: Option<String>,
    pub source: AlbumSource,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum AlbumSource {
    Local,
    Tidal,
    Qobuz,
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
pub struct AlbumResponse {
    pub text: String,
    pub icon: String,
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
    pub year: i32,
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

pub async fn get_all_albums(
    player_id: &str,
    data: web::Data<AppState>,
) -> serde_json::Result<Vec<Album>> {
    let (local, tidal, qobuz) = future::join3(
        get_local_albums(player_id, data.clone()),
        get_tidal_albums(player_id, data.clone()),
        get_qobuz_albums(player_id, data),
    )
    .await;

    Ok([local.unwrap(), tidal.unwrap(), qobuz.unwrap()].concat())
}

pub async fn get_local_albums(
    player_id: &str,
    data: web::Data<AppState>,
) -> serde_json::Result<Vec<Album>> {
    let proxy_url = &data.proxy_url;
    let request = CacheRequest {
        key: format!("local_albums|{player_id}|{proxy_url}"),
        expiration: Duration::from_secs(60 * 60),
    };

    match get_or_set_to_cache(request, || async {
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

        let album_items = match data
            .proxy_client
            .post(get_albums_url)
            .timeout(Duration::from_secs(100))
            .send_json(&get_albums_request)
            .await
        {
            Ok(mut res) => match res.json::<GetLocalAlbumsResponse>().await {
                Ok(json) => json.result.albums_loop,
                Err(error) => {
                    panic!("Failed to deserialize GetLocalAlbumsResponse: {:?}", error)
                }
            },
            Err(error) => panic!("Request failure: {:?}", error),
        };

        let albums = album_items
            .iter()
            .filter(|item| item.extid.is_none())
            .map(|item| {
                let icon = item
                    .artwork_track_id
                    .as_ref()
                    .map(|track_id| format!("albums/{track_id}/300x300"));
                Album {
                    id: format!("album_id:{:?}", item.id),
                    title: item.album.clone(),
                    artist: item.artist.clone(),
                    year: Some(item.year),
                    icon,
                    source: AlbumSource::Local,
                }
            })
            .collect();

        CacheItemType::Albums(albums)
    })
    .await
    .into_albums()
    {
        Ok(albums) => Ok(albums),
        Err(error) => {
            Err(format!("Error fetching albums: {:?}", error)).map_err(serde_json::Error::custom)
        }
    }
}

pub async fn get_tidal_albums(
    player_id: &str,
    data: web::Data<AppState>,
) -> serde_json::Result<Vec<Album>> {
    let proxy_url = &data.proxy_url;
    let request = CacheRequest {
        key: format!("tidal_albums|{player_id}|{proxy_url}"),
        expiration: Duration::from_secs(60 * 60),
    };

    match get_or_set_to_cache(request, || async {
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
                    "item_id:7342f28d.4.1.2"
                ]
            ]
        });

        let album_items = match data
            .proxy_client
            .post(get_albums_url)
            .timeout(Duration::from_secs(100))
            .send_json(&get_albums_request)
            .await
        {
            Ok(mut res) => match res.json::<GetAlbumsResponse>().await {
                Ok(json) => json.result.item_loop,
                Err(error) => panic!("Failed to deserialize GetAlbumsResponse: {:?}", error),
            },
            Err(error) => panic!("Request failure: {:?}", error),
        };

        let albums = album_items
            .iter()
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
                    icon: Some(item.icon.clone()),
                    source: AlbumSource::Tidal,
                }
            })
            .collect();

        CacheItemType::Albums(albums)
    })
    .await
    .into_albums()
    {
        Ok(albums) => Ok(albums),
        Err(error) => {
            Err(format!("Error fetching albums: {:?}", error)).map_err(serde_json::Error::custom)
        }
    }
}

pub async fn get_qobuz_albums(
    player_id: &str,
    data: web::Data<AppState>,
) -> serde_json::Result<Vec<Album>> {
    let proxy_url = &data.proxy_url;
    let request = CacheRequest {
        key: format!("qobuz_albums|{player_id}|{proxy_url}"),
        expiration: Duration::from_secs(60 * 60),
    };

    match get_or_set_to_cache(request, || async {
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

        let album_items = match data
            .proxy_client
            .post(get_albums_url)
            .timeout(Duration::from_secs(100))
            .send_json(&get_albums_request)
            .await
        {
            Ok(mut res) => match res.json::<GetAlbumsResponse>().await {
                Ok(json) => json.result.item_loop,
                Err(error) => panic!("Failed to deserialize qobuz GetAlbumsResponse: {:?}", error),
            },
            Err(error) => panic!("Request failure: {:?}", error),
        };

        let albums = album_items
            .iter()
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
                let icon = Some(format!("{proxy_url}{proxy_icon_url}"));
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
                    year: String::from(year).parse::<i32>().ok(),
                    icon,
                    source: AlbumSource::Qobuz,
                }
            })
            .collect();

        CacheItemType::Albums(albums)
    })
    .await
    .into_albums()
    {
        Ok(albums) => Ok(albums),
        Err(error) => {
            Err(format!("Error fetching albums: {:?}", error)).map_err(serde_json::Error::custom)
        }
    }
}
