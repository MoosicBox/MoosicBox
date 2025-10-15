# HyperChad HTML Actix Renderer

Actix Web integration for HyperChad HTML renderer with full web server functionality.

## Overview

The HyperChad HTML Actix Renderer provides:

- **Actix Web Integration**: Full integration with the Actix Web framework
- **HTTP Server**: Complete HTTP server with routing and middleware support
- **CORS Support**: Cross-Origin Resource Sharing configuration
- **Static Assets**: File serving and asset management
- **Action Handling**: Server-side action processing
- **SSE Support**: Server-Sent Events for real-time updates
- **Compression**: Automatic response compression
- **Logging**: Request/response logging and monitoring

## Features

### Web Server Capabilities

- **HTTP**: Full HTTP protocol support
- **Routing**: Flexible URL routing and path matching
- **Middleware**: Request/response middleware pipeline
- **CORS**: Configurable cross-origin resource sharing
- **Compression**: Gzip, Deflate, Brotli, and Zstd compression for responses
- **Static Files**: Efficient static file serving

### HyperChad Integration

- **HTML Rendering**: Server-side HTML generation
- **Action Processing**: Handle HyperChad actions server-side
- **Partial Updates**: HTMX-compatible partial page updates
- **Event Streaming**: Server-Sent Events for real-time updates
- **Asset Management**: Integrated static asset serving

### Performance Features

- **Async Processing**: Fully asynchronous request handling
- **Streaming**: Streaming responses for large content and SSE

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_html_actix = { path = "../hyperchad/renderer/html/actix" }

# With additional features
hyperchad_renderer_html_actix = {
    path = "../hyperchad/renderer/html/actix",
    features = ["actions", "sse", "assets"]
}
```

## Usage

### Basic Web Server

```rust
use actix_web::{web, App, HttpServer, HttpRequest, HttpResponse, Result};
use hyperchad_renderer_html_actix::{ActixApp, ActixResponseProcessor};
use bytes::Bytes;

#[derive(Clone)]
struct MyRequest {
    path: String,
    method: String,
}

struct MyProcessor;

#[async_trait::async_trait]
impl ActixResponseProcessor<MyRequest> for MyProcessor {
    fn prepare_request(
        &self,
        req: HttpRequest,
        _body: Option<std::sync::Arc<Bytes>>,
    ) -> Result<MyRequest, actix_web::Error> {
        Ok(MyRequest {
            path: req.path().to_string(),
            method: req.method().to_string(),
        })
    }

    async fn to_response(&self, data: MyRequest) -> Result<HttpResponse, actix_web::Error> {
        match data.path.as_str() {
            "/" => Ok(HttpResponse::Ok()
                .content_type("text/html")
                .body("<html><body><h1>Welcome to HyperChad!</h1></body></html>")),
            "/about" => Ok(HttpResponse::Ok()
                .content_type("text/html")
                .body("<html><body><h1>About Us</h1></body></html>")),
            _ => Ok(HttpResponse::NotFound().body("Page not found")),
        }
    }

    async fn to_body(
        &self,
        content: hyperchad_renderer::Content,
        _data: MyRequest,
    ) -> Result<(bytes::Bytes, String), actix_web::Error> {
        let body = content.to_string();
        let content_type = "text/html".to_string();
        Ok((bytes::Bytes::from(body), content_type))
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let processor = MyProcessor;
    let (_tx, rx) = flume::unbounded();
    let app = ActixApp::new(processor, rx);

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(app.clone()))
            .default_service(web::route().to(handle_request))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}

async fn handle_request(
    req: HttpRequest,
    body: web::Bytes,
    data: web::Data<ActixApp<MyRequest, MyProcessor>>,
) -> Result<HttpResponse> {
    let body = if body.is_empty() {
        None
    } else {
        Some(std::sync::Arc::new(body))
    };

    let route_request = data.processor.prepare_request(req, body)?;
    data.processor.to_response(route_request).await
}
```

### Action Handling

```rust
use hyperchad_renderer_html_actix::ActixApp;
use hyperchad_renderer::transformer::actions::logic::Value;

// With actions feature enabled
let (action_tx, action_rx) = flume::unbounded();
let app = ActixApp::new(processor, renderer_rx)
    .with_action_tx(action_tx);

// Handle actions in background
tokio::spawn(async move {
    while let Ok((action_name, value)) = action_rx.recv_async().await {
        match action_name.as_str() {
            "submit_form" => {
                if let Some(Value::Object(data)) = value {
                    println!("Form submitted: {:?}", data);
                    // Process form data
                }
            }
            "user_login" => {
                if let Some(Value::Object(credentials)) = value {
                    // Handle user login
                    println!("Login attempt: {:?}", credentials);
                }
            }
            _ => {
                println!("Unknown action: {}", action_name);
            }
        }
    }
});
```

### Static Asset Serving

```rust
use hyperchad_renderer::assets::{StaticAssetRoute, AssetPathTarget};
use std::path::PathBuf;

let mut app = ActixApp::new(processor, rx);
app.static_asset_routes = vec![
    StaticAssetRoute {
        route: "/css/style.css".to_string(),
        target: AssetPathTarget::File(PathBuf::from("assets/style.css")),
    },
    StaticAssetRoute {
        route: "/js/app.js".to_string(),
        target: AssetPathTarget::File(PathBuf::from("assets/app.js")),
    },
    StaticAssetRoute {
        route: "/images/".to_string(),
        target: AssetPathTarget::Directory(PathBuf::from("assets/images")),
    },
];
```

### HTMX Integration

This renderer fully supports HTMX through its CORS configuration, which includes all HTMX headers in allowed and exposed headers. Here's an example of handling HTMX requests:

```rust
use actix_web::{HttpRequest, HttpResponse, Result};

async fn handle_htmx_request(req: HttpRequest) -> Result<HttpResponse> {
    if req.headers().contains_key("hx-request") {
        // Handle HTMX partial update
        let partial_content = r#"
            <div class="updated-content">
                <h2>Updated Content</h2>
                <p>This was loaded via HTMX.</p>
            </div>
        "#;

        Ok(HttpResponse::Ok()
            .content_type("text/html")
            .body(partial_content))
    } else {
        // Handle full page request
        let full_page = r#"
            <html>
                <head>
                    <title>HTMX Demo</title>
                    <script src="https://unpkg.com/htmx.org@1.9.10"></script>
                </head>
                <body>
                    <div class="container">
                        <h1>HTMX Demo</h1>
                        <button
                            hx-get="/api/update"
                            hx-target="#content"
                            hx-swap="innerHTML">
                            Load Content
                        </button>
                        <div id="content">
                            Click the button to load content
                        </div>
                    </div>
                </body>
            </html>
        "#;

        Ok(HttpResponse::Ok()
            .content_type("text/html")
            .body(full_page))
    }
}
```

### Server-Sent Events

The renderer includes built-in SSE support when the `sse` feature is enabled (default). The SSE endpoint is automatically registered at `/$sse` and streams `RendererEvent`s from the renderer event channel:

```rust
use hyperchad_renderer_html_actix::ActixApp;
use hyperchad_renderer::RendererEvent;

// Create the app with a renderer event channel
let (_event_tx, event_rx) = flume::unbounded();
let app = ActixApp::new(processor, event_rx);

// The SSE endpoint at /$sse is automatically available
// Send events through the channel:
// event_tx.send(RendererEvent::View(view)).unwrap();
// event_tx.send(RendererEvent::Partial(partial_view)).unwrap();

// Client can connect to: http://localhost:8080/$sse
// Events are streamed in SSE format with compression support
```

### Custom Middleware

```rust
use actix_web::{
    dev::{forward_ready, Service, ServiceRequest, ServiceResponse, Transform},
    Error,
};
use futures_util::future::LocalBoxFuture;

pub struct HyperChadMiddleware;

impl<S, B> Transform<S, ServiceRequest> for HyperChadMiddleware
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type InitError = ();
    type Transform = HyperChadMiddlewareService<S>;
    type Future = std::future::Ready<Result<Self::Transform, Self::InitError>>;

    fn new_transform(&self, service: S) -> Self::Future {
        std::future::ready(Ok(HyperChadMiddlewareService { service }))
    }
}

pub struct HyperChadMiddlewareService<S> {
    service: S,
}

impl<S, B> Service<ServiceRequest> for HyperChadMiddlewareService<S>
where
    S: Service<ServiceRequest, Response = ServiceResponse<B>, Error = Error>,
    S::Future: 'static,
    B: 'static,
{
    type Response = ServiceResponse<B>;
    type Error = Error;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    forward_ready!(service);

    fn call(&self, req: ServiceRequest) -> Self::Future {
        let fut = self.service.call(req);

        Box::pin(async move {
            let mut res = fut.await?;

            // Add HyperChad headers
            res.headers_mut().insert(
                actix_web::http::header::HeaderName::from_static("x-hyperchad-version"),
                actix_web::http::HeaderValue::from_static("1.0.0"),
            );

            Ok(res)
        })
    }
}

// Use the middleware
App::new()
    .wrap(HyperChadMiddleware)
    .app_data(web::Data::new(app.clone()))
    .default_service(web::route().to(handle_request))
```

### Error Handling

```rust
use actix_web::{
    HttpResponse, Result,
    dev::ServiceResponse,
    middleware::{ErrorHandlerResponse, ErrorHandlers},
    http,
};

fn handle_404<B>(res: ServiceResponse<B>) -> Result<ErrorHandlerResponse<B>> {
    let error_page = r#"
        <html>
            <head>
                <title>Page Not Found</title>
            </head>
            <body>
                <div class="error-page">
                    <h1>404 - Page Not Found</h1>
                    <p>The page you're looking for doesn't exist.</p>
                    <a href="/">Go Home</a>
                </div>
            </body>
        </html>
    "#;

    let new_response = res.into_response(
        HttpResponse::NotFound()
            .content_type("text/html")
            .body(error_page)
    );

    Ok(ErrorHandlerResponse::Response(new_response.map_into_left_body()))
}

// Configure error handling
App::new()
    .wrap(ErrorHandlers::new()
        .handler(http::StatusCode::NOT_FOUND, handle_404))
    .app_data(web::Data::new(app.clone()))
    .default_service(web::route().to(handle_request))
```

## Feature Flags

- **`actions`**: Enable server-side action processing (default)
- **`sse`**: Enable Server-Sent Events support (default)
- **`assets`**: Enable static asset serving (default)
- **`debug`**: Enable debug logging (default)

## Configuration

### CORS Configuration

The renderer includes comprehensive CORS configuration:

- **Allowed Origins**: Any origin (configurable)
- **Allowed Methods**: GET, POST, OPTIONS, DELETE, PUT, PATCH
- **Allowed Headers**: Authorization, Accept, Content-Type, HTMX headers, moosicbox-profile
- **Exposed Headers**: HTMX headers for client-side updates
- **Credentials**: Supports credentials for authenticated requests

### Compression

Automatic response compression is enabled by default via Actix Web middleware:

- **Gzip**: Standard gzip compression
- **Deflate**: Deflate compression support
- **Brotli**: Brotli compression support
- **Zstd**: Zstandard compression support

Note: SSE streams support Gzip, Deflate, and Zlib encoding.

## Dependencies

Key dependencies include:

- **actix-web**: Web framework and HTTP server
- **actix-cors**: CORS middleware for Actix Web
- **actix-files**: Static file serving (when `assets` feature is enabled)
- **hyperchad_renderer**: Core HyperChad rendering functionality
- **flume**: Async channel communication for events and actions
- **bytes**: Efficient byte handling
- **flate2**: Compression support for SSE streams
- **async-trait**: Async trait support for `ActixResponseProcessor`
- **moosicbox_middleware**: API logging middleware
- **serde/serde_json**: JSON serialization (when `actions` or `sse` features are enabled)

## Integration

This renderer is designed for:

- **Web Applications**: Full-featured web applications
- **API Servers**: REST and GraphQL API servers
- **Microservices**: Service-oriented architectures
- **Real-time Applications**: Applications with live updates
- **Content Management**: CMS and content-driven sites

## Performance Considerations

- **Async Processing**: All operations are fully asynchronous
- **Streaming**: Server-Sent Events and large response streaming support
- **Compression**: Automatic response compression reduces bandwidth
