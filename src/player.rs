use crate::app::AppState;

use core::panic;
use std::time::Duration;

use actix_web::web;
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
    pub text: String,
    pub icon: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AlbumResponse {
    pub text: String,
    pub icon: String,
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

pub async fn get_albums(
    player_id: String,
    data: web::Data<AppState>,
) -> serde_json::Result<Vec<Album>> {
    let proxy_url = &data.proxy_url;
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
        .map(|item| Album {
            text: item.text.clone(),
            icon: item.icon.clone(),
        })
        .collect();

    Ok(albums)
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
