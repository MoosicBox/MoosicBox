# HyperChad HTML Web Server Renderer

This package provides a web server backend for HyperChad HTML rendering using the MoosicBox web server framework.

## Features

- **Web Server Backend**: Uses `moosicbox_web_server` for HTTP handling
- **Actix Support**: Optional actix-web backend support via the `actix` feature
- **Simulator Support**: Easy testing and simulation via the `simulator` feature
- **Assets**: Static asset serving via the `assets` feature

## Usage

This package provides low-level web server backend infrastructure. Users typically interact with it via higher-level packages like `hyperchad_renderer_html` which provides the `router_to_web_server()` helper function.

To use this package directly, implement the `WebServerResponseProcessor` trait:

```rust
use hyperchad_renderer_html_web_server::*;
use hyperchad_renderer::{Handle, ToRenderRunner};
use flume::unbounded;
use async_trait::async_trait;
use std::sync::Arc;

// Define your request data type
#[derive(Clone)]
struct MyRequestType {
    path: String,
}

// Implement WebServerResponseProcessor for your response type
#[derive(Clone)]
struct MyProcessor;

#[async_trait]
impl WebServerResponseProcessor<MyRequestType> for MyProcessor {
    fn prepare_request(
        &self,
        req: HttpRequest,
        body: Option<Arc<bytes::Bytes>>,
    ) -> Result<MyRequestType, WebServerError> {
        // Implementation here
    }

    async fn to_response(&self, data: MyRequestType) -> Result<HttpResponse, WebServerError> {
        // Implementation here
    }

    async fn to_body(&self, content: hyperchad_renderer::Content, data: MyRequestType) -> Result<(bytes::Bytes, String), WebServerError> {
        // Implementation here
    }
}

// Create a web server app with your processor
let (_tx, rx) = unbounded();
let processor = MyProcessor;
let app = WebServerApp::new(processor, rx);

// Convert to a runner and start
let handle = Handle::current();
let mut runner = app.to_runner(handle)?;
runner.run()?;
```

## Cargo Features

- `actix` - Enable actix-web backend support in `moosicbox_web_server`
- `simulator` - Enable simulation capabilities for testing in `moosicbox_web_server`
- `assets` - Enable static asset serving (enabled by default)
- `debug` - Enable debug features (enabled by default)
