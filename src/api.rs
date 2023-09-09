use crate::player::{
    connect, get_players, get_status, handshake, ping, player_pause, player_play,
    set_player_status, PingResponse,
};

use crate::app::AppState;

use core::panic;

use actix_web::{
    get, post,
    web::{self, Json},
    Responder,
};
use serde::Deserialize;
use serde_json::Result;

#[post("/connect")]
pub async fn connect_endpoint(data: web::Data<AppState>) -> Result<impl Responder> {
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
) -> Result<impl Responder> {
    let status_response = match get_status(data).await {
        Ok(resp) => resp,
        Err(error) => panic!("Failed to get status: {:?}", error),
    };

    Ok(Json(status_response))
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
) -> Result<impl Responder> {
    let ping_response = match ping(query.client_id.clone(), data).await {
        Ok(resp) => resp,
        Err(error) => panic!("Failed to ping: {:?}", error),
    };

    let successful = match &ping_response[0] {
        PingResponse::ResponseStatus(x) => x.successful,
        _ => panic!("Invalid ping response data"),
    };

    Ok(Json(serde_json::json!({"success": successful})))
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
) -> Result<impl Responder> {
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
) -> Result<impl Responder> {
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
) -> Result<impl Responder> {
    match player_play(query.player_id.clone(), data).await {
        Ok(json) => json,
        Err(_) => panic!("Failed to play player"),
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
) -> Result<impl Responder> {
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
) -> Result<impl Responder> {
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
) -> Result<impl Responder> {
    set_player_status(query.player_id.clone(), String::from("0"), data.clone())
        .await
        .unwrap();
    set_player_status(query.player_id.clone(), String::from("1"), data.clone())
        .await
        .unwrap();

    Ok(Json(serde_json::json!({"success": true})))
}

#[post("/proxy/{proxy:.*}")]
pub async fn proxy_endpoint(
    body: String,
    path: web::Path<(String,)>,
    data: web::Data<AppState>,
) -> Result<impl Responder> {
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
