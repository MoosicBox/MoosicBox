# Switchy Web Server Actix

Actix Web backend implementation for `switchy_web_server`.

## Overview

This crate adapts the framework-agnostic Switchy web server abstractions to
Actix Web. It provides the runtime backend used when serving Switchy routes with
Actix, including request/response conversion and optional middleware support.

## Features

- Actix Web server backend for `switchy_web_server`
- Integration with `switchy_web_server_core` server traits
- Request and response conversion through `switchy_http_models`
- Optional CORS support through `switchy_web_server_cors`
- Optional static file serving
- Optional HTMX integration

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
switchy_web_server_actix = { version = "0.2.0" }
```

## Usage

This crate is usually used through the higher-level `switchy_web_server` crate's
Actix backend feature rather than directly.

```toml
[dependencies]
switchy_web_server = { version = "0.2.0", features = ["actix"] }
```

## License

MPL-2.0
