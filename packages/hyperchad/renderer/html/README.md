# HyperChad HTML Renderer

Server-side HTML renderer for HyperChad with support for multiple web frameworks and deployment targets.

## Overview

The HyperChad HTML Renderer provides:

- **Server-side Rendering**: Generate static and dynamic HTML from HyperChad components
- **Framework Integration**: Support for Actix Web, Lambda, and generic HTTP servers
- **Responsive Design**: CSS media queries and responsive breakpoints
- **Static Assets**: Asset serving and management
- **HTML Tag Rendering**: Complete HTML element generation with styling
- **Partial Updates**: HTMX-compatible partial page updates
- **SEO Optimization**: Server-rendered HTML for search engine optimization

## Features

### HTML Generation
- **Complete HTML Output**: Full HTML documents with DOCTYPE, head, and body
- **CSS Styling**: Inline styles and CSS classes generation
- **Responsive CSS**: Media queries for responsive design
- **Element Attributes**: Data attributes, IDs, classes, and custom attributes
- **Semantic HTML**: Proper semantic HTML element generation

### Framework Support
- **Actix Web**: Full integration with Actix Web framework
- **AWS Lambda**: Serverless deployment support
- **Generic HTTP**: Works with any HTTP server implementation
- **Static Assets**: File serving and asset management

### Rendering Modes
- **Full Page Rendering**: Complete HTML documents
- **Partial Rendering**: HTMX-compatible partial updates
- **Component Rendering**: Individual component HTML generation
- **Template Rendering**: Reusable template rendering

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_html = { path = "../hyperchad/renderer/html" }

# With Actix Web support
hyperchad_renderer_html = {
    path = "../hyperchad/renderer/html",
    features = ["actix"]
}

# With Lambda support
hyperchad_renderer_html = {
    path = "../hyperchad/renderer/html",
    features = ["lambda"]
}

# With asset serving
hyperchad_renderer_html = {
    path = "../hyperchad/renderer/html",
    features = ["assets"]
}
```

## Usage

### Basic HTML Rendering

```rust
use hyperchad_renderer_html::{HtmlRenderer, DefaultHtmlTagRenderer};
use hyperchad_template::container;
use hyperchad_renderer::{View, Renderer};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create HTML tag renderer
    let tag_renderer = DefaultHtmlTagRenderer::default();

    // Create HTML renderer
    let mut renderer = HtmlRenderer::new(tag_renderer)
        .with_title(Some("My App".to_string()))
        .with_description(Some("A HyperChad application".to_string()));

    // Initialize renderer
    renderer.init(
        800.0,    // width
        600.0,    // height
        None,     // x position
        None,     // y position
        None,     // background color
        Some("My App"), // title
        Some("My HyperChad App"), // description
        Some("width=device-width, initial-scale=1"), // viewport
    ).await?;

    // Create HyperChad view
    let view = container! {
        div
            class="container"
            style="max-width: 800px; margin: 0 auto; padding: 20px;"
        {
            h1
                style="color: #333; text-align: center;"
            {
                "Welcome to HyperChad!"
            }

            p
                style="font-size: 16px; line-height: 1.6;"
            {
                "This is a server-rendered HyperChad application."
            }
        }
    };

    // Render to HTML
    renderer.render(View::from(view)).await?;

    Ok(())
}
```

### Actix Web Integration

```rust
use actix_web::{web, App, HttpServer, HttpResponse, Result};
use hyperchad_renderer_html::actix::{ActixApp, ActixResponseProcessor};
use hyperchad_renderer_html::DefaultHtmlTagRenderer;
use hyperchad_router::{Router, RouteRequest};
use hyperchad_template::container;
use std::collections::HashMap;

struct MyResponseProcessor;

#[async_trait::async_trait]
impl ActixResponseProcessor<RouteRequest> for MyResponseProcessor {
    fn prepare_request(
        &self,
        req: actix_web::HttpRequest,
        body: Option<std::sync::Arc<bytes::Bytes>>,
    ) -> Result<RouteRequest, actix_web::Error> {
        // Convert Actix request to RouteRequest
        let path = req.path().to_string();
        let query = req.query_string().to_string();
        let method = req.method().to_string();

        Ok(RouteRequest {
            path,
            query: if query.is_empty() { None } else { Some(query) },
            method,
            headers: HashMap::new(),
            body: body.map(|b| b.to_vec()),
        })
    }

    async fn to_response(&self, data: RouteRequest) -> Result<HttpResponse, actix_web::Error> {
        // Route to appropriate handler
        match data.path.as_str() {
            "/" => Ok(HttpResponse::Ok().content_type("text/html").body(
                render_home_page().await
            )),
            "/about" => Ok(HttpResponse::Ok().content_type("text/html").body(
                render_about_page().await
            )),
            _ => Ok(HttpResponse::NotFound().body("Page not found")),
        }
    }

    async fn to_body(
        &self,
        content: hyperchad_renderer::Content,
        _data: RouteRequest,
    ) -> Result<String, actix_web::Error> {
        Ok(content.to_string())
    }
}

async fn render_home_page() -> String {
    let view = container! {
        div class="page" {
            h1 { "Home Page" }
            p { "Welcome to our website!" }
            a href="/about" { "About Us" }
        }
    };

    // Render with tag renderer
    let tag_renderer = DefaultHtmlTagRenderer::default();
    tag_renderer.root_html(
        &HashMap::new(),
        &view,
        view.to_string(),
        Some("width=device-width, initial-scale=1"),
        None,
        Some("Home"),
        Some("Welcome to our website"),
    )
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let processor = MyResponseProcessor;
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
    req: actix_web::HttpRequest,
    body: web::Bytes,
    data: web::Data<ActixApp<RouteRequest, MyResponseProcessor>>,
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

### Lambda Integration

```rust
use hyperchad_renderer_html::lambda::{LambdaApp, LambdaResponseProcessor, Content};
use lambda_http::{Request, Error as LambdaError};
use hyperchad_template::container;

struct MyLambdaProcessor;

#[async_trait::async_trait]
impl LambdaResponseProcessor<String> for MyLambdaProcessor {
    fn prepare_request(
        &self,
        req: Request,
        _body: Option<std::sync::Arc<bytes::Bytes>>,
    ) -> Result<String, lambda_runtime::Error> {
        Ok(req.uri().path().to_string())
    }

    fn headers(&self, _content: &hyperchad_renderer::Content) -> Option<Vec<(String, String)>> {
        Some(vec![
            ("Content-Type".to_string(), "text/html".to_string()),
            ("Cache-Control".to_string(), "public, max-age=3600".to_string()),
        ])
    }

    async fn to_response(
        &self,
        path: String,
    ) -> Result<Option<(Content, Option<Vec<(String, String)>>)>, lambda_runtime::Error> {
        let html = match path.as_str() {
            "/" => render_home_page(),
            "/about" => render_about_page(),
            _ => return Ok(None), // 404
        };

        Ok(Some((Content::Html(html), None)))
    }

    async fn to_body(
        &self,
        content: hyperchad_renderer::Content,
        _data: String,
    ) -> Result<Content, lambda_runtime::Error> {
        Ok(Content::Html(content.to_string()))
    }
}

fn render_home_page() -> String {
    let view = container! {
        div class="container" {
            h1 { "Serverless Home" }
            p { "This page is rendered by AWS Lambda!" }
        }
    };

    format!("<!DOCTYPE html><html><head><title>Home</title></head><body>{}</body></html>",
            view.to_string())
}

#[tokio::main]
async fn main() -> Result<(), lambda_runtime::Error> {
    let processor = MyLambdaProcessor;
    let app = LambdaApp::new(processor);

    let runner = app.to_runner(hyperchad_renderer::Handle::current())?;
    runner.run().map_err(|e| lambda_runtime::Error::from(e.to_string()))?;

    Ok(())
}
```

### Responsive Design

```rust
use hyperchad_renderer_html::DefaultHtmlTagRenderer;
use hyperchad_transformer::{ResponsiveTrigger, Number};

let mut tag_renderer = DefaultHtmlTagRenderer::default()
    .with_responsive_trigger("mobile", ResponsiveTrigger::MaxWidth(Number::Real(768.0)))
    .with_responsive_trigger("tablet", ResponsiveTrigger::MaxWidth(Number::Real(1024.0)));

let responsive_view = container! {
    div
        class="responsive-container"
        width="100%"
        direction="row"
        responsive_target="mobile" => {
            direction: "column",
            padding: "10px"
        }
        responsive_target="tablet" => {
            padding: "20px"
        }
    {
        div
            width="50%"
            responsive_target="mobile" => {
                width: "100%"
            }
        {
            h2 { "Main Content" }
            p { "This content adapts to screen size." }
        }

        div
            width="50%"
            responsive_target="mobile" => {
                width: "100%"
            }
        {
            h3 { "Sidebar" }
            p { "This sidebar becomes full-width on mobile." }
        }
    }
};
```

### Static Asset Serving

```rust
use hyperchad_renderer::{assets::{StaticAssetRoute, AssetPathTarget}};
use std::path::PathBuf;

let renderer = HtmlRenderer::new(tag_renderer)
    .with_static_asset_routes(vec![
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
    ]);

// In your HTML template
let view = container! {
    html {
        head {
            link rel="stylesheet" href="/css/style.css" {}
            script src="/js/app.js" {}
        }
        body {
            img src="/images/logo.png" alt="Logo" {}
        }
    }
};
```

### Partial Updates (HTMX)

```rust
use hyperchad_renderer::PartialView;

// Handle HTMX partial update
let partial_update = PartialView {
    target: "content".to_string(),
    content: container! {
        div {
            h2 { "Updated Content" }
            p { "This content was loaded via HTMX." }
        }
    },
    swap: hyperchad_transformer_models::SwapTarget::InnerHtml,
};

renderer.render_partial(partial_update).await?;

// The HTML template with HTMX
let htmx_view = container! {
    div {
        button
            hx-get="/api/update"
            hx-target="#content"
            hx-swap="innerHTML"
        {
            "Load Content"
        }

        div id="content" {
            "Initial content"
        }
    }
};
```

## Feature Flags

- **`actix`**: Enable Actix Web integration
- **`lambda`**: Enable AWS Lambda integration
- **`assets`**: Enable static asset serving
- **`extend`**: Enable renderer extension system

## HTML Output Features

### CSS Generation
- **Inline Styles**: Component styles as inline CSS
- **CSS Classes**: Automatic CSS class generation
- **Media Queries**: Responsive breakpoint CSS
- **CSS Variables**: Support for CSS custom properties

### SEO Optimization
- **Semantic HTML**: Proper HTML5 semantic elements
- **Meta Tags**: Title, description, and viewport meta tags
- **Structured Data**: Support for structured data markup
- **Accessibility**: ARIA attributes and semantic structure

## Dependencies

- **Maud**: HTML template engine for safe HTML generation
- **HyperChad Core**: Template, transformer, and router systems
- **Actix Web**: Web framework integration (optional)
- **Lambda HTTP**: AWS Lambda integration (optional)
- **Flume**: Async channel communication

## Integration

This renderer is designed for:
- **Web Applications**: Server-side rendered web apps
- **Static Sites**: Static site generation
- **Serverless**: AWS Lambda and other serverless platforms
- **Microservices**: API-driven web services
- **SEO-critical Sites**: Applications requiring search engine optimization

## Performance Considerations

- **Server-side Rendering**: HTML generation happens on the server
- **Caching**: Generated HTML can be cached for performance
- **Streaming**: Supports streaming HTML responses
- **Minimal JavaScript**: Reduced client-side JavaScript requirements
