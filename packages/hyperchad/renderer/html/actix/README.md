# HyperChad HTML Actix Renderer

Actix Web server renderer for HyperChad HTML applications.

## Overview

This crate provides an Actix Web integration for the HyperChad renderer framework, enabling server-side rendering of HyperChad applications with support for:

- Server-sent events (SSE) for real-time updates
- Action handling for interactive user events
- Static asset serving
- Custom response processing

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_html_actix = { version = "0.1.4" }
```

## Features

| Feature   | Default | Description                                                                 |
| --------- | ------- | --------------------------------------------------------------------------- |
| `actions` | Yes     | Enables action handling for interactive user events via `/$action` endpoint |
| `assets`  | Yes     | Enables static asset serving for files and directories                      |
| `debug`   | Yes     | Enables debug logging                                                       |
| `sse`     | Yes     | Enables server-sent events for real-time updates via `/$sse` endpoint       |

## Usage

### Basic Setup

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
    let (_tx, rx) = flume::unbounded::<RendererEvent>();
    let processor = MyProcessor;
    let app = ActixApp::new(processor, rx);
    // Use app.to_runner() to create a RenderRunner
}
```

### Core Types

#### `ActixResponseProcessor<T>` Trait

The main trait for processing HTTP requests and converting content to responses:

- `prepare_request` - Prepares request data from the HTTP request and body
- `to_response` - Converts prepared data into an HTTP response
- `to_body` - Converts content and prepared data into response body bytes and content type

#### `ActixApp<T, R>`

The Actix web application struct. Create with `ActixApp::new(processor, renderer_event_rx)`.

Methods:

- `with_action_tx` - Sets the action transmitter channel (requires `actions` feature)
- `set_action_tx` - Sets the action transmitter channel in place (requires `actions` feature)

### Configuration

The server listens on `0.0.0.0:8343` by default. Override with the `PORT` environment variable.

## License

This project is licensed under the MPL-2.0 License.
