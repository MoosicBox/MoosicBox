# HyperChad HTML Lambda Renderer

AWS Lambda integration for HyperChad HTML renderer with serverless deployment support.

## Overview

The HyperChad HTML Lambda Renderer provides:

- **AWS Lambda Integration**: Full integration with AWS Lambda runtime
- **Serverless Deployment**: Deploy HyperChad applications as serverless functions
- **HTTP Event Handling**: Process API Gateway and ALB HTTP events
- **Response Compression**: Automatic gzip compression for responses
- **Cold Start Optimization**: Optimized for minimal cold start times
- **Error Handling**: Comprehensive error handling and logging
- **JSON Support**: Support for JSON API responses alongside HTML

## Features

### Lambda Runtime Support
- **Lambda HTTP**: Integration with lambda_http crate
- **Event Processing**: Handle API Gateway and ALB events
- **Response Generation**: Generate proper Lambda HTTP responses
- **Error Handling**: Lambda-compatible error responses

### Response Features
- **HTML Responses**: Server-rendered HTML pages
- **JSON Responses**: API responses in JSON format
- **Compression**: Automatic gzip compression
- **Headers**: Custom header support
- **Status Codes**: Full HTTP status code support

### Performance Optimizations
- **Cold Start**: Minimal initialization overhead
- **Memory Efficiency**: Low memory footprint
- **Response Streaming**: Efficient response generation
- **Compression**: Reduced response sizes

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_html_lambda = { path = "../hyperchad/renderer/html/lambda" }

# With JSON support
hyperchad_renderer_html_lambda = {
    path = "../hyperchad/renderer/html/lambda",
    features = ["json"]
}

# With asset serving
hyperchad_renderer_html_lambda = {
    path = "../hyperchad/renderer/html/lambda",
    features = ["assets"]
}
```

## Usage

### Basic Lambda Function

```rust
use hyperchad_renderer_html_lambda::{LambdaApp, LambdaResponseProcessor, Content};
use lambda_http::{Request, Error as LambdaError};
use lambda_runtime::{self, Error};
use hyperchad_template::container;
use bytes::Bytes;
use std::sync::Arc;

struct MyLambdaProcessor;

#[async_trait::async_trait]
impl LambdaResponseProcessor<String> for MyLambdaProcessor {
    fn prepare_request(
        &self,
        req: Request,
        _body: Option<Arc<Bytes>>,
    ) -> Result<String, lambda_runtime::Error> {
        Ok(req.uri().path().to_string())
    }

    fn headers(&self, _content: &hyperchad_renderer::Content) -> Option<Vec<(String, String)>> {
        Some(vec![
            ("Content-Type".to_string(), "text/html; charset=utf-8".to_string()),
            ("Cache-Control".to_string(), "public, max-age=3600".to_string()),
            ("X-Powered-By".to_string(), "HyperChad".to_string()),
        ])
    }

    async fn to_response(
        &self,
        path: String,
    ) -> Result<Option<(Content, Option<Vec<(String, String)>>)>, lambda_runtime::Error> {
        match path.as_str() {
            "/" => Ok(Some((Content::Html(render_home_page()), None))),
            "/about" => Ok(Some((Content::Html(render_about_page()), None))),
            "/api/health" => Ok(Some((
                Content::Json(serde_json::json!({"status": "ok", "timestamp": chrono::Utc::now()})),
                Some(vec![("Content-Type".to_string(), "application/json".to_string())])
            ))),
            _ => Ok(None), // 404
        }
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
        html {
            head {
                title { "HyperChad Lambda" }
                meta name="viewport" content="width=device-width, initial-scale=1";
            }
            body {
                div class="container" {
                    h1 { "Welcome to HyperChad on Lambda!" }
                    p { "This page is rendered by AWS Lambda." }

                    nav {
                        a href="/about" { "About" }
                        " | "
                        a href="/api/health" { "Health Check" }
                    }

                    div class="info" {
                        h2 { "Serverless Benefits" }
                        ul {
                            li { "Automatic scaling" }
                            li { "Pay per request" }
                            li { "Zero server management" }
                            li { "Global deployment" }
                        }
                    }
                }
            }
        }
    };

    view.to_string()
}

fn render_about_page() -> String {
    let view = container! {
        html {
            head {
                title { "About - HyperChad Lambda" }
                meta name="viewport" content="width=device-width, initial-scale=1";
            }
            body {
                div class="container" {
                    h1 { "About HyperChad Lambda" }
                    p {
                        "HyperChad Lambda renderer enables serverless deployment "
                        "of HyperChad applications on AWS Lambda."
                    }

                    a href="/" { "← Back to Home" }
                }
            }
        }
    };

    view.to_string()
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .init();

    let processor = MyLambdaProcessor;
    let app = LambdaApp::new(processor);

    let runner = app.to_runner(hyperchad_renderer::Handle::current())?;
    runner.run().map_err(|e| Error::from(e.to_string()))?;

    Ok(())
}
```

### API Gateway Integration

```rust
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct CreateUserRequest {
    name: String,
    email: String,
}

#[derive(Serialize)]
struct CreateUserResponse {
    id: u64,
    name: String,
    email: String,
    created_at: String,
}

struct ApiProcessor;

#[async_trait::async_trait]
impl LambdaResponseProcessor<(String, String, Option<String>)> for ApiProcessor {
    fn prepare_request(
        &self,
        req: Request,
        body: Option<Arc<Bytes>>,
    ) -> Result<(String, String, Option<String>), lambda_runtime::Error> {
        let path = req.uri().path().to_string();
        let method = req.method().to_string();
        let body_str = body.and_then(|b| String::from_utf8(b.to_vec()).ok());

        Ok((method, path, body_str))
    }

    fn headers(&self, content: &hyperchad_renderer::Content) -> Option<Vec<(String, String)>> {
        match content {
            hyperchad_renderer::Content::Html(_) => Some(vec![
                ("Content-Type".to_string(), "text/html".to_string()),
            ]),
            hyperchad_renderer::Content::Json(_) => Some(vec![
                ("Content-Type".to_string(), "application/json".to_string()),
            ]),
        }
    }

    async fn to_response(
        &self,
        (method, path, body): (String, String, Option<String>),
    ) -> Result<Option<(Content, Option<Vec<(String, String)>>)>, lambda_runtime::Error> {
        match (method.as_str(), path.as_str()) {
            ("GET", "/") => Ok(Some((Content::Html(render_api_docs()), None))),

            ("GET", "/api/users") => {
                let users = vec![
                    serde_json::json!({"id": 1, "name": "Alice", "email": "alice@example.com"}),
                    serde_json::json!({"id": 2, "name": "Bob", "email": "bob@example.com"}),
                ];
                Ok(Some((Content::Json(serde_json::json!({"users": users})), None)))
            }

            ("POST", "/api/users") => {
                if let Some(body) = body {
                    let request: CreateUserRequest = serde_json::from_str(&body)
                        .map_err(|e| lambda_runtime::Error::from(e.to_string()))?;

                    let response = CreateUserResponse {
                        id: 123,
                        name: request.name,
                        email: request.email,
                        created_at: chrono::Utc::now().to_rfc3339(),
                    };

                    Ok(Some((Content::Json(serde_json::to_value(response).unwrap()), None)))
                } else {
                    Ok(Some((
                        Content::Json(serde_json::json!({"error": "Missing request body"})),
                        Some(vec![("Status".to_string(), "400".to_string())])
                    )))
                }
            }

            _ => Ok(None), // 404
        }
    }

    async fn to_body(
        &self,
        content: hyperchad_renderer::Content,
        _data: (String, String, Option<String>),
    ) -> Result<Content, lambda_runtime::Error> {
        match content {
            hyperchad_renderer::Content::Html(html) => Ok(Content::Html(html)),
            hyperchad_renderer::Content::Json(json) => Ok(Content::Json(json)),
        }
    }
}

fn render_api_docs() -> String {
    let view = container! {
        html {
            head {
                title { "API Documentation" }
                style {
                    "
                    body { font-family: Arial, sans-serif; margin: 40px; }
                    .endpoint { background: #f5f5f5; padding: 15px; margin: 10px 0; border-radius: 5px; }
                    .method { color: white; padding: 3px 8px; border-radius: 3px; font-weight: bold; }
                    .get { background: #4CAF50; }
                    .post { background: #2196F3; }
                    "
                }
            }
            body {
                h1 { "API Documentation" }

                div class="endpoint" {
                    span class="method get" { "GET" }
                    " "
                    code { "/api/users" }
                    p { "Get all users" }
                }

                div class="endpoint" {
                    span class="method post" { "POST" }
                    " "
                    code { "/api/users" }
                    p { "Create a new user" }
                    pre {
                        r#"{"name": "John Doe", "email": "john@example.com"}"#
                    }
                }
            }
        }
    };

    view.to_string()
}
```

### Static Asset Serving

```rust
use hyperchad_renderer::{assets::{StaticAssetRoute, AssetPathTarget}};
use std::path::PathBuf;

struct AssetProcessor {
    static_routes: Vec<StaticAssetRoute>,
}

impl AssetProcessor {
    fn new() -> Self {
        Self {
            static_routes: vec![
                StaticAssetRoute {
                    route: "/css/style.css".to_string(),
                    target: AssetPathTarget::FileContents(include_bytes!("../assets/style.css").to_vec()),
                },
                StaticAssetRoute {
                    route: "/js/app.js".to_string(),
                    target: AssetPathTarget::FileContents(include_bytes!("../assets/app.js").to_vec()),
                },
            ],
        }
    }
}

#[async_trait::async_trait]
impl LambdaResponseProcessor<String> for AssetProcessor {
    fn prepare_request(
        &self,
        req: Request,
        _body: Option<Arc<Bytes>>,
    ) -> Result<String, lambda_runtime::Error> {
        Ok(req.uri().path().to_string())
    }

    fn headers(&self, _content: &hyperchad_renderer::Content) -> Option<Vec<(String, String)>> {
        None
    }

    async fn to_response(
        &self,
        path: String,
    ) -> Result<Option<(Content, Option<Vec<(String, String)>>)>, lambda_runtime::Error> {
        // Check static assets first
        for route in &self.static_routes {
            if route.route == path {
                match &route.target {
                    AssetPathTarget::FileContents(contents) => {
                        let content_type = match path.as_str() {
                            p if p.ends_with(".css") => "text/css",
                            p if p.ends_with(".js") => "application/javascript",
                            p if p.ends_with(".png") => "image/png",
                            p if p.ends_with(".jpg") | p.ends_with(".jpeg") => "image/jpeg",
                            _ => "application/octet-stream",
                        };

                        return Ok(Some((
                            Content::Html(String::from_utf8_lossy(contents).to_string()),
                            Some(vec![
                                ("Content-Type".to_string(), content_type.to_string()),
                                ("Cache-Control".to_string(), "public, max-age=86400".to_string()),
                            ])
                        )));
                    }
                    _ => {}
                }
            }
        }

        // Regular page routing
        match path.as_str() {
            "/" => Ok(Some((Content::Html(render_home_with_assets()), None))),
            _ => Ok(None),
        }
    }

    async fn to_body(
        &self,
        content: hyperchad_renderer::Content,
        _data: String,
    ) -> Result<Content, lambda_runtime::Error> {
        Ok(Content::Html(content.to_string()))
    }
}

fn render_home_with_assets() -> String {
    let view = container! {
        html {
            head {
                title { "HyperChad with Assets" }
                link rel="stylesheet" href="/css/style.css";
            }
            body {
                div class="container" {
                    h1 { "HyperChad with Static Assets" }
                    p { "This page includes CSS and JavaScript assets." }
                }
                script src="/js/app.js" {}
            }
        }
    };

    view.to_string()
}
```

### Environment-based Configuration

```rust
use std::env;

struct ConfigurableProcessor {
    environment: String,
    debug: bool,
}

impl ConfigurableProcessor {
    fn new() -> Self {
        Self {
            environment: env::var("ENVIRONMENT").unwrap_or_else(|_| "production".to_string()),
            debug: env::var("DEBUG").is_ok(),
        }
    }
}

#[async_trait::async_trait]
impl LambdaResponseProcessor<String> for ConfigurableProcessor {
    fn prepare_request(
        &self,
        req: Request,
        _body: Option<Arc<Bytes>>,
    ) -> Result<String, lambda_runtime::Error> {
        Ok(req.uri().path().to_string())
    }

    fn headers(&self, _content: &hyperchad_renderer::Content) -> Option<Vec<(String, String)>> {
        let mut headers = vec![
            ("Content-Type".to_string(), "text/html".to_string()),
            ("X-Environment".to_string(), self.environment.clone()),
        ];

        if self.debug {
            headers.push(("X-Debug".to_string(), "true".to_string()));
        }

        Some(headers)
    }

    async fn to_response(
        &self,
        path: String,
    ) -> Result<Option<(Content, Option<Vec<(String, String)>>)>, lambda_runtime::Error> {
        match path.as_str() {
            "/" => Ok(Some((Content::Html(self.render_home()), None))),
            "/debug" if self.debug => Ok(Some((Content::Html(self.render_debug()), None))),
            _ => Ok(None),
        }
    }

    async fn to_body(
        &self,
        content: hyperchad_renderer::Content,
        _data: String,
    ) -> Result<Content, lambda_runtime::Error> {
        Ok(Content::Html(content.to_string()))
    }
}

impl ConfigurableProcessor {
    fn render_home(&self) -> String {
        let view = container! {
            html {
                head {
                    title { format!("HyperChad - {}", self.environment) }
                }
                body {
                    div class="container" {
                        h1 { "HyperChad Lambda" }
                        p { format!("Environment: {}", self.environment) }

                        @if self.debug {
                            div class="debug-info" {
                                h2 { "Debug Mode Enabled" }
                                a href="/debug" { "View Debug Info" }
                            }
                        }
                    }
                }
            }
        };

        view.to_string()
    }

    fn render_debug(&self) -> String {
        let view = container! {
            html {
                head {
                    title { "Debug Information" }
                }
                body {
                    div class="container" {
                        h1 { "Debug Information" }

                        h2 { "Environment Variables" }
                        ul {
                            @for (key, value) in env::vars() {
                                li { format!("{}: {}", key, value) }
                            }
                        }

                        a href="/" { "← Back to Home" }
                    }
                }
            }
        };

        view.to_string()
    }
}
```

## Deployment

### SAM Template

```yaml
AWSTemplateFormatVersion: '2010-09-09'
Transform: AWS::Serverless-2016-10-31

Resources:
  HyperChadFunction:
    Type: AWS::Serverless::Function
    Properties:
      CodeUri: target/lambda/hyperchad-lambda/
      Handler: provided
      Runtime: provided.al2
      Architectures:
        - x86_64
      Events:
        Api:
          Type: Api
          Properties:
            Path: /{proxy+}
            Method: ANY
        Root:
          Type: Api
          Properties:
            Path: /
            Method: ANY
      Environment:
        Variables:
          RUST_LOG: info
          ENVIRONMENT: production
```

### Build Script

```bash
#!/bin/bash
# Build for Lambda
cargo build --release --target x86_64-unknown-linux-musl

# Create deployment package
mkdir -p target/lambda/hyperchad-lambda
cp target/x86_64-unknown-linux-musl/release/hyperchad-lambda target/lambda/hyperchad-lambda/bootstrap

# Deploy with SAM
sam build
sam deploy --guided
```

## Feature Flags

- **`json`**: Enable JSON response support
- **`assets`**: Enable static asset serving

## Performance Optimizations

### Cold Start Reduction
- **Minimal Dependencies**: Only essential dependencies included
- **Lazy Initialization**: Defer expensive initialization
- **Binary Size**: Optimized binary size for faster cold starts

### Memory Efficiency
- **Streaming**: Stream large responses
- **Compression**: Automatic gzip compression
- **Memory Pool**: Efficient memory allocation

## Dependencies

- **Lambda HTTP**: AWS Lambda HTTP event handling
- **Lambda Runtime**: AWS Lambda runtime integration
- **HyperChad HTML Renderer**: Core HTML rendering functionality
- **Flate2**: Gzip compression support
- **Serde JSON**: JSON serialization (optional)

## Integration

This renderer is designed for:
- **Serverless Web Apps**: Full serverless web applications
- **API Services**: REST and GraphQL APIs
- **Static Sites**: Server-rendered static sites
- **Microservices**: Event-driven microservices
- **Edge Computing**: CloudFront Lambda@Edge functions

## Limitations

- **Cold Starts**: Initial request latency for cold starts
- **Execution Time**: 15-minute maximum execution time
- **Memory**: Limited memory allocation options
- **State**: No persistent state between invocations
