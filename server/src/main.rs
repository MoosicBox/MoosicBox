mod api;
mod scan;

use actix_cors::Cors;
use actix_web::{http, middleware, web, App, HttpServer};
use moosicbox_core::{
    app::{AppState, Db},
    sqlite::db::init_db,
};
use std::{env, time::Duration};

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let service_port = 8000;

    let library_db = ::sqlite::open("library.db").unwrap();
    let db = Db {
        library: library_db,
    };
    let _ = init_db(&db).await;

    let app = move || {
        let args: Vec<String> = env::args().collect();

        let proxy_url = if args.len() > 1 {
            args[1].clone()
        } else {
            String::from("http://127.0.0.1:9000")
        };

        let proxy_client = awc::Client::builder()
            .timeout(Duration::from_secs(120))
            .finish();

        let image_client = awc::Client::builder()
            .timeout(Duration::from_secs(120))
            .finish();

        let library_db = ::sqlite::open("library.db").unwrap();
        let db = Db {
            library: library_db,
        };

        let app_data = AppState {
            service_port,
            proxy_url,
            proxy_client,
            image_client,
            db,
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
            .service(api::connect_endpoint)
            .service(api::status_endpoint)
            .service(api::playlist_status_endpoint)
            .service(api::ping_endpoint)
            .service(api::pause_player_endpoint)
            .service(api::play_player_endpoint)
            .service(api::play_album_endpoint)
            .service(api::player_start_track_endpoint)
            .service(api::player_next_track_endpoint)
            .service(api::player_previous_track_endpoint)
            .service(api::get_album_endpoint)
            .service(moosicbox_menu::api::get_albums_endpoint)
            .service(moosicbox_menu::api::get_album_tracks_endpoint)
            .service(moosicbox_files::api::track_endpoint)
            .service(moosicbox_files::api::album_artwork_endpoint)
            .service(api::get_players_endpoint)
            .service(api::start_player_endpoint)
            .service(api::stop_player_endpoint)
            .service(api::restart_player_endpoint)
            .service(api::image_proxy_endpoint)
            .service(api::proxy_get_endpoint)
            .service(api::proxy_post_endpoint)
            .service(api::scan_endpoint)
    };

    HttpServer::new(app)
        .bind(("0.0.0.0", service_port))?
        .run()
        .await
}
