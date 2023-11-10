#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

mod api;
mod ws;

use actix_cors::Cors;
use actix_web::{http, middleware, web, App};
use lambda_runtime::Error;
use moosicbox_core::app::{AppState, Db};
use std::{
    env,
    sync::{Arc, Mutex, OnceLock},
    time::Duration,
};
#[cfg(feature = "server")]
use tokio::{task::spawn, try_join};

#[actix_web::main]
async fn main() -> Result<(), Error> {
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

    #[cfg(feature = "server")]
    let (chat_server, server_tx) = ws::server::ChatServer::new(Arc::new(Mutex::new(db.clone())));

    #[cfg(feature = "server")]
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

        #[allow(unused_mut)]
        let mut app = App::new()
            .wrap(cors)
            .wrap(middleware::Compress::default())
            .app_data(web::Data::new(app_data))
            .service(api::track_endpoint);

        #[cfg(feature = "server")]
        {
            app = app
                .app_data(web::Data::new(server_tx.clone()))
                .service(ws::api::websocket);
        }

        app
    };

    #[cfg(all(feature = "server", feature = "serverless"))]
    {
        if lambda_web::is_running_on_lambda() {
            lambda_web::run_actix_on_lambda(app).await?;
        } else {
            let http_server = actix_web::HttpServer::new(app)
                .bind(("0.0.0.0", service_port))?
                .run();
            try_join!(http_server, async move { chat_server.await.unwrap() })?;
        }
    }
    #[cfg(all(not(feature = "server"), feature = "serverless"))]
    {
        lambda_web::run_actix_on_lambda(app).await?;
    }
    #[cfg(all(not(feature = "serverless"), feature = "server"))]
    {
        let http_server = actix_web::HttpServer::new(app)
            .bind(("0.0.0.0", service_port))?
            .run();
        try_join!(http_server, async move { chat_server.await.unwrap() })?;
    }

    Ok(())
}
