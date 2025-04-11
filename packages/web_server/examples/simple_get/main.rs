use moosicbox_web_server::HttpResponse;

#[tokio::main]
async fn main() {
    let cors = moosicbox_web_server::cors::Cors::default()
        .allow_any_origin()
        .allow_any_method()
        .allow_any_header()
        .expose_any_header();

    let server = moosicbox_web_server::WebServerBuilder::new()
        .with_cors(cors)
        .build_actix();

    server.start().await;
}

moosicbox_web_server::route!(GET, example, "/example", |req| {
    Box::pin(async move {
        Ok(HttpResponse {
            body: "hello, world!".into(),
        })
    })
});
