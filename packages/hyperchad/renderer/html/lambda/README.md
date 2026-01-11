# HyperChad HTML Lambda Renderer

AWS Lambda renderer implementation for HyperChad HTML applications, enabling
serverless deployment on AWS Lambda with HTTP request/response processing and
gzip compression.

## Features

- `assets` - Enable static asset route support (enabled by default)
- `json` - Enable JSON response content type (enabled by default)
- `debug` - Enable debug logging (enabled by default)

## Installation

```bash
cargo add hyperchad_renderer_html_lambda
```

Or add to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_html_lambda = "0.1"
```

## Usage

Implement the `LambdaResponseProcessor` trait to handle requests and generate
responses:

```rust
use hyperchad_renderer_html_lambda::{Content, LambdaApp, LambdaResponseProcessor};
use hyperchad_renderer::ToRenderRunner;
use async_trait::async_trait;
use bytes::Bytes;
use lambda_http::Request;
use std::sync::Arc;

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

    fn headers(
        &self,
        _content: &hyperchad_renderer::Content,
    ) -> Option<Vec<(String, String)>> {
        None
    }

    async fn to_response(
        &self,
        data: String,
    ) -> Result<Option<(Content, Option<Vec<(String, String)>>)>, lambda_runtime::Error> {
        Ok(Some((
            Content::Html(format!("<h1>Path: {}</h1>", data)),
            None,
        )))
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
    let app = LambdaApp::new(MyProcessor);
    // Convert to runner and execute via ToRenderRunner trait
}
```

### Content Types

The `Content` enum supports multiple response types:

- `Content::Html(String)` - HTML content with `text/html; charset=utf-8`
- `Content::Raw { data: Bytes, content_type: String }` - Binary data with custom
  MIME type
- `Content::Json(serde_json::Value)` - JSON content (requires `json` feature)

### Static Assets

With the `assets` feature enabled, you can serve static files:

```rust
use hyperchad_renderer::assets::{AssetPathTarget, StaticAssetRoute};

let mut app = LambdaApp::new(processor);
app.static_asset_routes.push(StaticAssetRoute {
    route: "/static/style.css".to_string(),
    target: AssetPathTarget::FileContents(Bytes::from_static(b"body { margin: 0; }")),
    not_found_behavior: None,
});
```

## License

This project is licensed under the MPL-2.0 License.
