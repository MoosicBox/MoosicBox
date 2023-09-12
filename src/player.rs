use crate::{
    app::AppState,
    cache::{get_or_set_to_cache, CacheItemType, CacheRequest},
};

use core::panic;
use std::time::Duration;

use actix_web::web;
use futures::future;
use serde::{de::Error, Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ConnectResponse {
    pub client_id: String,
    pub channel: String,
    pub id: String,
    pub subscription: String,
    pub successful: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StatusResponseResult {
    #[serde(rename = "other player count")]
    pub other_player_count: i32,

    pub players_loop: Vec<PlayerResponse>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub players: Vec<Player>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StatusResponse {
    pub method: String,
    pub result: StatusResponseResult,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistStatus {
    pub tracks: Vec<Track>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub title: String,
    pub icon: String,
    pub album: String,
    pub artist: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistStatusResponse {
    pub method: String,
    pub result: PlaylistStatusResponseResult,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlaylistStatusResponseResult {
    pub playlist_loop: Vec<TrackResponse>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TrackResponse {
    pub title: String,
    pub artwork_url: String,
    pub album: String,
    pub artist: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PingResponseStatus {
    pub timestamp: String,
    pub client_id: Option<String>,
    pub channel: String,
    pub id: String,
    pub successful: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PingResponseDataWrapper {
    pub data: PingResponseData,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PingResponseData {
    pub players_loop: Vec<PlayerResponse>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum PingResponse {
    ResponseStatus(PingResponseStatus),
    ResponseDataWrapper(PingResponseDataWrapper),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeResponse {
    pub client_id: String,
}

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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPlayersResponseStatus {
    pub client_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetPlayersResponseDataWrapper {
    pub data: GetPlayersResponseData,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetPlayersResponseData {
    pub players_loop: Vec<PlayerResponse>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PlayerResponse {
    #[serde(rename = "playerid")]
    pub player_id: String,

    #[serde(rename = "isplaying")]
    pub is_playing: i32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Player {
    pub player_id: String,

    pub is_playing: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum GetPlayersResponse {
    ResponseStatus(GetPlayersResponseStatus),
    ResponseDataWrapper(GetPlayersResponseDataWrapper),
}

pub async fn player_pause(
    player_id: String,
    data: web::Data<AppState>,
) -> serde_json::Result<Value> {
    let proxy_url = &data.proxy_url;
    let request_url = format!("{proxy_url}/jsonrpc.js");

    let request = serde_json::json!({
        "id": 0,
        "method": "slim.request",
        "params": [
            player_id,
            [
                "pause",
            ]
        ],
    });

    let client = awc::Client::default();

    let response = match client.post(request_url).send_json(&request).await {
        Ok(mut res) => match res.json::<serde_json::Value>().await {
            Ok(json) => json,
            Err(error) => panic!(
                "Failed to deserialize set player status response: {:?}",
                error
            ),
        },
        Err(error) => panic!("Request failure: {:?}", error),
    };

    Ok(response)
}

pub async fn player_start_track(
    player_id: String,
    data: web::Data<AppState>,
) -> serde_json::Result<Value> {
    let proxy_url = &data.proxy_url;
    let request_url = format!("{proxy_url}/jsonrpc.js");

    let request = serde_json::json!({
        "id": 0,
        "method": "slim.request",
        "params": [
            player_id,
            [
                "button",
                "jump_new",
            ]
        ],
    });

    let client = awc::Client::default();

    let response = match client.post(request_url).send_json(&request).await {
        Ok(mut res) => match res.json::<serde_json::Value>().await {
            Ok(json) => json,
            Err(error) => panic!(
                "Failed to deserialize player start track response: {:?}",
                error
            ),
        },
        Err(error) => panic!("Request failure: {:?}", error),
    };

    Ok(response)
}

pub async fn player_previous_track(
    player_id: String,
    data: web::Data<AppState>,
) -> serde_json::Result<Value> {
    let proxy_url = &data.proxy_url;
    let request_url = format!("{proxy_url}/jsonrpc.js");

    let request = serde_json::json!({
        "id": 0,
        "method": "slim.request",
        "params": [
            player_id,
            [
                "playlist",
                "index",
                "-1",
            ]
        ],
    });

    let client = awc::Client::default();

    let response = match client.post(request_url).send_json(&request).await {
        Ok(mut res) => match res.json::<serde_json::Value>().await {
            Ok(json) => json,
            Err(error) => panic!(
                "Failed to deserialize player previous track response: {:?}",
                error
            ),
        },
        Err(error) => panic!("Request failure: {:?}", error),
    };

    Ok(response)
}

pub async fn player_next_track(
    player_id: String,
    data: web::Data<AppState>,
) -> serde_json::Result<Value> {
    let proxy_url = &data.proxy_url;
    let request_url = format!("{proxy_url}/jsonrpc.js");

    let request = serde_json::json!({
        "id": 0,
        "method": "slim.request",
        "params": [
            player_id,
            [
                "playlist",
                "index",
                "+1",
            ]
        ],
    });

    let client = awc::Client::default();

    let response = match client.post(request_url).send_json(&request).await {
        Ok(mut res) => match res.json::<serde_json::Value>().await {
            Ok(json) => json,
            Err(error) => panic!(
                "Failed to deserialize player next track response: {:?}",
                error
            ),
        },
        Err(error) => panic!("Request failure: {:?}", error),
    };

    Ok(response)
}

pub async fn play_album(
    player_id: String,
    album_id: String,
    data: web::Data<AppState>,
) -> serde_json::Result<Value> {
    let proxy_url = &data.proxy_url;
    let request_url = format!("{proxy_url}/jsonrpc.js");

    let request = if album_id.starts_with("album_id:") {
        serde_json::json!({
            "id": 0,
            "method": "slim.request",
            "params": [
                player_id,
                [
                    "playlistcontrol",
                    "cmd:load",
                    album_id,
                    "library_id:0",
                ]
            ],
        })
    } else {
        serde_json::json!({
            "id": 0,
            "method": "slim.request",
            "params": [
                player_id,
                [
                    "myapps",
                    "playlist",
                    "play",
                    "menu:myapps",
                    "isContextMenu:1",
                    album_id,
                ]
            ],
        })
    };

    let response = match data
        .proxy_client
        .post(request_url)
        .send_json(&request)
        .await
    {
        Ok(mut res) => match res.json::<serde_json::Value>().await {
            Ok(json) => json,
            Err(error) => panic!("Failed to deserialize play album response: {:?}", error),
        },
        Err(error) => panic!("Request failure: {:?}", error),
    };

    Ok(response)
}

pub async fn player_play(
    player_id: String,
    data: web::Data<AppState>,
) -> serde_json::Result<Value> {
    let proxy_url = &data.proxy_url;
    let request_url = format!("{proxy_url}/jsonrpc.js");

    let request = serde_json::json!({
        "id": 0,
        "method": "slim.request",
        "params": [
            player_id,
            [
                "play",
            ]
        ],
    });

    let client = awc::Client::default();

    let response = match client.post(request_url).send_json(&request).await {
        Ok(mut res) => match res.json::<serde_json::Value>().await {
            Ok(json) => json,
            Err(error) => panic!(
                "Failed to deserialize set player status response: {:?}",
                error
            ),
        },
        Err(error) => panic!("Request failure: {:?}", error),
    };

    Ok(response)
}

pub async fn set_player_status(
    player_id: String,
    status: String,
    data: web::Data<AppState>,
) -> serde_json::Result<Value> {
    let proxy_url = &data.proxy_url;
    let request_url = format!("{proxy_url}/jsonrpc.js");

    let request = serde_json::json!({
        "id": 0,
        "method": "slim.request",
        "params": [
            player_id,
            [
                "power",
                status,
            ]
        ],
    });

    let client = awc::Client::default();

    let response = match client.post(request_url).send_json(&request).await {
        Ok(mut res) => match res.json::<serde_json::Value>().await {
            Ok(json) => json,
            Err(error) => panic!(
                "Failed to deserialize set player status response: {:?}",
                error
            ),
        },
        Err(error) => panic!("Request failure: {:?}", error),
    };

    Ok(response)
}

pub async fn handshake(data: web::Data<AppState>) -> serde_json::Result<HandshakeResponse> {
    let proxy_url = &data.proxy_url;
    let handshake_url = format!("{proxy_url}/cometd/handshake");

    let handshake_request = serde_json::json!([
        {
            "id": "1",
            "version": "1.0",
            "minimumVersion": "1.0",
            "channel": "/meta/handshake",
            "supportedConnectionTypes": [
                "long-polling"
            ],
            "advice": {
                "timeout": 60000,
                "interval": 0
            }
        }
    ]);

    let handshake_response = match data
        .proxy_client
        .post(handshake_url)
        .send_json(&handshake_request)
        .await
    {
        Ok(mut res) => match res.json::<Vec<HandshakeResponse>>().await {
            Ok(json) => match json.len() {
                1 => json[0].clone(),
                _ => return Err("Invalid").map_err(serde_json::Error::custom),
            },
            Err(error) => panic!("Failed to deserialize handshake: {:?}", error),
        },
        Err(error) => panic!("Request failure: {:?}", error),
    };

    Ok(handshake_response)
}

pub async fn connect(
    client_id: String,
    data: web::Data<AppState>,
) -> serde_json::Result<ConnectResponse> {
    let proxy_url = &data.proxy_url;
    let connect_url = format!("{proxy_url}/cometd");

    let connect_request = serde_json::json!([
        {
            "id": "2",
            "channel": "/meta/subscribe",
            "subscription": format!("/{client_id}/**"),
            "clientId": client_id,
        }
    ]);

    let connect_response: ConnectResponse = match data
        .proxy_client
        .post(connect_url)
        .send_json(&connect_request)
        .await
    {
        Ok(mut res) => match res.json::<Vec<ConnectResponse>>().await {
            Ok(json) => match json.len() {
                1 => json[0].clone(),
                _ => return Err("Invalid").map_err(serde_json::Error::custom),
            },
            Err(error) => panic!("Failed to deserialize connection response: {:?}", error),
        },
        Err(error) => panic!("Request failure: {:?}", error),
    };

    Ok(connect_response)
}

pub async fn get_status(data: web::Data<AppState>) -> serde_json::Result<Status> {
    let proxy_url = &data.proxy_url;
    let status_url = format!("{proxy_url}/jsonrpc.js");

    let status_request = serde_json::json!({
        "id": 0,
        "method": "slim.request",
        "params": [
            "",
            [
                "serverstatus",
                0,
                100
            ]
        ]
    });

    let status_response = match data
        .proxy_client
        .post(status_url)
        .timeout(Duration::from_secs(100))
        .send_json(&status_request)
        .await
    {
        Ok(mut res) => match res.json::<StatusResponse>().await {
            Ok(json) => json,
            Err(error) => panic!("Failed to deserialize status response: {:?}", error),
        },
        Err(error) => panic!("Request failure: {:?}", error),
    };

    Ok(Status {
        players: status_response
            .result
            .players_loop
            .iter()
            .map(|p| Player {
                player_id: p.player_id.clone(),
                is_playing: p.is_playing == 1,
            })
            .collect(),
    })
}

pub async fn get_playlist_status(
    player_id: String,
    data: web::Data<AppState>,
) -> serde_json::Result<PlaylistStatus> {
    let proxy_url = &data.proxy_url;
    let playlist_status_url = format!("{proxy_url}/jsonrpc.js");

    let playlist_status_request = serde_json::json!({
        "id": 0,
        "method": "slim.request",
        "params": [
            player_id,
            [
                "status",
                "-",
                1,
                "tags:cdegiloqrstuyAABGIKNST"
            ]
        ]
    });

    let playlist_status_response = match data
        .proxy_client
        .post(playlist_status_url)
        .timeout(Duration::from_secs(100))
        .send_json(&playlist_status_request)
        .await
    {
        Ok(mut res) => match res.json::<PlaylistStatusResponse>().await {
            Ok(json) => json,
            Err(error) => panic!(
                "Failed to deserialize playlist status response: {:?}",
                error
            ),
        },
        Err(error) => panic!("Request failure: {:?}", error),
    };

    Ok(PlaylistStatus {
        tracks: playlist_status_response
            .result
            .playlist_loop
            .iter()
            .map(|p| Track {
                icon: p.artwork_url.clone(),
                album: p.album.clone(),
                artist: p.artist.clone(),
                title: p.title.clone(),
            })
            .collect(),
    })
}

pub async fn ping(
    client_id: String,
    data: web::Data<AppState>,
) -> serde_json::Result<Vec<PingResponse>> {
    let proxy_url = &data.proxy_url;
    let ping_url = format!("{proxy_url}/cometd/connect");

    let ping_request = serde_json::json!([
        {
            "id": "0",
            "channel": "/meta/connect",
            "connectionType": "long-polling",
            "clientId": client_id,
        }
    ]);

    let ping_response = match data
        .proxy_client
        .post(ping_url)
        .timeout(Duration::from_secs(100))
        .send_json(&ping_request)
        .await
    {
        Ok(mut res) => match res.json::<Vec<PingResponse>>().await {
            Ok(json) => json,
            Err(error) => panic!("Failed to deserialize ping response: {:?}", error),
        },
        Err(error) => panic!("Request failure: {:?}", error),
    };

    Ok(ping_response)
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
                    "item_id:6b154c36.4.1.2"
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

pub async fn get_players(
    client_id: String,
    data: web::Data<AppState>,
) -> serde_json::Result<Vec<PlayerResponse>> {
    let proxy_url = &data.proxy_url;
    let get_players_url = format!("{proxy_url}/cometd");

    let get_players_request = serde_json::json!([
        {
            "data": {
                "response": format!("/{client_id}/slim/serverstatus"),
                "request": [
                    "",
                    [
                        "serverstatus",
                        0,
                        100,
                        "subscribe:60"
                    ]
                ]
            },
            "id": "3",
            "channel": "/slim/subscribe",
            "clientId": client_id,
        }
    ]);

    let get_players_response = match data
        .proxy_client
        .post(get_players_url)
        .send_json(&get_players_request)
        .await
    {
        Ok(mut res) => match res.json::<Vec<GetPlayersResponse>>().await {
            Ok(json) => json,
            Err(error) => panic!("Failed to deserialize GetPlayersResponse: {:?}", error),
        },
        Err(error) => panic!("Request failure: {:?}", error),
    };

    let players = match &get_players_response[1] {
        GetPlayersResponse::ResponseDataWrapper(x) => x.data.players_loop.clone(),
        _ => panic!("Invalid get players response data"),
    };

    Ok(players)
}
