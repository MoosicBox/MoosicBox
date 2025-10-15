# HyperChad HTML HTTP Renderer

Generic HTTP server integration for HyperChad HTML renderer with framework-agnostic design.

## Overview

The HyperChad HTML HTTP Renderer provides:

- **Framework Agnostic**: Works with any HTTP server implementation
- **Request Processing**: Process RouteRequest objects into HTTP responses
- **Response Generation**: Standards-compliant HTTP response generation
- **Static Assets**: Integrated static file serving
- **Action Handling**: Server-side action processing
- **Error Handling**: Comprehensive HTTP error handling

## Features

### HTTP Server Capabilities

- **Generic HTTP**: Works with any HTTP server framework
- **Request Processing**: Process RouteRequest objects into HTTP responses
- **Response Building**: Generate proper HTTP responses
- **Status Codes**: Full HTTP status code support
- **Headers**: Custom header management
- **Content Types**: Automatic content type detection

### HyperChad Integration

- **HTML Rendering**: Server-side HTML generation
- **Routing**: URL routing and path matching
- **Action Processing**: Handle HyperChad actions
- **Partial Updates**: Support for partial page updates
- **Asset Serving**: Static asset management

### Performance Features

- **Async Processing**: Fully asynchronous request handling

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_html_http = { path = "../hyperchad/renderer/html/http" }

# Default features: actions, assets, debug, json
# Or with specific features only
hyperchad_renderer_html_http = {
    path = "../hyperchad/renderer/html/http",
    default-features = false,
    features = ["actions"]
}
```

## Usage

**Note:** The code examples below are simplified for clarity. In production code, you'll need to add proper error handling, import all required types, and handle edge cases appropriately.

### Basic HTTP Application

```rust
use hyperchad_renderer_html_http::HttpApp;
use hyperchad_renderer_html::DefaultHtmlTagRenderer;
use hyperchad_router::{Router, RouteRequest, RequestInfo};
use hyperchad_template::container;
use http::{Request, Response};
use std::collections::BTreeMap;
use std::sync::Arc;
use bytes::Bytes;
use switchy::http::models::Method;

async fn handle_request(req: Request<Vec<u8>>) -> Result<Response<Vec<u8>>, Box<dyn std::error::Error>> {
    // Create router and tag renderer
    let router = Router::new();
    let tag_renderer = DefaultHtmlTagRenderer::default();

    // Create HTTP app
    let app = HttpApp::new(tag_renderer, router)
        .with_title("My HyperChad App")
        .with_description("A HyperChad HTTP application")
        .with_viewport("width=device-width, initial-scale=1");

    // Parse query parameters
    let query = req.uri().query()
        .map(|q| {
            qstring::QString::from(q)
                .into_iter()
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();

    // Convert HTTP request to RouteRequest
    let route_request = RouteRequest {
        path: req.uri().path().to_string(),
        query,
        method: Method::from(req.method().as_str()),
        headers: req.headers().iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect(),
        cookies: BTreeMap::new(),
        info: RequestInfo::default(),
        body: Some(Arc::new(Bytes::from(req.into_body()))),
    };

    // Process the request
    let response = app.process(&route_request).await?;

    Ok(response)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Example with hyper server
    use hyper::service::{make_service_fn, service_fn};
    use hyper::{Body, Server};
    use std::convert::Infallible;

    let make_svc = make_service_fn(|_conn| async {
        Ok::<_, Infallible>(service_fn(|req: Request<Body>| async move {
            // Convert hyper request to our format
            let (parts, body) = req.into_parts();
            let body_bytes = hyper::body::to_bytes(body).await.unwrap();
            let req = Request::from_parts(parts, body_bytes.to_vec());

            match handle_request(req).await {
                Ok(response) => {
                    let (parts, body) = response.into_parts();
                    Ok::<_, Infallible>(Response::from_parts(parts, Body::from(body)))
                }
                Err(_) => Ok(Response::builder()
                    .status(500)
                    .body(Body::from("Internal Server Error"))
                    .unwrap()),
            }
        }))
    });

    let addr = ([127, 0, 0, 1], 3000).into();
    let server = Server::bind(&addr).serve(make_svc);

    println!("Server running on http://{}", addr);
    server.await?;

    Ok(())
}
```

### Advanced Routing

**Note:** The Router uses a builder pattern with `with_route_result` for adding routes. Route handlers must return `Result<Option<Content>, BoxError>`.

```rust
use hyperchad_router::{Router, RouteRequest};
use hyperchad_renderer::Content;
use hyperchad_template::container;

fn create_router() -> Router {
    Router::new()
        .with_route_result("/", |_req: RouteRequest| async {
            Ok(Some(render_home()))
        })
        .with_route_result("/about", |_req: RouteRequest| async {
            Ok(Some(render_about()))
        })
        .with_route_result("/users/{id}", |req: RouteRequest| async {
            // Note: Path parameter extraction needs custom implementation
            let user_id = req.path.split('/').last().unwrap_or("unknown");
            Ok(Some(render_user(user_id)))
        })
        .with_route_result("/api/users", |req: RouteRequest| async {
            handle_api_users(&req).await
        })
}

fn render_home() -> Content {
    let view = container! {
        html {
            head {
                title { "Home - HyperChad HTTP" }
                meta name="viewport" content="width=device-width, initial-scale=1";
            }
            body {
                div class="container" {
                    h1 { "Welcome to HyperChad HTTP" }
                    p { "This is a framework-agnostic HTTP server." }

                    nav {
                        a href="/about" { "About" }
                        " | "
                        a href="/users/123" { "User Profile" }
                        " | "
                        a href="/api/users" { "API" }
                    }

                    div class="features" {
                        h2 { "Features" }
                        ul {
                            li { "Framework agnostic design" }
                            li { "Full HTTP standard support" }
                            li { "Static asset serving" }
                            li { "Action processing" }
                        }
                    }
                }
            }
        }
    };

    Content::from(view)
}

fn render_about() -> Content {
    let view = container! {
        html {
            head {
                title { "About - HyperChad HTTP" }
            }
            body {
                div class="container" {
                    h1 { "About HyperChad HTTP" }
                    p { "A framework-agnostic HTTP renderer for HyperChad." }
                    a href="/" { "← Back to Home" }
                }
            }
        }
    };

    Content::from(view)
}

fn render_user(user_id: &str) -> Content {
    let view = container! {
        html {
            head {
                title { format!("User {} - HyperChad HTTP", user_id) }
            }
            body {
                div class="container" {
                    h1 { format!("User Profile: {}", user_id) }

                    div class="user-info" {
                        p { format!("User ID: {}", user_id) }
                        p { "Name: John Doe" }
                        p { "Email: john@example.com" }
                    }

                    a href="/" { "← Back to Home" }
                }
            }
        }
    };

    Content::from(view)
}

async fn handle_api_users(req: &RouteRequest) -> Result<Option<Content>, Box<dyn std::error::Error>> {
    use hyperchad_renderer::Content;
    use switchy::http::models::Method;

    match req.method {
        Method::Get => {
            let users = serde_json::json!([
                {"id": 1, "name": "Alice", "email": "alice@example.com"},
                {"id": 2, "name": "Bob", "email": "bob@example.com"}
            ]);

            Ok(Some(Content::Json(users)))
        }
        Method::Post => {
            // Handle user creation
            let body = req.body.as_ref().ok_or("Missing body")?;
            let new_user: serde_json::Value = serde_json::from_slice(body)?;

            let response = serde_json::json!({
                "id": 123,
                "name": new_user["name"],
                "email": new_user["email"]
            });

            Ok(Some(Content::Json(response)))
        }
        _ => {
            Ok(Some(Content::Raw {
                data: b"Method Not Allowed".to_vec(),
                content_type: "text/plain".to_string(),
            }))
        }
    }
}
```

### Static Asset Serving

**Note:** Static asset serving requires the `assets` feature (enabled by default).

```rust
use hyperchad_renderer_html_http::HttpApp;
use hyperchad_renderer_html::DefaultHtmlTagRenderer;
use hyperchad_router::Router;
use hyperchad_renderer::assets::AssetPathTarget;
use std::path::PathBuf;

fn create_app_with_assets() -> HttpApp<DefaultHtmlTagRenderer> {
    let router = Router::new();
    let tag_renderer = DefaultHtmlTagRenderer::default();

    HttpApp::new(tag_renderer, router)
        .with_static_asset_route_handler(|req| {
            // Map URL paths to filesystem paths
            match req.path.as_str() {
                "/css/style.css" => Some(AssetPathTarget::File(PathBuf::from("assets/style.css"))),
                "/js/app.js" => Some(AssetPathTarget::File(PathBuf::from("assets/app.js"))),
                path if path.starts_with("/images/") => {
                    Some(AssetPathTarget::Directory(PathBuf::from("assets/images")))
                }
                path if path.starts_with("/uploads/") => {
                    Some(AssetPathTarget::Directory(PathBuf::from("uploads")))
                }
                _ => None,
            }
        })
}

fn render_page_with_assets() -> Content {
    let view = container! {
        html {
            head {
                title { "HyperChad with Assets" }
                link rel="stylesheet" href="/css/style.css";
                meta name="viewport" content="width=device-width, initial-scale=1";
            }
            body {
                div class="container" {
                    h1 { "HyperChad with Static Assets" }

                    img src="/images/logo.png" alt="Logo" class="logo";

                    p { "This page includes CSS, JavaScript, and image assets." }

                    div class="gallery" {
                        img src="/uploads/photo1.jpg" alt="Photo 1";
                        img src="/uploads/photo2.jpg" alt="Photo 2";
                    }
                }

                script src="/js/app.js" {}
            }
        }
    };

    Content::from(view)
}
```

### Action Handling

**Note:** Action handling requires the `actions` feature (enabled by default).

```rust
use hyperchad_renderer::transformer::actions::logic::Value;

fn create_app_with_actions() -> HttpApp<DefaultHtmlTagRenderer> {
    let (action_tx, action_rx) = flume::unbounded();

    // Handle actions in background
    tokio::spawn(async move {
        while let Ok((action_name, value)) = action_rx.recv_async().await {
            match action_name.as_str() {
                "submit_contact_form" => {
                    if let Some(Value::Object(data)) = value {
                        println!("Contact form submitted: {:?}", data);
                        // Send email, save to database, etc.
                    }
                }
                "user_login" => {
                    if let Some(Value::Object(credentials)) = value {
                        println!("Login attempt: {:?}", credentials);
                        // Authenticate user
                    }
                }
                _ => {
                    println!("Unknown action: {}", action_name);
                }
            }
        }
    });

    let router = Router::new();
    let tag_renderer = DefaultHtmlTagRenderer::default();

    HttpApp::new(tag_renderer, router)
        .with_action_tx(action_tx)
}

fn render_contact_form() -> Content {
    let view = container! {
        html {
            head {
                title { "Contact Us" }
            }
            body {
                div class="container" {
                    h1 { "Contact Us" }

                    form
                        method="post"
                        action="/$action"
                        onsubmit=request_action("submit_contact_form", form_data())
                    {
                        div class="form-group" {
                            label for="name" { "Name:" }
                            input
                                type="text"
                                id="name"
                                name="name"
                                required=true
                            {}
                        }

                        div class="form-group" {
                            label for="email" { "Email:" }
                            input
                                type="email"
                                id="email"
                                name="email"
                                required=true
                            {}
                        }

                        div class="form-group" {
                            label for="message" { "Message:" }
                            textarea
                                id="message"
                                name="message"
                                rows=5
                                required=true
                            {}
                        }

                        button type="submit" { "Send Message" }
                    }
                }
            }
        }
    };

    Content::from(view)
}
```

### Error Handling

```rust
use http::StatusCode;
use hyperchad_template::container;

async fn handle_request_with_errors(req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    match handle_request(req).await {
        Ok(response) => response,
        Err(e) => handle_error(e),
    }
}

fn handle_error(error: Box<dyn std::error::Error>) -> Response<Vec<u8>> {
    // Determine status code and message based on error type
    let (status, message) = if error.to_string().contains("not found") {
        (StatusCode::NOT_FOUND, format!("Page not found: {}", error))
    } else if error.to_string().contains("IO") {
        (StatusCode::INTERNAL_SERVER_ERROR, format!("IO error: {}", error))
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
    };

    let error_page = container! {
        html {
            head {
                title { format!("{} - Error", status.as_u16()) }
            }
            body {
                div class="error-page" {
                    h1 { format!("Error {}", status.as_u16()) }
                    p { message }
                    a href="/" { "Go Home" }
                }
            }
        }
    };

    Response::builder()
        .status(status)
        .header("Content-Type", "text/html")
        .body(error_page.to_string().into_bytes())
        .unwrap()
}
```

### Middleware Integration

```rust
use std::time::Instant;

async fn logging_middleware<F, Fut>(
    req: Request<Vec<u8>>,
    handler: F,
) -> Response<Vec<u8>>
where
    F: FnOnce(Request<Vec<u8>>) -> Fut,
    Fut: std::future::Future<Output = Response<Vec<u8>>>,
{
    let start = Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    println!("→ {} {}", method, path);

    let response = handler(req).await;

    let duration = start.elapsed();
    let status = response.status();

    println!("← {} {} {} {:?}", method, path, status.as_u16(), duration);

    response
}

async fn cors_middleware<F, Fut>(
    req: Request<Vec<u8>>,
    handler: F,
) -> Response<Vec<u8>>
where
    F: FnOnce(Request<Vec<u8>>) -> Fut,
    Fut: std::future::Future<Output = Response<Vec<u8>>>,
{
    let mut response = handler(req).await;

    let headers = response.headers_mut();
    headers.insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    headers.insert("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS".parse().unwrap());
    headers.insert("Access-Control-Allow-Headers", "Content-Type, Authorization".parse().unwrap());

    response
}

async fn handle_with_middleware(req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    cors_middleware(req, |req| {
        logging_middleware(req, |req| {
            handle_request(req)
        })
    }).await
}
```

## Feature Flags

Default features: `actions`, `assets`, `debug`, `json`

- **`actions`**: Enable server-side action processing (enables `_json`, `serde`, `hyperchad_renderer/logic`, `hyperchad_router/serde`)
- **`assets`**: Enable static asset serving (enables `mime_guess`, `switchy_async`, `switchy_fs`, `hyperchad_renderer/assets`)
- **`debug`**: Enable debug-specific functionality
- **`json`**: Enable JSON content type support (enables `_json`, `hyperchad_renderer/json`)
- **`_json`**: Internal feature that enables `serde_json` (do not enable directly, use `actions` or `json` instead)

## HTTP Standards Compliance

### Supported Methods

- **GET**: Retrieve resources
- **POST**: Create resources
- **PUT**: Update resources
- **DELETE**: Delete resources
- **PATCH**: Partial updates
- **HEAD**: Headers only
- **OPTIONS**: CORS preflight

### Status Codes

- **2xx Success**: 200 OK, 201 Created, 204 No Content
- **3xx Redirection**: 301 Moved Permanently, 302 Found, 304 Not Modified
- **4xx Client Error**: 400 Bad Request, 401 Unauthorized, 404 Not Found
- **5xx Server Error**: 500 Internal Server Error, 503 Service Unavailable

### Headers

- **Content-Type**: Automatic content type detection for assets and responses
- Custom headers can be added via middleware (see middleware example)

## Dependencies

Core dependencies:

- **http**: Standard HTTP types and utilities
- **hyperchad_color**: Color handling
- **hyperchad_renderer**: Renderer abstractions and traits (with `canvas` feature)
- **hyperchad_renderer_html**: Core HTML rendering functionality
- **hyperchad_router**: Routing and navigation
- **moosicbox_assert**: Assertion utilities
- **moosicbox_env_utils**: Environment utilities
- **thiserror**: Error handling
- **flume**: Multi-producer, multi-consumer channels
- **log**: Logging facade

Optional dependencies (enabled by features):

- **serde** & **serde_json**: JSON serialization (enabled by `actions` and `json` features)
- **mime_guess**: Content-type detection for static assets (enabled by `assets` feature)
- **switchy_async** & **switchy_fs**: Async file I/O for asset serving (enabled by `assets` feature)

## Integration

This renderer is designed for:

- **Custom HTTP Servers**: Build your own HTTP server
- **Framework Integration**: Integrate with existing frameworks
- **Microservices**: HTTP-based microservices
- **API Gateways**: Custom API gateway implementations
- **Edge Computing**: Edge server implementations

## Performance Considerations

- **Async**: Fully asynchronous processing with tokio/async runtime support
- **Memory**: Efficient memory usage patterns
- **File I/O**: Async file operations for asset serving
