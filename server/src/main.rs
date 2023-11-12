#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

mod api;
mod scan;
mod ws;

use actix_cors::Cors;
use actix_web::{http, middleware, web, App, HttpServer};
use lazy_static::lazy_static;
use log::debug;
use moosicbox_core::app::{AppState, Db};
use moosicbox_tunnel::{tunnel::tunnel_request, ws::sender::TunnelMessage};
use serde_json::Value;
use std::{
    env,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::{
    runtime::{self, Runtime},
    task::spawn,
    try_join,
};
use ws::server::ChatServer;

lazy_static! {
    static ref RT: Runtime = runtime::Builder::new_multi_thread()
        .enable_all()
        .max_blocking_threads(4)
        .build()
        .unwrap();
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let args: Vec<String> = env::args().collect();

    let service_port = if args.len() > 1 {
        args[1].parse::<u16>().unwrap()
    } else {
        8000
    };

    static DB: OnceLock<Db> = OnceLock::new();
    let db = DB.get_or_init(|| {
        let library = ::rusqlite::Connection::open("library.db").unwrap();
        library
            .busy_timeout(Duration::from_millis(10))
            .expect("Failed to set busy timeout");
        Db {
            library: Arc::new(Mutex::new(library)),
        }
    });

    if let Ok(host) = env::var("WS_HOST") {
        use moosicbox_tunnel::ws::{init_host, sender::start};
        init_host(host).expect("Failed to initialize websocket host");
        let (ready, rx) = start();
        ready.recv().unwrap();

        RT.spawn(async move {
            while let Ok(m) = rx.recv() {
                match m {
                    TunnelMessage::Text(m) => {
                        debug!("Received text TunnelMessage");
                        let value: Value = serde_json::from_str(&m).unwrap();

                        match value.get("type").map(|t| t.as_str()) {
                            Some(Some("CONNECTION_ID")) => {
                                debug!("Received connection id: {value:?}");
                            }
                            Some(Some("TUNNEL_REQUEST")) => {
                                tunnel_request(
                                    db,
                                    serde_json::from_value(value.get("id").unwrap().clone())
                                        .unwrap(),
                                    "track".into(),
                                    value.get("payload").unwrap().clone(),
                                )
                                .await
                                .unwrap();
                            }
                            _ => {}
                        }
                    }
                    TunnelMessage::Binary(_) => todo!(),
                    TunnelMessage::Ping(_) => {}
                    TunnelMessage::Pong(_) => todo!(),
                    TunnelMessage::Close => todo!(),
                    TunnelMessage::Frame(_) => todo!(),
                }
            }
        });
    }

    let (chat_server, server_tx) = ChatServer::new(Arc::new(Mutex::new(db.clone())));

    let chat_server = spawn(chat_server.run());

    let app = move || {
        let app_data = AppState {
            service_port,
            db: Some(db.clone()),
        };

        let cors = Cors::default()
            .allow_any_origin()
            .allowed_methods(vec!["GET", "POST"])
            .allowed_headers(vec![http::header::AUTHORIZATION, http::header::ACCEPT])
            .allowed_header(http::header::CONTENT_TYPE)
            .supports_credentials()
            .max_age(3600);

        App::new()
            .wrap(cors)
            .wrap(middleware::Compress::default())
            .app_data(web::Data::new(app_data))
            .app_data(web::Data::new(server_tx.clone()))
            .service(api::websocket)
            .service(api::scan_endpoint)
            .service(moosicbox_menu::api::get_artists_endpoint)
            .service(moosicbox_menu::api::get_artist_endpoint)
            .service(moosicbox_menu::api::get_album_endpoint)
            .service(moosicbox_menu::api::get_albums_endpoint)
            .service(moosicbox_menu::api::get_album_tracks_endpoint)
            .service(moosicbox_menu::api::get_artist_albums_endpoint)
            .service(moosicbox_files::api::track_endpoint)
            .service(moosicbox_files::api::artist_cover_endpoint)
            .service(moosicbox_files::api::album_artwork_endpoint)
            .service(moosicbox_player::api::play_track_endpoint)
            .service(moosicbox_player::api::play_tracks_endpoint)
            .service(moosicbox_player::api::play_album_endpoint)
            .service(moosicbox_player::api::pause_playback_endpoint)
            .service(moosicbox_player::api::resume_playback_endpoint)
            .service(moosicbox_player::api::update_playback_endpoint)
            .service(moosicbox_player::api::next_track_endpoint)
            .service(moosicbox_player::api::previous_track_endpoint)
            .service(moosicbox_player::api::stop_track_endpoint)
            .service(moosicbox_player::api::seek_track_endpoint)
            .service(moosicbox_player::api::player_status_endpoint)
    };

    let http_server = HttpServer::new(app).bind(("0.0.0.0", service_port))?.run();

    try_join!(http_server, async move { chat_server.await.unwrap() })?;

    Ok(())
}
