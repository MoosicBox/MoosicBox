use crate::app::AppState;
use crate::slim::menu::{get_all_albums, Album, AlbumFilters, AlbumSort, AlbumSource};
use crate::slim::player::{
    connect, get_players, get_playlist_status, get_status, handshake, ping, play_album,
    player_next_track, player_pause, player_play, player_previous_track, player_start_track,
    set_player_status, PingResponse, PlaylistStatus, Status,
};
use crate::sqlite::menu::{get_album, FullAlbum, GetAlbumError};
use actix_web::error::{ErrorBadRequest, ErrorNotFound};
use actix_web::http::StatusCode;
use actix_web::HttpResponse;
use actix_web::{
    get, post,
    web::{self, Json},
    Result,
};
use core::panic;
use serde::Deserialize;
use serde_json::Value;
use std::str::FromStr;
use std::thread;
use std::time::Duration;

#[post("/connect")]
pub async fn connect_endpoint(data: web::Data<AppState>) -> Result<Json<Value>> {
    let client_id = match handshake(data.clone()).await {
        Ok(handshake) => handshake.client_id.clone(),
        Err(error) => panic!("Failed to handshake: {:?}", error),
    };

    match connect(client_id.clone(), data.clone()).await {
        Ok(json) => json,
        Err(error) => panic!("Failed to connect: {:?}", error),
    };

    let player_ids: Vec<String> = get_players(client_id.clone(), data)
        .await
        .unwrap()
        .iter()
        .map(|p| p.player_id.clone())
        .collect();

    Ok(Json(serde_json::json!({
        "clientId": client_id,
        "players": player_ids,
    })))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StatusRequest {}

#[get("/status")]
pub async fn status_endpoint(
    _query: web::Query<StatusRequest>,
    data: web::Data<AppState>,
) -> Result<Json<Status>> {
    let status_response = match get_status(data).await {
        Ok(resp) => resp,
        Err(error) => panic!("Failed to get status: {:?}", error),
    };

    Ok(Json(status_response))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistStatusRequest {
    player_id: String,
}

#[get("/playlist/status")]
pub async fn playlist_status_endpoint(
    query: web::Query<PlaylistStatusRequest>,
    data: web::Data<AppState>,
) -> Result<Json<PlaylistStatus>> {
    let playlist_status_response = match get_playlist_status(query.player_id.clone(), data).await {
        Ok(resp) => resp,
        Err(error) => panic!("Failed to get status: {:?}", error),
    };

    Ok(Json(playlist_status_response))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PingQuery {
    client_id: String,
}

#[post("/ping")]
pub async fn ping_endpoint(
    query: web::Query<PingQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    let ping_response = match ping(query.client_id.clone(), data).await {
        Ok(resp) => resp,
        Err(error) => panic!("Failed to ping: {:?}", error),
    };

    let successful = match &ping_response[0] {
        PingResponse::ResponseStatus(x) => x.successful,
        _ => panic!("Invalid ping response data"),
    };

    Ok(Json(serde_json::json!({"alive": successful})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumQuery {
    player_id: String,
    album_id: i32,
}

#[get("/album")]
pub async fn get_album_endpoint(
    query: web::Query<GetAlbumQuery>,
    data: web::Data<AppState>,
) -> Result<Json<FullAlbum>> {
    let player_id = &query.player_id;
    let album_id = query.album_id;

    match get_album(player_id, album_id, &data).await {
        Ok(resp) => Ok(Json(resp)),
        Err(error) => match error {
            GetAlbumError::AlbumNotFound { .. } => Err(ErrorNotFound(error.to_string())),
            _ => panic!("Failed to get album: {:?}", error),
        },
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetAlbumsQuery {
    player_id: String,
    sources: Option<String>,
    sort: Option<String>,
}

#[get("/albums")]
pub async fn get_albums_endpoint(
    query: web::Query<GetAlbumsQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Vec<Album>>> {
    let player_id = &query.player_id;
    let filters = AlbumFilters {
        sources: query
            .sources
            .as_ref()
            .map(|sources| {
                sources
                    .split(',')
                    .map(|s| s.trim())
                    .map(|s| {
                        AlbumSource::from_str(s)
                            .map_err(|_e| ErrorBadRequest(format!("Invalid sort value: {s}")))
                    })
                    .collect()
            })
            .transpose()?,
        sort: query
            .sort
            .as_ref()
            .map(|sort| {
                AlbumSort::from_str(sort)
                    .map_err(|_e| ErrorBadRequest(format!("Invalid sort value: {sort}")))
            })
            .transpose()?,
    };

    match get_all_albums(player_id, &data, &filters).await {
        Ok(resp) => Ok(Json(resp)),
        Err(error) => panic!("Failed to get albums: {:?}", error),
    }
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetPlayersQuery {
    client_id: String,
}

#[get("/playback/players")]
pub async fn get_players_endpoint(
    query: web::Query<GetPlayersQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    let player_ids: Vec<String> = get_players(query.client_id.clone(), data)
        .await
        .unwrap()
        .iter()
        .map(|p| p.player_id.clone())
        .collect();

    Ok(Json(serde_json::json!({
        "players": player_ids,
    })))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PausePlayerQuery {
    player_id: String,
}

#[post("/playback/pause")]
pub async fn pause_player_endpoint(
    query: web::Query<PausePlayerQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    match player_pause(query.player_id.clone(), data).await {
        Ok(json) => json,
        Err(_) => panic!("Failed to pause player"),
    };

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayPlayerQuery {
    player_id: String,
}

#[post("/playback/play")]
pub async fn play_player_endpoint(
    query: web::Query<PlayPlayerQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    match player_play(query.player_id.clone(), data).await {
        Ok(json) => json,
        Err(_) => panic!("Failed to play player"),
    };

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayAlbumQuery {
    player_id: String,
    album_id: String,
}

#[post("/playback/play-album")]
pub async fn play_album_endpoint(
    query: web::Query<PlayAlbumQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    match play_album(query.player_id.clone(), query.album_id.clone(), data).await {
        Ok(json) => json,
        Err(error) => panic!("Failed to play album {:?}", error),
    };

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayerNextTrackQuery {
    player_id: String,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayerStartTrackQuery {
    player_id: String,
}

#[post("/playback/start-track")]
pub async fn player_start_track_endpoint(
    query: web::Query<PlayerStartTrackQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    match player_start_track(query.player_id.clone(), data).await {
        Ok(json) => json,
        Err(_) => panic!("Failed to go to player start track"),
    };

    Ok(Json(serde_json::json!({"success": true})))
}

#[post("/playback/next-track")]
pub async fn player_next_track_endpoint(
    query: web::Query<PlayerNextTrackQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    match player_next_track(query.player_id.clone(), data).await {
        Ok(json) => json,
        Err(_) => panic!("Failed to go to player next track"),
    };

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlayerPreviousTrackQuery {
    player_id: String,
}

#[post("/playback/previous-track")]
pub async fn player_previous_track_endpoint(
    query: web::Query<PlayerPreviousTrackQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    match player_previous_track(query.player_id.clone(), data).await {
        Ok(json) => json,
        Err(_) => panic!("Failed to go to player previous track"),
    };

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StartPlayerQuery {
    player_id: String,
}

#[post("/playback/start-player")]
pub async fn start_player_endpoint(
    query: web::Query<StartPlayerQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    match set_player_status(query.player_id.clone(), String::from("1"), data).await {
        Ok(json) => json,
        Err(_) => panic!("Failed to start player"),
    };

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StopPlayerQuery {
    player_id: String,
}

#[post("/playback/stop-player")]
pub async fn stop_player_endpoint(
    query: web::Query<StopPlayerQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    match set_player_status(query.player_id.clone(), String::from("0"), data).await {
        Ok(json) => json,
        Err(_) => panic!("Failed to stop player"),
    };

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RestartPlayerQuery {
    player_id: String,
}

#[post("/playback/restart-player")]
pub async fn restart_player_endpoint(
    query: web::Query<RestartPlayerQuery>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    set_player_status(query.player_id.clone(), String::from("0"), data.clone())
        .await
        .unwrap();

    thread::sleep(Duration::from_millis(500));

    set_player_status(query.player_id.clone(), String::from("1"), data.clone())
        .await
        .unwrap();

    Ok(Json(serde_json::json!({"success": true})))
}

#[post("/proxy/{proxy:.*}")]
pub async fn proxy_post_endpoint(
    body: String,
    path: web::Path<(String,)>,
    data: web::Data<AppState>,
) -> Result<Json<Value>> {
    let proxy_url = &data.proxy_url;
    let proxy_value = path.into_inner().0;
    let request_url = format!("{proxy_url}/{proxy_value}");

    let response = match data.proxy_client.post(request_url).send_body(body).await {
        Ok(mut res) => match res.json::<serde_json::Value>().await {
            Ok(json) => json,
            Err(_) => panic!("Deserialization failure"),
        },
        Err(_) => panic!("Request failure"),
    };

    Ok(Json(response))
}

#[get("/proxy/{proxy:.*}")]
pub async fn proxy_get_endpoint(
    path: web::Path<(String,)>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let proxy_url = &data.proxy_url;
    let proxy_value = path.into_inner().0;
    let request_url = format!("{proxy_url}/{proxy_value}");

    let mut res = match data.proxy_client.get(request_url).send().await {
        Ok(res) => res,
        Err(error) => panic!("Request failure {:?}", error),
    };

    let mut request_builder = HttpResponse::build(StatusCode::OK);

    res.headers().iter().for_each(|header| {
        request_builder.append_header(header);
    });

    let body = match res.body().await {
        Ok(bytes) => bytes,
        Err(error) => panic!("Deserialization failure {:?}", error),
    };

    Ok(request_builder.body(body))
}

#[get("/image/{url:.*}")]
pub async fn image_proxy_endpoint(
    path: web::Path<(String,)>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let paths = path.into_inner();
    let url = paths.0;

    let mut res = match data.image_client.get(url).send().await {
        Ok(res) => res,
        Err(error) => panic!("Request failure {:?}", error),
    };

    let content_type = String::from(
        res.headers()
            .get("content_type")
            .map(|ctype| ctype.to_str().unwrap())
            .unwrap_or("image/jpeg"),
    );

    let body = match res.body().await {
        Ok(bytes) => bytes,
        Err(error) => panic!("Deserialization failure {:?}", error),
    };

    Ok(HttpResponse::build(StatusCode::OK)
        .content_type(content_type)
        .body(body))
}

#[get("/albums/{album_id}/{size}")]
pub async fn album_icon_endpoint(
    path: web::Path<(String, String)>,
    data: web::Data<AppState>,
) -> Result<HttpResponse> {
    let proxy_url = &data.proxy_url;
    let paths = path.into_inner();
    let album_id = paths.0;
    let size = paths.1;

    let dimensions = size.split('x').collect::<Vec<&str>>();

    let width = dimensions[0].parse::<i32>().unwrap();
    let height = dimensions[1].parse::<i32>().unwrap();

    let request_url = format!("{proxy_url}/music/{album_id}/cover_{width}x{height}_f");

    let mut res = match data.proxy_client.get(request_url).send().await {
        Ok(res) => res,
        Err(error) => panic!("Request failure {:?}", error),
    };

    let content_type = String::from(
        res.headers()
            .get("content_type")
            .map(|ctype| ctype.to_str().unwrap())
            .unwrap_or("image/jpeg"),
    );

    let body = match res.body().await {
        Ok(bytes) => bytes,
        Err(error) => panic!("Deserialization failure {:?}", error),
    };

    Ok(HttpResponse::build(StatusCode::OK)
        .content_type(content_type)
        .body(body))
}
