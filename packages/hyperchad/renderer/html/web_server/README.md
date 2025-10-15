# HyperChad HTML Web Server Renderer

This package provides a web server backend for HyperChad HTML rendering using the MoosicBox web server framework.

## Features

- **Web Server Backend**: Uses `moosicbox_web_server` for HTTP handling
- **Actix Support**: Optional actix-web backend support via the `actix` feature
- **Simulator Support**: Easy testing and simulation via the `simulator` feature
- **Assets**: Static asset serving via the `assets` feature

## Usage

This package provides the `WebServerApp` type that integrates with the HyperChad rendering system. To use it, you need to implement the `WebServerResponseProcessor` trait to handle HTTP requests and responses.

```rust
use hyperchad_renderer_html_web_server::*;
use hyperchad_renderer::{Handle, ToRenderRunner, RendererEvent};
use flume::unbounded;

// Implement WebServerResponseProcessor for your type
struct MyProcessor;

impl WebServerResponseProcessor<MyRequestData> for MyProcessor {
    fn prepare_request(
        &self,
        req: HttpRequest,
        body: Option<Arc<Bytes>>,
    ) -> Result<MyRequestData, WebServerError> {
        // Prepare request data
        todo!()
    }

    async fn to_response(&self, data: MyRequestData) -> Result<HttpResponse, WebServerError> {
        // Convert data to HTTP response
        todo!()
    }

    async fn to_body(
        &self,
        content: Content,
        data: MyRequestData,
    ) -> Result<(Bytes, String), WebServerError> {
        // Convert content to response body
        todo!()
    }
}

// Create a web server app with your processor
let (tx, rx) = unbounded::<RendererEvent>();
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
