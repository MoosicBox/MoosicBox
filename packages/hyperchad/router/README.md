# HyperChad Router

Async routing system for HyperChad applications with request handling and navigation.

## Overview

The HyperChad Router package provides:

- **Async Routing**: Full async request routing and handling
- **Route Matching**: Flexible route path matching with literals and prefixes
- **Request Processing**: Comprehensive request parsing and handling
- **Form Support**: Multipart form and JSON body parsing
- **Client Detection**: OS and client information detection
- **Navigation System**: Programmatic navigation and content delivery

## Features

### Route Matching
- **Literal Routes**: Exact path matching
- **Multiple Literals**: Match against multiple possible paths
- **Prefix Matching**: Match paths with specific prefixes
- **Flexible Patterns**: Support for various route patterns

### Request Handling
- **HTTP Methods**: Support for GET, POST, PUT, DELETE, PATCH
- **Query Parameters**: Automatic query string parsing
- **Headers**: Complete header access and manipulation
- **Cookies**: Cookie parsing and management
- **Body Parsing**: JSON and form data parsing

### Form Processing
- **Multipart Forms**: Complete multipart form support
- **File Uploads**: File upload handling with base64 encoding
- **JSON Bodies**: JSON request body parsing
- **URL Encoded**: URL-encoded form data support
- **Content-Type Detection**: Automatic content type handling

### Client Information
- **OS Detection**: Automatic operating system detection
- **Client Info**: Structured client information
- **Request Context**: Rich request context and metadata

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
hyperchad_router = { path = "../hyperchad/router" }

# Enable additional features
hyperchad_router = {
    path = "../hyperchad/router",
    features = ["serde", "form", "static-routes"]
}
```

## Usage

### Basic Router Setup

```rust
use hyperchad_router::{Router, RouteRequest, RoutePath};
use hyperchad_renderer::Content;

// Create router
let router = Router::new()
    .with_route("/", |_req| async {
        Some(Content::Html("<h1>Home</h1>".to_string()))
    })
    .with_route("/about", |_req| async {
        Some(Content::Html("<h1>About</h1>".to_string()))
    });

// Navigate to route
let content = router.navigate("/").await?;
```

### Route Patterns

```rust
use hyperchad_router::RoutePath;

// Literal route
let home_route = RoutePath::Literal("/".to_string());

// Multiple literals
let api_routes = RoutePath::Literals(vec![
    "/api/v1".to_string(),
    "/api/v2".to_string(),
]);

// Prefix matching
let static_route = RoutePath::LiteralPrefix("/static/".to_string());

// From string slice arrays
let routes: RoutePath = &["/api", "/v1", "/users"][..].into();
```

### Request Information

```rust
use hyperchad_router::{RouteRequest, RequestInfo, ClientInfo, ClientOs};

// Create request with client info
let client_info = ClientInfo {
    os: ClientOs {
        name: "Windows".to_string(),
    },
};

let request = RouteRequest::from(("/api/users", client_info));

// Access request properties
println!("Path: {}", request.path);
println!("Method: {:?}", request.method);
println!("OS: {}", request.info.client.os.name);
```

### Form Handling

```rust
use serde::Deserialize;

#[derive(Deserialize)]
struct LoginForm {
    username: String,
    password: String,
}

let router = Router::new()
    .with_route_result("/login", |req| async move {
        if req.method == Method::Post {
            let form: LoginForm = req.parse_form()?;
            // Process login
            Ok(Some(Content::Html("Login successful".to_string())))
        } else {
            Ok(Some(Content::Html(r#"
                <form method="post">
                    <input name="username" type="text" required>
                    <input name="password" type="password" required>
                    <button type="submit">Login</button>
                </form>
            "#.to_string())))
        }
    });
```

### JSON Body Parsing

```rust
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct ApiRequest {
    name: String,
    email: String,
}

#[derive(Serialize)]
struct ApiResponse {
    id: u32,
    message: String,
}

let router = Router::new()
    .with_route_result("/api/users", |req| async move {
        let user_data: ApiRequest = req.parse_body()?;

        // Process user creation
        let response = ApiResponse {
            id: 123,
            message: "User created".to_string(),
        };

        Ok(Some(Content::Json(serde_json::to_string(&response)?)))
    });
```

### Static Routes

```rust
// Static routes (compiled at build time)
let router = Router::new()
    .with_static_route("/static/css/style.css", |_req| async {
        Some(Content::Css(include_str!("../static/style.css").to_string()))
    })
    .with_static_route("/static/js/app.js", |_req| async {
        Some(Content::JavaScript(include_str!("../static/app.js").to_string()))
    });
```

### Navigation and Content Delivery

```rust
// Spawn navigation in background
let handle = router.navigate_spawn("/api/data");

// Wait for navigation result
match handle.await {
    Ok(Ok(())) => println!("Navigation successful"),
    Ok(Err(e)) => println!("Navigation error: {}", e),
    Err(e) => println!("Task error: {}", e),
}

// Send navigation result to receiver
router.navigate_send("/dashboard").await?;

// Wait for content on receiver
if let Some(content) = router.wait_for_navigation().await {
    // Handle received content
}
```

### Error Handling

```rust
use hyperchad_router::{NavigateError, ParseError};

match router.navigate("/api/endpoint").await {
    Ok(Some(content)) => {
        // Handle successful navigation
    }
    Ok(None) => {
        // Route returned no content
    }
    Err(NavigateError::InvalidPath) => {
        // Invalid path provided
    }
    Err(NavigateError::Handler(e)) => {
        // Handler returned an error
    }
    Err(NavigateError::Sender) => {
        // Channel sender error
    }
}
```

## Route Types

### RoutePath Variants
- **Literal(String)**: Exact string match
- **Literals(Vec<String>)**: Match any of multiple strings
- **LiteralPrefix(String)**: Match strings with specific prefix

### Content Types
- **Html**: HTML content
- **Json**: JSON responses
- **Css**: CSS stylesheets
- **JavaScript**: JavaScript files
- **Custom**: Custom content types

## Client Information

### ClientInfo Structure
```rust
pub struct ClientInfo {
    pub os: ClientOs,
}

pub struct ClientOs {
    pub name: String,  // "Windows", "macOS", "Linux", etc.
}
```

### Default Client Detection
Automatic OS detection using the `os_info` crate provides default client information.

## Feature Flags

- **`serde`**: Enable JSON and form parsing
- **`form`**: Enable multipart form support
- **`static-routes`**: Enable static route compilation

## Dependencies

- **Tokio**: Async runtime and task management
- **Futures**: Future utilities
- **Bytes**: Efficient byte handling
- **Flume**: Channel communication
- **QString**: Query string parsing
- **Serde**: Optional serialization support
- **Mime Multipart**: Optional form parsing

## Integration

This package is designed for:
- **Web Applications**: Full-featured web application routing
- **API Servers**: RESTful API endpoint handling
- **Form Processing**: Web form and file upload handling
- **SPA Routing**: Single-page application routing
- **Content Management**: Dynamic content delivery systems
