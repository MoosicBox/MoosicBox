# HyperChad Vanilla JS Hash

Content-based hash generation for HyperChad Vanilla JS scripts with plugin-aware cache busting.

## Overview

The HyperChad Vanilla JS Hash package provides:

- **Content-based Hashing**: Generate SHA256 hashes based on script content and enabled plugins
- **Cache Busting**: Automatic cache invalidation when plugins or content change
- **Plugin Awareness**: Hash includes all enabled plugin features
- **Compile-time Generation**: Hashes computed at compile time for zero runtime cost
- **Hex Encoding**: Human-readable hexadecimal hash output

## Features

### Hash Generation

- **SHA256 Algorithm**: Secure cryptographic hashing
- **Plugin Detection**: Automatically includes enabled plugin features in hash
- **Compile-time Computation**: No runtime performance impact
- **Deterministic**: Same plugin configuration always produces same hash

### Plugin Support

All HyperChad Vanilla JS plugins are included in the hash:

#### Core Plugins

- **Navigation (`plugin-nav`)**: Client-side routing
- **Idiomorph (`plugin-idiomorph`)**: DOM morphing
- **SSE (`plugin-sse`)**: Server-Sent Events
- **Tauri Event (`plugin-tauri-event`)**: Tauri integration
- **UUID (`plugin-uuid`)**: Secure UUID generation
- **UUID Insecure (`plugin-uuid-insecure`)**: Development UUID generation
- **Routing (`plugin-routing`)**: Advanced routing
- **Event (`plugin-event`)**: Custom events
- **Canvas (`plugin-canvas`)**: Canvas support
- **Form (`plugin-form`)**: Form handling
- **HTTP Events (`plugin-http-events`)**: HTTP event handling

#### Action Plugins

- **Change Actions (`plugin-actions-change`)**: Input change handling
- **Click Actions (`plugin-actions-click`)**: Click event handling
- **Click Outside (`plugin-actions-click-outside`)**: Click outside detection
- **Event Actions (`plugin-actions-event`)**: Custom event actions
- **Event Key Down (`plugin-actions-event-key-down`)**: Event-based key down handling
- **Event Key Up (`plugin-actions-event-key-up`)**: Event-based key up handling
- **Immediate Actions (`plugin-actions-immediate`)**: Immediate execution
- **Key Down (`plugin-actions-key-down`)**: Key down events
- **Key Up (`plugin-actions-key-up`)**: Key up events
- **Mouse Down (`plugin-actions-mouse-down`)**: Mouse down events
- **Mouse Over (`plugin-actions-mouse-over`)**: Mouse over events
- **Resize Actions (`plugin-actions-resize`)**: Resize handling

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_renderer_vanilla_js_hash = { path = "../hyperchad/renderer/vanilla_js/hash" }

# Enable the same plugins as your main vanilla_js renderer
hyperchad_renderer_vanilla_js_hash = {
    path = "../hyperchad/renderer/vanilla_js/hash",
    features = [
        "plugin-nav",
        "plugin-idiomorph",
        "plugin-sse",
        "plugin-actions-click",
        "plugin-actions-change"
    ]
}
```

## Usage

### Basic Hash Generation

```rust
use hyperchad_renderer_vanilla_js_hash::{PLUGIN_HASH_HEX, PLUGIN_HASH};

fn main() {
    // Get the hex-encoded hash string
    println!("Plugin hash: {}", PLUGIN_HASH_HEX);

    // Get the plugin identifier string (input to the hash)
    println!("Raw plugin string: {}", PLUGIN_HASH);

    // Example output:
    // Plugin hash: a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456
    // Raw plugin string: plugins-nav-idiomorph-sse-actions-click-actions-change
}
```

### Script Filename Generation

```rust
use hyperchad_renderer_vanilla_js_hash::PLUGIN_HASH_HEX;

fn generate_script_filename() -> String {
    format!("hyperchad-{}.min.js", &PLUGIN_HASH_HEX[..10])
}

fn main() {
    let filename = generate_script_filename();
    println!("Script filename: {}", filename);
    // Output: hyperchad-a1b2c3d4e5.min.js
}
```

### Cache Headers

```rust
use hyperchad_renderer_vanilla_js_hash::PLUGIN_HASH_HEX;

fn get_cache_headers() -> Vec<(String, String)> {
    vec![
        ("Cache-Control".to_string(), "public, max-age=31536000".to_string()),
        ("ETag".to_string(), format!("\"{}\"", PLUGIN_HASH_HEX)),
    ]
}

fn main() {
    let headers = get_cache_headers();
    for (key, value) in headers {
        println!("{}: {}", key, value);
    }
    // Output:
    // Cache-Control: public, max-age=31536000
    // ETag: "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456"
}
```

### Web Server Integration

```rust
use hyperchad_renderer_vanilla_js_hash::PLUGIN_HASH_HEX;
use actix_web::{HttpResponse, Result};

async fn serve_script() -> Result<HttpResponse> {
    let script_content = include_str!("../assets/hyperchad.min.js");

    Ok(HttpResponse::Ok()
        .content_type("application/javascript")
        .insert_header(("Cache-Control", "public, max-age=31536000"))
        .insert_header(("ETag", format!("\"{}\"", PLUGIN_HASH_HEX)))
        .body(script_content))
}

async fn serve_versioned_script(filename: String) -> Result<HttpResponse> {
    // Extract hash from filename
    let expected_hash = &filename[10..20]; // Extract hash part

    if expected_hash == &PLUGIN_HASH_HEX[..10] {
        serve_script().await
    } else {
        Ok(HttpResponse::NotFound().body("Script version not found"))
    }
}
```

### HTML Template Integration

```rust
use hyperchad_renderer_vanilla_js_hash::PLUGIN_HASH_HEX;
use maud::{html, Markup};

fn render_page_with_script() -> Markup {
    let script_filename = format!("hyperchad-{}.min.js", &PLUGIN_HASH_HEX[..10]);

    html! {
        html {
            head {
                title { "My HyperChad App" }
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
            }
            body {
                div id="app" {
                    h1 { "Loading..." }
                }

                script src=(format!("/js/{}", script_filename)) {}
            }
        }
    }
}
```

### Development vs Production

```rust
use hyperchad_renderer_vanilla_js_hash::PLUGIN_HASH_HEX;

fn get_script_url() -> String {
    #[cfg(debug_assertions)]
    {
        // Development: no hash, for easier debugging
        "/js/hyperchad.js".to_string()
    }

    #[cfg(not(debug_assertions))]
    {
        // Production: hashed filename for cache busting
        format!("/js/hyperchad-{}.min.js", &PLUGIN_HASH_HEX[..10])
    }
}
```

### CDN Integration

```rust
use hyperchad_renderer_vanilla_js_hash::PLUGIN_HASH_HEX;

fn get_cdn_script_url() -> String {
    let hash = &PLUGIN_HASH_HEX[..10];
    format!("https://cdn.example.com/hyperchad/{}/hyperchad.min.js", hash)
}

fn main() {
    let cdn_url = get_cdn_script_url();
    println!("CDN URL: {}", cdn_url);
    // Output: https://cdn.example.com/hyperchad/a1b2c3d4e5/hyperchad.min.js
}
```

## Constants

### `PLUGIN_HASH`

Raw string containing all enabled plugin identifiers:

```rust
pub const PLUGIN_HASH: &str = "plugins-nav-idiomorph-sse-actions-click";
```

### `RAW_HASH`

Raw SHA256 hash bytes:

```rust
pub const RAW_HASH: [u8; 32] = [/* 32 bytes */];
```

### `PLUGIN_HASH_HEX`

Hexadecimal string representation of the hash:

```rust
pub const PLUGIN_HASH_HEX: &str = "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456";
```

## Plugin Detection

The hash automatically includes enabled plugins based on Cargo features:

```toml
# This configuration:
[features]
default = ["plugin-nav", "plugin-actions-click"]
plugin-nav = []
plugin-actions-click = []
```

```rust
// Generates this plugin string:
"plugins-nav-actions-click"

// Which produces this hash:
"b8e7f2a9c4d6e1f3a5b7c9d2e4f6a8b0c3d5e7f9a1b3c5d7e9f1a3b5c7d9e1f3"
```

## Feature Flags

All HyperChad Vanilla JS plugin features are supported:

### Core Plugins

- **`plugin-nav`**: Navigation plugin
- **`plugin-idiomorph`**: DOM morphing plugin
- **`plugin-sse`**: Server-Sent Events plugin
- **`plugin-tauri-event`**: Tauri integration plugin
- **`plugin-uuid`**: UUID generation plugin
- **`plugin-uuid-insecure`**: Insecure UUID plugin (development)
- **`plugin-routing`**: Advanced routing plugin
- **`plugin-event`**: Custom events plugin
- **`plugin-canvas`**: Canvas plugin
- **`plugin-form`**: Form handling plugin
- **`plugin-http-events`**: HTTP events plugin

### Action Plugins

- **`plugin-actions-change`**: Change event actions
- **`plugin-actions-click`**: Click event actions
- **`plugin-actions-click-outside`**: Click outside detection
- **`plugin-actions-event`**: Custom event actions
- **`plugin-actions-event-key-down`**: Event-based key down actions
- **`plugin-actions-event-key-up`**: Event-based key up actions
- **`plugin-actions-immediate`**: Immediate actions
- **`plugin-actions-key-down`**: Key down actions
- **`plugin-actions-key-up`**: Key up actions
- **`plugin-actions-mouse-down`**: Mouse down actions
- **`plugin-actions-mouse-over`**: Mouse over actions
- **`plugin-actions-resize`**: Resize actions

## Use Cases

### Cache Busting

Ensure clients always get the correct script version when plugins change:

```rust
// Old configuration: plugin-nav only
// Hash: a1b2c3d4e5...
// URL: /js/hyperchad-a1b2c3d4e5.min.js

// New configuration: plugin-nav + plugin-sse
// Hash: f6g7h8i9j0...
// URL: /js/hyperchad-f6g7h8i9j0.min.js
```

### CDN Deployment

Deploy different script versions to CDN based on plugin configuration:

```bash
# Deploy scripts with different plugin combinations
aws s3 cp hyperchad-basic.min.js s3://cdn/hyperchad/a1b2c3d4e5/
aws s3 cp hyperchad-full.min.js s3://cdn/hyperchad/f6g7h8i9j0/
```

### Build System Integration

Integrate with build systems for automatic asset management:

```rust
// build.rs
use hyperchad_renderer_vanilla_js_hash::PLUGIN_HASH_HEX;

fn main() {
    println!("cargo:rustc-env=SCRIPT_HASH={}", PLUGIN_HASH_HEX);
}

// main.rs
const SCRIPT_HASH: &str = env!("SCRIPT_HASH");
```

## Dependencies

- **SHA2 Const Stable**: Compile-time SHA256 hashing
- **Const Hex**: Compile-time hexadecimal encoding
- **Const Format**: Compile-time string formatting

## Performance

- **Zero Runtime Cost**: All computation happens at compile time
- **No Allocations**: Uses only compile-time constants
- **Deterministic**: Same configuration always produces same hash
- **Fast Lookups**: Hash comparison is simple string/byte comparison
