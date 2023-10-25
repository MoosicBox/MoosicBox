#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

mod api;
mod scan;
mod ws;

use actix_cors::Cors;
use actix_web::{http, middleware, web, App, HttpServer};
use moosicbox_core::app::{AppState, Db};
use std::{
    env,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
use tokio::{task::spawn, try_join};
use ws::server::ChatServer;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
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
    };

    let http_server = HttpServer::new(app).bind(("0.0.0.0", service_port))?.run();

    try_join!(http_server, async move { chat_server.await.unwrap() })?;

    Ok(())
}
