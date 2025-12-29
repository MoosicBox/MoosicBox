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
- **JSON Responses**: API responses in JSON format (with `json` feature)
- **Raw Responses**: Binary content with custom content-type
- **Compression**: Automatic gzip compression for all responses
- **Headers**: Custom header support

### Performance Optimizations

- **Cold Start**: Minimal initialization overhead
- **Memory Efficiency**: Low memory footprint
- **Compression**: Automatic gzip compression for reduced response sizes
- **Streaming Response API**: Uses Lambda's streaming response API

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_html_lambda = { path = "../hyperchad/renderer/html/lambda" }
hyperchad_template = { path = "../hyperchad/template" }  # For template DSL support

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

> **Note**: The examples below use the HyperChad template DSL (`container!` macro from `hyperchad_template`). While the Lambda renderer itself only handles HTML strings, most users will be using HyperChad templates to generate that HTML. You can also use raw HTML strings or any other templating approach that returns a `String`.

### Basic Lambda Function

```rust
use hyperchad_renderer_html_lambda::{LambdaApp, LambdaResponseProcessor, Content};
use hyperchad_renderer::ToRenderRunner;
use hyperchad_template::container;
use lambda_http::Request;
use lambda_runtime::Error;
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
            #[cfg(feature = "json")]
            "/api/health" => Ok(Some((
                Content::Json(serde_json::json!({"status": "ok"})),
                None
            ))),
            _ => Ok(None), // 404
        }
    }

    async fn to_body(
        &self,
        _content: hyperchad_renderer::Content,
        _data: String,
    ) -> Result<Content, lambda_runtime::Error> {
        unimplemented!("to_body is not used by the Lambda runtime")
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
            }
            body {
                div class="container" {
                    h1 { "About HyperChad Lambda" }
                    p { "HyperChad Lambda brings the power of HyperChad to serverless environments." }
                    a href="/" { "Back to Home" }
                }
            }
        }
    };

    view.to_string()
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let processor = MyLambdaProcessor;
    let app = LambdaApp::new(processor);

    let runner = app.to_runner(hyperchad_renderer::Handle::current())?;
    runner.run().map_err(|e| Error::from(e.to_string()))?;

    Ok(())
}
```

### API Gateway Integration

This example requires the `json` feature to be enabled.

```rust
#[cfg(feature = "json")]
mod api_example {
    use hyperchad_renderer_html_lambda::{LambdaApp, LambdaResponseProcessor, Content};
    use hyperchad_template::container;
    use lambda_http::Request;
    use lambda_runtime::Error;
    use bytes::Bytes;
    use std::sync::Arc;
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

        fn headers(&self, _content: &hyperchad_renderer::Content) -> Option<Vec<(String, String)>> {
            None // Content-Type is set automatically based on Content variant
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
                        };

                        Ok(Some((Content::Json(serde_json::to_value(response).unwrap()), None)))
                    } else {
                        Ok(Some((
                            Content::Json(serde_json::json!({"error": "Missing request body"})),
                            None
                        )))
                    }
                }

                _ => Ok(None), // 404
            }
        }

        async fn to_body(
            &self,
            _content: hyperchad_renderer::Content,
            _data: (String, String, Option<String>),
        ) -> Result<Content, lambda_runtime::Error> {
            unimplemented!("to_body is not used by the Lambda runtime")
        }
    }

    fn render_api_docs() -> String {
        let view = container! {
            html {
                head {
                    title { "API Documentation" }
                }
                body {
                    div class="container" {
                        h1 { "API Documentation" }

                        section {
                            h2 { "GET /api/users" }
                            p { "Returns a list of all users." }
                        }

                        section {
                            h2 { "POST /api/users" }
                            p { "Creates a new user." }
                            pre {
                                "{\n  \"name\": \"string\",\n  \"email\": \"string\"\n}"
                            }
                        }
                    }
                }
            }
        };

        view.to_string()
    }
}
```

### Static Asset Serving

This example requires the `assets` feature to be enabled.

```rust
#[cfg(feature = "assets")]
mod asset_example {
    use hyperchad_renderer_html_lambda::{LambdaApp, LambdaResponseProcessor, Content};
    use hyperchad_renderer::assets::{StaticAssetRoute, AssetPathTarget};
    use hyperchad_template::container;
    use lambda_http::Request;
    use lambda_runtime::Error;
    use bytes::Bytes;
    use std::sync::Arc;

    struct AssetProcessor;

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
            match path.as_str() {
                "/css/style.css" => {
                    let css_content = include_bytes!("../assets/style.css");
                    Ok(Some((
                        Content::Raw {
                            data: Bytes::from_static(css_content),
                            content_type: "text/css".to_string(),
                        },
                        Some(vec![("Cache-Control".to_string(), "public, max-age=86400".to_string())])
                    )))
                }
                "/" => Ok(Some((Content::Html(render_home_with_assets()), None))),
                _ => Ok(None),
            }
        }

        async fn to_body(
            &self,
            _content: hyperchad_renderer::Content,
            _data: String,
        ) -> Result<Content, lambda_runtime::Error> {
            unimplemented!("to_body is not used by the Lambda runtime")
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
                        p { "This page includes CSS assets served from Lambda." }

                        div class="styled-content" {
                            p { "Static assets are bundled and served efficiently." }
                            ul {
                                li { "CSS stylesheets" }
                                li { "JavaScript files" }
                                li { "Images and fonts" }
                            }
                        }
                    }
                }
            }
        };

        view.to_string()
    }

    // Example of using the static_asset_routes field in LambdaApp
    fn create_app_with_routes() -> LambdaApp<String, AssetProcessor> {
        let mut app = LambdaApp::new(AssetProcessor);
        app.static_asset_routes = vec![
            StaticAssetRoute {
                route: "/css/style.css".to_string(),
                target: AssetPathTarget::FileContents(
                    Bytes::from_static(include_bytes!("../assets/style.css"))
                ),
                not_found_behavior: None,
            },
        ];
        app
    }
}
```

### Environment-based Configuration

```rust
use hyperchad_renderer_html_lambda::{LambdaApp, LambdaResponseProcessor, Content};
use lambda_http::Request;
use lambda_runtime::Error;
use bytes::Bytes;
use std::{env, sync::Arc};

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

    fn render_home(&self) -> String {
        format!(
            r#"<html>
                <head><title>HyperChad - {}</title></head>
                <body>
                    <h1>HyperChad Lambda</h1>
                    <p>Environment: {}</p>
                </body>
            </html>"#,
            self.environment, self.environment
        )
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
            _ => Ok(None),
        }
    }

    async fn to_body(
        &self,
        _content: hyperchad_renderer::Content,
        _data: String,
    ) -> Result<Content, lambda_runtime::Error> {
        unimplemented!("to_body is not used by the Lambda runtime")
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

- **`json`**: Enable JSON response support (enabled by default)
- **`assets`**: Enable static asset serving support (enabled by default)
- **`debug`**: Enable debug mode features (enabled by default)

## Performance Optimizations

### Cold Start Reduction

- **Minimal Dependencies**: Only essential dependencies included
- **Binary Size**: Optimized for fast cold starts

### Memory Efficiency

- **Compression**: Automatic gzip compression for all responses
- **Binary Responses**: Efficient binary body handling

## Dependencies

- **lambda_http**: AWS Lambda HTTP event handling
- **lambda_runtime**: AWS Lambda runtime integration
- **hyperchad_renderer**: Core HTML rendering functionality
- **flate2**: Gzip compression support
- **bytes**: Efficient byte buffer handling
- **async-trait**: Async trait support
- **serde_json**: JSON serialization (optional, enabled by default)

## Integration

This renderer is designed for:

- **Serverless Web Apps**: Full serverless web applications
- **API Services**: REST and JSON APIs
- **Server-Rendered Pages**: Dynamic HTML page generation
- **Microservices**: Event-driven microservices architecture

## Limitations

- **Cold Starts**: Initial request latency for cold starts
- **Execution Time**: 15-minute maximum execution time
- **Memory**: Limited memory allocation options
- **State**: No persistent state between invocations
