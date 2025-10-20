# HTTP Models

HTTP protocol models and types for methods, status codes, and framework integration.

## Overview

The HTTP Models package provides:

- **HTTP Methods**: Complete HTTP method enumeration with parsing
- **Status Codes**: Comprehensive HTTP status code definitions
- **Framework Integration**: Actix Web and Reqwest compatibility layers
- **Type Safety**: Strong typing for HTTP protocol elements
- **Serialization**: Serde support for JSON/API integration

## Features

### HTTP Methods

- **Complete Method Set**: GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, CONNECT, TRACE
- **Case-insensitive Parsing**: Parse from various string formats
- **Display Implementation**: Convert methods back to strings
- **Serde Integration**: JSON serialization/deserialization

### HTTP Status Codes

- **Common Status Codes**: Standard HTTP status codes including informational (1xx), success (2xx), redirection (3xx), client error (4xx), and server error (5xx)
- **Category Helpers**: Check if status is informational, success, redirection, client error, or server error
- **Numeric Conversion**: Convert to/from u16 values
- **MDN Documentation**: Based on Mozilla Developer Network reference

### Framework Integration

- **Actix Web**: Conversion traits for actix-web types
- **Reqwest**: Conversion traits for reqwest HTTP client
- **Generic Support**: Works with any HTTP framework

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
switchy_http_models = { path = "../http/models" }

# With specific features (default includes actix, reqwest, serde)
switchy_http_models = {
    path = "../http/models",
    default-features = false,
    features = ["actix", "reqwest", "serde"]
}
```

## Usage

### HTTP Methods

```rust
use switchy_http_models::Method;
use std::str::FromStr;

// Create methods
let get = Method::Get;
let post = Method::Post;

// Parse from strings (case-insensitive)
let method = Method::from_str("GET")?;
let method = Method::from_str("post")?;
let method = Method::from_str("Put")?;

// Convert to string
println!("{}", Method::Get); // "GET"
println!("{}", Method::Post); // "POST"

// Use in match expressions
match method {
    Method::Get => println!("GET request"),
    Method::Post => println!("POST request"),
    Method::Put | Method::Patch => println!("Update request"),
    Method::Delete => println!("DELETE request"),
    _ => println!("Other method"),
}
```

### HTTP Status Codes

```rust
use switchy_http_models::StatusCode;

// Create status codes
let ok = StatusCode::Ok;
let not_found = StatusCode::NotFound;
let internal_error = StatusCode::InternalServerError;

// Convert to/from numeric values
let code: u16 = StatusCode::Ok.into(); // 200
let status = StatusCode::try_from(404)?; // StatusCode::NotFound

// Category checking
assert!(StatusCode::Ok.is_success());
assert!(StatusCode::NotFound.is_client_error());
assert!(StatusCode::InternalServerError.is_server_error());
assert!(StatusCode::MovedPermanently.is_redirection());
assert!(StatusCode::Continue.is_informational());

// Display status codes
println!("{}", StatusCode::Ok); // "OK"
println!("{}", StatusCode::NotFound); // "NOT_FOUND"
```

### Status Code Categories

```rust
use switchy_http_models::StatusCode;

// Informational (1xx)
assert!(StatusCode::Continue.is_informational());
assert!(StatusCode::SwitchingProtocols.is_informational());

// Success (2xx)
assert!(StatusCode::Ok.is_success());
assert!(StatusCode::Created.is_success());
assert!(StatusCode::NoContent.is_success());

// Redirection (3xx)
assert!(StatusCode::MovedPermanently.is_redirection());
assert!(StatusCode::Found.is_redirection());
assert!(StatusCode::NotModified.is_redirection());

// Client Error (4xx)
assert!(StatusCode::BadRequest.is_client_error());
assert!(StatusCode::Unauthorized.is_client_error());
assert!(StatusCode::NotFound.is_client_error());

// Server Error (5xx)
assert!(StatusCode::InternalServerError.is_server_error());
assert!(StatusCode::BadGateway.is_server_error());
assert!(StatusCode::ServiceUnavailable.is_server_error());
```

### Serde Integration

```rust
use switchy_http_models::{Method, StatusCode};
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
struct HttpRequest {
    method: Method,
    path: String,
}

#[derive(Serialize, Deserialize)]
struct HttpResponse {
    status: StatusCode,
    body: String,
}

// Serialize to JSON
let request = HttpRequest {
    method: Method::Post,
    path: "/api/users".to_string(),
};
let json = serde_json::to_string(&request)?;
// {"method":"POST","path":"/api/users"}

let response = HttpResponse {
    status: StatusCode::Created,
    body: "User created".to_string(),
};
let json = serde_json::to_string(&response)?;
// {"status":"CREATED","body":"User created"}
```

### Actix Web Integration

```rust
use switchy_http_models::StatusCode;
use actix_web::{HttpResponse, Result};

async fn handler() -> Result<HttpResponse> {
    // Convert HTTP models to Actix Web types (requires actix feature)
    #[cfg(feature = "actix")]
    {
        let status: actix_web::http::StatusCode = StatusCode::Ok.into();
        Ok(HttpResponse::build(status).json("Success"))
    }
    #[cfg(not(feature = "actix"))]
    {
        Ok(HttpResponse::Ok().json("Success"))
    }
}
```

### Reqwest Integration

```rust
use switchy_http_models::{Method, StatusCode};

async fn make_request() -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    // Convert HTTP models to reqwest types (requires reqwest feature)
    #[cfg(feature = "reqwest")]
    {
        let method: reqwest::Method = Method::Post.into();

        let response = client
            .request(method, "https://api.example.com/users")
            .send()
            .await?;

        // Convert reqwest status code to HTTP models
        let status: StatusCode = response.status().into();
        println!("Response status: {}", status);
    }

    Ok(())
}
```

## Complete Status Code List

### Informational (1xx)

- `Continue` (100)
- `SwitchingProtocols` (101)
- `Processing` (102)
- `EarlyHints` (103)

### Success (2xx)

- `Ok` (200)
- `Created` (201)
- `Accepted` (202)
- `NonAuthoritativeInformation` (203)
- `NoContent` (204)
- `ResetContent` (205)
- `PartialContent` (206)
- `MultiStatus` (207)
- `AlreadyReported` (208)
- `IMUsed` (226)

### Redirection (3xx)

- `MultipleChoices` (300)
- `MovedPermanently` (301)
- `Found` (302)
- `SeeOther` (303)
- `NotModified` (304)
- `UseProxy` (305)
- `TemporaryRedirect` (307)
- `PermanentRedirect` (308)

### Client Error (4xx)

- `BadRequest` (400)
- `Unauthorized` (401)
- `PaymentRequired` (402)
- `Forbidden` (403)
- `NotFound` (404)
- `MethodNotAllowed` (405)
- `NotAcceptable` (406)
- `ProxyAuthenticationRequired` (407)
- `RequestTimeout` (408)
- `Conflict` (409)
- `Gone` (410)
- `LengthRequired` (411)
- `PreconditionFailed` (412)
- `ContentTooLarge` (413)
- `URITooLong` (414)
- `UnsupportedMediaType` (415)
- `RangeNotSatisfiable` (416)
- `ExpectationFailed` (417)
- `ImATeapot` (418)
- `MisdirectedRequest` (421)
- `UncompressableContent` (422)
- `Locked` (423)
- `FailedDependency` (424)
- `TooEarly` (425)
- `UpgradeRequired` (426)
- `PreconditionRequired` (428)
- `TooManyRequests` (429)
- `RequestHeaderFieldsTooLarge` (431)
- `UnavailableForLegalReasons` (451)

### Server Error (5xx)

- `InternalServerError` (500)
- `NotImplemented` (501)
- `BadGateway` (502)
- `ServiceUnavailable` (503)
- `GatewayTimeout` (504)
- `HTTPVersionNotSupported` (505)
- `VariantAlsoNegotiates` (506)
- `InsufficientStorage` (507)
- `LoopDetected` (508)
- `NotExtended` (510)
- `NetworkAuthenticationRequired` (511)

## Error Handling

```rust
use switchy_http_models::{Method, StatusCode, InvalidMethod};
use std::str::FromStr;

// Method parsing errors
match Method::from_str("INVALID") {
    Ok(method) => println!("Parsed method: {}", method),
    Err(InvalidMethod) => println!("Invalid HTTP method"),
}

// Status code conversion errors
match StatusCode::try_from(999) {
    Ok(status) => println!("Status: {}", status),
    Err(_) => println!("Invalid status code"),
}
```

## Feature Flags

- **`serde`**: Enable JSON serialization/deserialization
- **`actix`**: Enable Actix Web integration
- **`reqwest`**: Enable Reqwest HTTP client integration

## Dependencies

- **moosicbox_assert**: Internal assertion utilities
- **Serde**: Serialization support (optional, enabled by default)
- **Strum**: Enum utilities for string conversion
- **Thiserror**: Error handling
- **Actix Web**: Web framework integration (optional, enabled by default)
- **Reqwest**: HTTP client integration (optional, enabled by default)

## Use Cases

- **HTTP Client Libraries**: Type-safe HTTP method and status handling
- **Web Frameworks**: Request/response type safety
- **API Development**: Consistent HTTP protocol handling
- **Testing**: Mock HTTP responses with proper types
- **Logging**: Structured logging of HTTP requests/responses
