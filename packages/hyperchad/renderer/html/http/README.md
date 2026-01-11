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

### Basic HTTP Application

**Note:** This example shows integration with external HTTP server frameworks. The `qstring`, `hyper`, and `bytes` crates are not dependencies of this package and must be added separately.

```rust
use hyperchad_renderer_html_http::HttpApp;
use hyperchad_renderer_html::DefaultHtmlTagRenderer;
use hyperchad_router::{Router, RouteRequest};
use http::{Request, Response, StatusCode};
use bytes::Bytes;
use qstring::QString;

async fn handle_request(req: Request<Vec<u8>>) -> Result<Response<Vec<u8>>, Box<dyn std::error::Error>> {
    // Create router and tag renderer
    let router = Router::new();
    let tag_renderer = DefaultHtmlTagRenderer::default();

    // Create HTTP app
    let app = HttpApp::new(tag_renderer, router)
        .with_title("My HyperChad App".to_string())
        .with_description("A HyperChad HTTP application".to_string())
        .with_viewport("width=device-width, initial-scale=1".to_string());

    // Convert HTTP request to RouteRequest
    let query_str = req.uri().query().unwrap_or("");
    let query = QString::from(query_str).into_iter().collect();
    let route_request = RouteRequest {
        path: req.uri().path().to_string(),
        method: switchy::http::models::Method::from(req.method().as_str()),
        query,
        headers: req.headers().iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect(),
        cookies: std::collections::BTreeMap::new(),
        info: hyperchad_router::RequestInfo::default(),
        body: Some(std::sync::Arc::new(Bytes::from(req.into_body()))),
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

**Note:** This example uses the `hyperchad_template` crate for HTML templating, which must be added as a separate dependency.

```rust
use hyperchad_router::{RouteRequest, Router, RoutePath};
use hyperchad_template::{ContainerVecExt, container};

fn create_router() -> Router {
    let router = Router::new();

    // Add routes
    router.add_route_result("/", |_req| async { Ok::<_, Box<dyn std::error::Error>>(render_home()) });
    router.add_route_result("/about", |_req| async { Ok::<_, Box<dyn std::error::Error>>(render_about()) });
    router.add_route_result("/users/{id}", |req| async move {
        let user_id = req.path.strip_prefix("/users/").unwrap_or("");
        Ok::<_, Box<dyn std::error::Error>>(render_user(user_id))
    });
    router.add_route_result("/api/users", |req| async move { handle_api_users(&req).await });

    router
}

fn render_home() -> String {
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

    view.to_string()
}

fn render_user(user_id: &str) -> String {
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

    view.to_string()
}

async fn handle_api_users(req: &RouteRequest) -> Result<Content, Box<dyn std::error::Error>> {
    use hyperchad_renderer::Content;

    match req.method.as_ref() {
        "GET" => {
            let users = serde_json::json!([
                {"id": 1, "name": "Alice", "email": "alice@example.com"},
                {"id": 2, "name": "Bob", "email": "bob@example.com"}
            ]);

            Ok(Content::Json(users))
        }
        "POST" => {
            // Handle user creation
            let body = req.body.as_ref().unwrap();
            let new_user: serde_json::Value = serde_json::from_slice(body)?;

            let response = serde_json::json!({
                "id": 123,
                "name": new_user["name"],
                "email": new_user["email"]
            });

            Ok(Content::Json(response))
        }
        _ => {
            Ok(Content::Raw {
                data: bytes::Bytes::from_static(b"Method Not Allowed"),
                content_type: "text/plain".to_string(),
            })
        }
    }
}
```

### Static Asset Serving

**Note:** Static asset serving requires the `assets` feature (enabled by default). The `hyperchad_template` crate used for HTML templating in this example must be added as a separate dependency.

```rust
use hyperchad_renderer_html_http::HttpApp;
use hyperchad_renderer_html::DefaultHtmlTagRenderer;
use hyperchad_router::Router;
use hyperchad_renderer::assets::AssetPathTarget;
use hyperchad_template::{ContainerVecExt, container};
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

fn render_page_with_assets() -> String {
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

    view.to_string()
}
```

### Action Handling

**Note:** Action handling requires the `actions` feature (enabled by default).

```rust
use hyperchad_renderer_html_http::HttpApp;
use hyperchad_renderer_html::DefaultHtmlTagRenderer;
use hyperchad_router::Router;
use hyperchad_renderer::transformer::actions::logic::Value;

fn create_app_with_actions() -> HttpApp<DefaultHtmlTagRenderer> {
    let (action_tx, action_rx) = flume::unbounded();

    // Handle actions in background
    tokio::spawn(async move {
        while let Ok((action_name, value)) = action_rx.recv_async().await {
            match action_name.as_str() {
                "submit_contact_form" => {
                    println!("Contact form submitted: {:?}", value);
                    // Send email, save to database, etc.
                }
                "user_login" => {
                    println!("Login attempt: {:?}", value);
                    // Authenticate user
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
```

### Error Handling

**Note:** The `hyperchad_template` crate used for HTML templating in this example must be added as a separate dependency.

```rust
use http::{Request, Response, StatusCode};
use hyperchad_template::{ContainerVecExt, container};

async fn handle_request_with_errors(req: Request<Vec<u8>>) -> Response<Vec<u8>> {
    match process_request(req).await {
        Ok(response) => response,
        Err(e) => handle_error(e),
    }
}

fn handle_error(error: Box<dyn std::error::Error>) -> Response<Vec<u8>> {
    let (status, message) = match error.downcast_ref::<hyperchad_renderer_html_http::Error>() {
        Some(hyperchad_renderer_html_http::Error::Navigate(nav_err)) => {
            (StatusCode::NOT_FOUND, format!("Page not found: {}", nav_err))
        }
        Some(hyperchad_renderer_html_http::Error::IO(io_err)) => {
            (StatusCode::INTERNAL_SERVER_ERROR, format!("IO error: {}", io_err))
        }
        _ => {
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
        }
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

## Feature Flags

Default features: `actions`, `assets`, `debug`, `json`

- **`actions`**: Enable server-side action processing (depends on `_json` and `serde`)
- **`assets`**: Enable static asset serving (depends on `mime_guess`, `switchy_async`, `switchy_fs`)
- **`debug`**: Enable debug-specific functionality
- **`json`**: Enable JSON content type support (depends on `_json`)
- **`_json`**: Internal feature for JSON dependencies (do not enable directly)

## HTTP Standards Compliance

### Status Codes

The `HttpApp::process` method returns the following HTTP status codes:

- **200 OK**: Successful HTML, JSON, or raw content responses
- **204 No Content**: When a route returns no content or action processing completes
- **400 Bad Request**: Invalid action request body
- **404 Not Found**: Requested static asset file does not exist
- **500 Internal Server Error**: Asset file access failure (configurable via `AssetNotFoundBehavior`)

### Headers

- **Content-Type**: Automatic content type detection for assets and responses

## Dependencies

Core dependencies:

- **http**: Standard HTTP types and utilities
- **hyperchad_renderer_html**: Core HTML rendering functionality
- **hyperchad_router**: Routing and navigation
- **hyperchad_renderer**: Renderer abstractions and traits
- **thiserror**: Error handling
- **flume**: Multi-producer, multi-consumer channels for action handling
- **serde_json**: JSON serialization for internal use

Optional dependencies (enabled by features):

- **serde**: JSON deserialization for actions (enabled by `actions` feature)
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
