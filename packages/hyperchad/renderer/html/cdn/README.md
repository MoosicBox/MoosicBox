# HyperChad HTML CDN Optimization

This package provides CDN optimization utilities for HyperChad HTML applications.

## Features

- **Automatic CDN skeleton generation**: Creates optimized `index.html` for CDN deployment
- **Dynamic content fetching**: Skeleton fetches full application content via `fetch()` API
- **Zero configuration**: Automatically detects when optimization is needed
- **Document replacement**: Uses `document.open()/write()/close()` for seamless content replacement

## Configuration

The `setup_cdn_optimization` function accepts optional HTML head configuration:

- `title: Option<&str>` - Page title for the skeleton HTML
- `viewport: Option<&str>` - Viewport meta tag content

If `None` is provided, the corresponding HTML elements are omitted from the skeleton.

## Usage

```rust
use hyperchad_router::Router;
use hyperchad_renderer_html_cdn::setup_cdn_optimization;

// Create router with dynamic root route
let router = Router::new()
    .with_route("/", |_req| async move { "Hello, World!" });

// Setup CDN optimization with custom title and viewport
let router = setup_cdn_optimization(
    router,
    Some("My App"),
    Some("width=device-width, initial-scale=1")
);
```

### Usage Examples

```rust
// Minimal setup (no title/viewport)
let router = setup_cdn_optimization(router, None, None);

// Custom values
let router = setup_cdn_optimization(
    router,
    Some("Loading My App..."),
    Some("width=device-width, initial-scale=1, user-scalable=no")
);
```

## How It Works

1. **Detection**: Only activates if root route ("/") is dynamic (not static)
2. **Skeleton Generation**: Replaces root route with a static route containing optimized skeleton `index.html`
3. **Dynamic Endpoint**: Registers `/__hyperchad_dynamic_root__` that serves the full application content
4. **Runtime**: CDN serves skeleton → browser fetches dynamic content via `fetch()` → document replaced using `document.open()/write()/close()`

## CDN Architecture

```
CDN (Static Origin)
├── index.html          # Optimized skeleton (fast load)
├── css/styles.css      # Static assets
├── js/app.js          # Static assets
└── images/            # Static assets

Compute Backend
└── /__hyperchad_dynamic_root__  # Dynamic content endpoint
```

This architecture provides:
- **Fast initial load**: Skeleton served from CDN edge
- **Dynamic content**: Full application functionality preserved
- **Cost efficiency**: Static assets don't consume compute resources
- **Scalability**: CDN handles traffic spikes automatically
