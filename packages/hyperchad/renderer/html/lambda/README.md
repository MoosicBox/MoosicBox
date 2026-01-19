# HyperChad HTML Lambda Renderer

AWS Lambda renderer implementation for HyperChad HTML applications.

This crate provides a Lambda-based runtime for HyperChad HTML renderers,
enabling serverless deployment of HyperChad applications on AWS Lambda.
It handles HTTP request/response processing, gzip compression, and
integrates with the HyperChad renderer framework.

## Features

- `assets` - Enable static asset route support (enabled by default)
- `json` - Enable JSON response content type (enabled by default)
- `debug` - Enable debug logging (enabled by default)

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_html_lambda = "0.1"
```

## Usage

### Core Types

#### `Content`

Represents HTTP response content types:

- `Content::Html(String)` - HTML content with UTF-8 encoding
- `Content::Raw { data: Bytes, content_type: String }` - Raw binary content with custom MIME type
- `Content::Json(serde_json::Value)` - JSON content (requires `json` feature)

#### `LambdaResponseProcessor<T>`

The main trait to implement for handling Lambda HTTP requests. It defines four methods:

- `prepare_request` - Extracts and transforms incoming requests into your application's request type
- `headers` - Returns additional HTTP headers based on rendered content
- `to_response` - Generates response content and headers from processed data
- `to_body` - Converts rendered content to the appropriate response body type

#### `LambdaApp<T, R>`

The main entry point for creating a Lambda-based HyperChad application. Create one with `LambdaApp::new(processor)`.

When the `assets` feature is enabled, you can configure static asset routes via the `static_asset_routes` field.

### Example

```rust
use hyperchad_renderer_html_lambda::{LambdaApp, LambdaResponseProcessor, Content};
use hyperchad_renderer::ToRenderRunner;
use async_trait::async_trait;
use std::sync::Arc;
use bytes::Bytes;
use lambda_http::Request;

#[derive(Clone)]
struct MyProcessor;

#[async_trait]
impl LambdaResponseProcessor<String> for MyProcessor {
    fn prepare_request(
        &self,
        req: Request,
        body: Option<Arc<Bytes>>,
    ) -> Result<String, lambda_runtime::Error> {
        Ok(req.uri().path().to_string())
    }

    fn headers(&self, _content: &hyperchad_renderer::Content) -> Option<Vec<(String, String)>> {
        None
    }

    async fn to_response(
        &self,
        data: String,
    ) -> Result<Option<(Content, Option<Vec<(String, String)>>)>, lambda_runtime::Error> {
        Ok(Some((Content::Html(format!("<h1>Path: {}</h1>", data)), None)))
    }

    async fn to_body(
        &self,
        _content: hyperchad_renderer::Content,
        _data: String,
    ) -> Result<Content, lambda_runtime::Error> {
        Ok(Content::Html("<h1>Hello</h1>".to_string()))
    }
}

fn main() {
    let processor = MyProcessor;
    let app = LambdaApp::new(processor);
    // Use app.to_runner() to create a RenderRunner for execution
}
```

### Re-exports

This crate re-exports `lambda_http` and `lambda_runtime` for convenient access to Lambda types like `Request`, `Response`, and `Error`.

## License

See the [LICENSE](LICENSE) file for details.
