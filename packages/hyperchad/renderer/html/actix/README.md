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
- **Compression**: Gzip, Deflate, and Zlib compression for responses
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

# For the examples below, you'll also need:
hyperchad_renderer_html = { path = "../hyperchad/renderer/html" }
hyperchad_router = { path = "../hyperchad/router" }
hyperchad_template = { path = "../hyperchad/template" }

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
use hyperchad_renderer_html::DefaultHtmlTagRenderer;
use hyperchad_router::{Router, RouteRequest};
use hyperchad_template::container;
use std::collections::HashMap;
use bytes::Bytes;

struct MyProcessor;

#[async_trait::async_trait]
impl ActixResponseProcessor<RouteRequest> for MyProcessor {
    fn prepare_request(
        &self,
        req: HttpRequest,
        body: Option<std::sync::Arc<Bytes>>,
    ) -> Result<RouteRequest, actix_web::Error> {
        Ok(RouteRequest {
            path: req.path().to_string(),
            query: req.query_string().to_string().into(),
            method: req.method().to_string(),
            headers: req.headers().iter()
                .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
                .collect(),
            body: body.map(|b| b.to_vec()),
        })
    }

    async fn to_response(&self, data: RouteRequest) -> Result<HttpResponse, actix_web::Error> {
        match data.path.as_str() {
            "/" => Ok(HttpResponse::Ok()
                .content_type("text/html")
                .body(render_home_page())),
            "/about" => Ok(HttpResponse::Ok()
                .content_type("text/html")
                .body(render_about_page())),
            _ => Ok(HttpResponse::NotFound().body("Page not found")),
        }
    }

    async fn to_body(
        &self,
        content: hyperchad_renderer::Content,
        _data: RouteRequest,
    ) -> Result<(bytes::Bytes, String), actix_web::Error> {
        let body = content.to_string();
        let content_type = "text/html".to_string();
        Ok((bytes::Bytes::from(body), content_type))
    }
}

fn render_home_page() -> String {
    let view = container! {
        div class="page" {
            h1 { "Welcome to HyperChad!" }
            p { "This is a server-rendered page using Actix Web." }
            a href="/about" { "About Us" }
        }
    };

    let tag_renderer = DefaultHtmlTagRenderer::default();
    tag_renderer.root_html(
        &HashMap::new(),
        &view,
        view.to_string(),
        Some("width=device-width, initial-scale=1"),
        None,
        Some("Home"),
        Some("Welcome to our HyperChad application"),
    )
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init();

    let processor = MyProcessor;
    let (tx, rx) = flume::unbounded();
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
    data: web::Data<ActixApp<RouteRequest, MyProcessor>>,
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
let (_renderer_tx, renderer_rx) = flume::unbounded();
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

```rust
use hyperchad_template::container;

async fn handle_htmx_request(req: HttpRequest) -> Result<HttpResponse> {
    if req.headers().contains_key("hx-request") {
        // Handle HTMX partial update
        let partial_content = container! {
            div class="updated-content" {
                h2 { "Updated Content" }
                p { "This was loaded via HTMX." }
                span { format!("Loaded at: {}", chrono::Utc::now()) }
            }
        };

        Ok(HttpResponse::Ok()
            .content_type("text/html")
            .body(partial_content.to_string()))
    } else {
        // Handle full page request
        let full_page = container! {
            html {
                head {
                    title { "HTMX Demo" }
                    script src="https://unpkg.com/htmx.org@1.9.10" {}
                }
                body {
                    div class="container" {
                        h1 { "HTMX Demo" }

                        button
                            hx-get="/api/update"
                            hx-target="#content"
                            hx-swap="innerHTML"
                        {
                            "Load Content"
                        }

                        div id="content" {
                            "Click the button to load content"
                        }
                    }
                }
            }
        };

        Ok(HttpResponse::Ok()
            .content_type("text/html")
            .body(full_page.to_string()))
    }
}
```

### Server-Sent Events

Server-Sent Events are automatically available at the `/$sse` endpoint when the `sse` feature is enabled. The SSE endpoint streams `RendererEvent` updates from the renderer event channel to connected clients.

Events include:

- `view`: View updates (including partial updates with fragments)
- `canvas_update`: Canvas updates for HTML5 canvas elements
- `event`: Custom events with name and value

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
use actix_web::{middleware::ErrorHandlerResponse, Result};

fn handle_404<B>(res: ServiceResponse<B>) -> Result<ErrorHandlerResponse<B>> {
    let error_page = container! {
        html {
            head {
                title { "Page Not Found" }
            }
            body {
                div class="error-page" {
                    h1 { "404 - Page Not Found" }
                    p { "The page you're looking for doesn't exist." }
                    a href="/" { "Go Home" }
                }
            }
        }
    };

    let new_response = res.into_response(
        HttpResponse::NotFound()
            .content_type("text/html")
            .body(error_page.to_string())
    );

    Ok(ErrorHandlerResponse::Response(new_response.map_into_left_body()))
}

// Configure error handling
App::new()
    .wrap(middleware::ErrorHandlers::new()
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
- **Allowed Headers**: Authorization, Accept, Content-Type, HyperChad headers
- **Exposed Headers**: HTMX headers for client-side updates
- **Credentials**: Supports credentials for authenticated requests

### Compression

Automatic response compression is enabled by default via Actix Web middleware:

- **Gzip**: Standard gzip compression
- **Deflate**: Deflate compression support
- **Zlib**: Zlib compression support

Note: SSE streams are currently sent without compression (Identity encoding).

## Dependencies

- **Actix Web**: Web framework and HTTP server
- **HyperChad HTML Renderer**: Core HTML rendering functionality
- **HyperChad Core**: Template, transformer, and action systems
- **Flume**: Async channel communication
- **Bytes**: Efficient byte handling

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
