use actix_cors::Cors;
use actix_web::{
    http, middleware,
    web::{self},
    Result,
};
use lambda_runtime::Error;
use lambda_web::actix_web::{self, App, HttpServer};
use lambda_web::{is_running_on_lambda, run_actix_on_lambda};
use moosicbox_core::app::AppState;
use moosicbox_player::api;

#[actix_web::main]
async fn main() -> Result<(), Error> {
    let service_port = 8000;

    let factory = move || {
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
            .app_data(web::Data::new(AppState {
                service_port,
                db: None,
            }))
            .service(api::play_track_endpoint)
            .service(api::play_tracks_endpoint)
            .service(api::pause_playback_endpoint)
            .service(api::resume_playback_endpoint)
            .service(api::update_playback_endpoint)
            .service(api::next_track_endpoint)
            .service(api::previous_track_endpoint)
            .service(api::stop_track_endpoint)
            .service(api::seek_track_endpoint)
            .service(api::player_status_endpoint)
    };

    if is_running_on_lambda() {
        run_actix_on_lambda(factory).await?;
    } else {
        HttpServer::new(factory)
            .bind(format!("0.0.0.0:{service_port}"))?
            .run()
            .await?;
    }
    Ok(())
}
