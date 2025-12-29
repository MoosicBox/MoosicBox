# HyperChad HTML Actix Renderer

Actix web server renderer for HyperChad HTML applications.

## Features

This crate provides an Actix Web integration for the HyperChad renderer framework, enabling server-side rendering with support for:

- **Server-sent events (SSE)** for real-time updates (with `sse` feature)
- **Action handling** for interactive user events (with `actions` feature)
- **Static asset serving** (with `assets` feature)
- **Custom response processing** through the `ActixResponseProcessor` trait

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_html_actix = "0.1.4"
```

### Features

| Feature   | Default | Description                                    |
| --------- | ------- | ---------------------------------------------- |
| `actions` | Yes     | Action handling for interactive user events    |
| `assets`  | Yes     | Static asset serving for files and directories |
| `sse`     | Yes     | Server-sent events for real-time updates       |
| `debug`   | Yes     | Debug logging                                  |

## Usage

### Basic Setup

Implement the `ActixResponseProcessor` trait to handle HTTP request/response conversion:

```rust
use hyperchad_renderer_html_actix::{ActixApp, ActixResponseProcessor};
use hyperchad_renderer::{RendererEvent, Content};
use actix_web::{HttpRequest, HttpResponse};
use bytes::Bytes;
use std::sync::Arc;
use async_trait::async_trait;

#[derive(Clone)]
struct MyProcessor;

#[async_trait]
impl ActixResponseProcessor<()> for MyProcessor {
    fn prepare_request(
        &self,
        _req: HttpRequest,
        _body: Option<Arc<Bytes>>,
    ) -> Result<(), actix_web::Error> {
        Ok(())
    }

    async fn to_response(&self, _data: ()) -> Result<HttpResponse, actix_web::Error> {
        Ok(HttpResponse::Ok().finish())
    }

    async fn to_body(
        &self,
        _content: Content,
        _data: (),
    ) -> Result<(Bytes, String), actix_web::Error> {
        Ok((Bytes::new(), "text/html".to_string()))
    }
}

fn main() {
    let (tx, rx) = flume::unbounded::<RendererEvent>();
    let processor = MyProcessor;
    let app = ActixApp::new(processor, rx);
    // Use app.to_runner() to create a RenderRunner
}
```

### Configuration

The server listens on `0.0.0.0:8343` by default. Configure the port using the `PORT` environment variable:

```bash
PORT=8080 cargo run
```

### Action Handling

When the `actions` feature is enabled, set up an action transmitter to receive user-triggered events:

```rust
let (action_tx, action_rx) = flume::unbounded();
let app = ActixApp::new(processor, rx).with_action_tx(action_tx);
```

Actions are posted to `/$action` and forwarded through the action channel.

### Static Assets

When the `assets` feature is enabled, configure static asset routes via the `static_asset_routes` field on `ActixApp`.

## License

This project is licensed under the MPL-2.0 License.
