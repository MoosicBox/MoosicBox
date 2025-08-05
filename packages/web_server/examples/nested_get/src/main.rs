use moosicbox_web_server::{HttpResponse, Scope};

#[tokio::main]
async fn main() {
    env_logger::init();

    let cors = moosicbox_web_server::cors::Cors::default()
        .allow_any_origin()
        .allow_any_method()
        .allow_any_header()
        .expose_any_header();

    let server =
        moosicbox_web_server::WebServerBuilder::new()
            .with_cors(cors)
            .with_scope(Scope::new("/nested").get("/example", |req| {
                let path = req.path().to_string();
                let query = req.query_string().to_string();
                Box::pin(async move {
                    Ok(HttpResponse::ok()
                        .with_body(format!("hello, world! path={path} query={query}")))
                })
            }))
            .build();

    server.start().await;
}
