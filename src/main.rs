mod api;
mod app;
mod player;

use actix_web::{http, web, App, HttpServer};
use std::env;

use actix_cors::Cors;

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
            .app_data(web::Data::new(app::AppState {
                proxy_url: proxy_url.clone(),
                proxy_client: awc::Client::default(),
            }))
            .service(api::connect_endpoint)
            .service(api::status_endpoint)
            .service(api::ping_endpoint)
            .service(api::pause_player_endpoint)
            .service(api::play_player_endpoint)
            .service(api::get_players_endpoint)
            .service(api::start_player_endpoint)
            .service(api::stop_player_endpoint)
            .service(api::restart_player_endpoint)
            .service(api::proxy_endpoint)
    })
    .bind(("127.0.0.1", 8000))?
    .run()
    .await
}
