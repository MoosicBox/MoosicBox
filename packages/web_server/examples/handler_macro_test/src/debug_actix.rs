use moosicbox_web_server::Error;
use moosicbox_web_server::handler::IntoHandler;
use moosicbox_web_server::{HttpResponse, RequestInfo};

// Simple test function with no parameters - this should implement IntoHandler<()>
async fn simple_handler() -> Result<HttpResponse, Error> {
    Ok(HttpResponse::ok().with_body("Simple handler"))
}

// Test function with one extractor - this should implement IntoHandler<(RequestInfo,)>
async fn info_handler(info: RequestInfo) -> Result<HttpResponse, Error> {
    let response = format!("Path: {}", info.path);
    Ok(HttpResponse::ok().with_body(response))
}

fn main() {
    println!("Testing IntoHandler trait implementation...");

    // Test simple handler (no parameters)
    let _simple_handler = simple_handler.into_handler();
    println!("✅ Simple handler (no params) implements IntoHandler!");

    // Test handler with one parameter
    let _info_handler = info_handler.into_handler();
    println!("✅ Info handler (1 param) implements IntoHandler!");

    println!("🎉 All IntoHandler trait tests passed!");
}
