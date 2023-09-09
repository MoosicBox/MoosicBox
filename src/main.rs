use core::panic;
use std::{env, time::Duration};

use actix_cors::Cors;
use actix_web::{
    get, http, post,
    web::{self, Json},
    App, HttpServer, Responder,
};
use serde::{de::Error, Deserialize, Serialize};
use serde_json::Value;

struct AppState {
    proxy_url: String,
    proxy_client: awc::Client,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ConnectResponse {
    client_id: String,
    channel: String,
    id: String,
    subscription: String,
    successful: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PingResponseStatus {
    timestamp: String,
    client_id: Option<String>,
    channel: String,
    id: String,
    successful: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PingResponseDataWrapper {
    data: PingResponseData,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct PingResponseData {
    players_loop: Vec<Player>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
enum PingResponse {
    ResponseStatus(PingResponseStatus),
    ResponseDataWrapper(PingResponseDataWrapper),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct HandshakeResponse {
    client_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetPlayersResponseStatus {
    client_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetPlayersResponseDataWrapper {
    data: GetPlayersResponseData,
}

#[derive(Debug, Serialize, Deserialize)]
struct GetPlayersResponseData {
    players_loop: Vec<Player>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Player {
    #[serde(rename = "playerid")]
    player_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GetPlayersResponse {
    ResponseStatus(GetPlayersResponseStatus),
    ResponseDataWrapper(GetPlayersResponseDataWrapper),
}

async fn player_pause(player_id: String, data: web::Data<AppState>) -> serde_json::Result<Value> {
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

async fn player_play(player_id: String, data: web::Data<AppState>) -> serde_json::Result<Value> {
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

async fn set_player_status(
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

async fn handshake(data: web::Data<AppState>) -> serde_json::Result<HandshakeResponse> {
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

async fn connect(
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

async fn ping(
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

async fn get_players(
    client_id: String,
    data: web::Data<AppState>,
) -> serde_json::Result<Vec<Player>> {
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
        Err(_) => panic!("Request failure"),
    };

    let players = match &get_players_response[1] {
        GetPlayersResponse::ResponseDataWrapper(x) => x.data.players_loop.clone(),
        _ => panic!("Invalid get players response data"),
    };

    Ok(players)
}

#[post("/connect")]
async fn connect_endpoint(data: web::Data<AppState>) -> serde_json::Result<impl Responder> {
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
struct PingQuery {
    client_id: String,
}

#[post("/ping")]
async fn ping_endpoint(
    query: web::Query<PingQuery>,
    data: web::Data<AppState>,
) -> serde_json::Result<impl Responder> {
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
struct GetPlayersQuery {
    client_id: String,
}

#[get("/playback/players")]
async fn get_players_endpoint(
    query: web::Query<GetPlayersQuery>,
    data: web::Data<AppState>,
) -> serde_json::Result<impl Responder> {
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
struct PausePlayerQuery {
    player_id: String,
}

#[post("/playback/pause")]
async fn pause_player_endpoint(
    query: web::Query<PausePlayerQuery>,
    data: web::Data<AppState>,
) -> serde_json::Result<impl Responder> {
    match player_pause(query.player_id.clone(), data).await {
        Ok(json) => json,
        Err(_) => panic!("Failed to pause player"),
    };

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PlayPlayerQuery {
    player_id: String,
}

#[post("/playback/play")]
async fn play_player_endpoint(
    query: web::Query<PlayPlayerQuery>,
    data: web::Data<AppState>,
) -> serde_json::Result<impl Responder> {
    match player_play(query.player_id.clone(), data).await {
        Ok(json) => json,
        Err(_) => panic!("Failed to play player"),
    };

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StartPlayerQuery {
    player_id: String,
}

#[post("/playback/start-player")]
async fn start_player_endpoint(
    query: web::Query<StartPlayerQuery>,
    data: web::Data<AppState>,
) -> serde_json::Result<impl Responder> {
    match set_player_status(query.player_id.clone(), String::from("1"), data).await {
        Ok(json) => json,
        Err(_) => panic!("Failed to start player"),
    };

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct StopPlayerQuery {
    player_id: String,
}

#[post("/playback/stop-player")]
async fn stop_player_endpoint(
    query: web::Query<StopPlayerQuery>,
    data: web::Data<AppState>,
) -> serde_json::Result<impl Responder> {
    match set_player_status(query.player_id.clone(), String::from("0"), data).await {
        Ok(json) => json,
        Err(_) => panic!("Failed to stop player"),
    };

    Ok(Json(serde_json::json!({"success": true})))
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RestartPlayerQuery {
    player_id: String,
}

#[post("/playback/restart-player")]
async fn restart_player_endpoint(
    query: web::Query<RestartPlayerQuery>,
    data: web::Data<AppState>,
) -> serde_json::Result<impl Responder> {
    set_player_status(query.player_id.clone(), String::from("0"), data.clone())
        .await
        .unwrap();
    set_player_status(query.player_id.clone(), String::from("1"), data.clone())
        .await
        .unwrap();

    Ok(Json(serde_json::json!({"success": true})))
}

#[post("/proxy/{proxy:.*}")]
async fn proxy_endpoint(
    body: String,
    path: web::Path<(String,)>,
    data: web::Data<AppState>,
) -> serde_json::Result<impl Responder> {
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

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        let args: Vec<String> = env::args().collect();

        let proxy_url = if args.len() > 1 {
            args[1].clone()
        } else {
            String::from("http://127.0.0.1:9000")
        };

        let cors = Cors::default()
            .allowed_origin("http://127.0.0.1:3000")
            .allowed_origin("http://localhost:3000")
            .allowed_methods(vec!["GET", "POST"])
            .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
            .allowed_header(http::header::CONTENT_TYPE)
            .supports_credentials()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(web::Data::new(AppState {
                proxy_url: proxy_url.clone(),
                proxy_client: awc::Client::default(),
            }))
            .service(connect_endpoint)
            .service(ping_endpoint)
            .service(pause_player_endpoint)
            .service(play_player_endpoint)
            .service(get_players_endpoint)
            .service(start_player_endpoint)
            .service(stop_player_endpoint)
            .service(restart_player_endpoint)
            .service(proxy_endpoint)
    })
    .bind(("127.0.0.1", 8000))?
    .run()
    .await
}
