# switchy_web_server_cors

CORS (Cross-Origin Resource Sharing) configuration for web servers.

## Overview

This crate provides types for configuring CORS policies, allowing you to control
which origins, HTTP methods, and headers are permitted for cross-origin requests.

## Usage

Add the dependency to your `Cargo.toml`:

```toml
[dependencies]
switchy_web_server_cors = { path = "..." }
```

### Basic Example

```rust
use switchy_web_server_cors::Cors;
use switchy_http_models::Method;

let cors = Cors::default()
    .allow_origin("https://example.com")
    .allow_method(Method::Get)
    .allow_method(Method::Post)
    .allow_header("Content-Type")
    .support_credentials()
    .max_age(3600);
```

### Permissive Configuration

For development environments where you want to allow any cross-origin request:

```rust
use switchy_web_server_cors::Cors;

let cors = Cors::default()
    .allow_any_origin()
    .allow_any_method()
    .allow_any_header();
```

## API

### `Cors`

The main configuration struct with the following builder methods:

| Method                  | Description                          |
| ----------------------- | ------------------------------------ |
| `allow_any_origin()`    | Allow requests from any origin (`*`) |
| `allow_origin(origin)`  | Add a specific allowed origin        |
| `allowed_origins(iter)` | Add multiple allowed origins         |
| `allow_any_method()`    | Allow any HTTP method (`*`)          |
| `allow_method(method)`  | Add a specific allowed method        |
| `allowed_methods(iter)` | Add multiple allowed methods         |
| `allow_any_header()`    | Allow any header (`*`)               |
| `allow_header(header)`  | Add a specific allowed header        |
| `allowed_headers(iter)` | Add multiple allowed headers         |
| `expose_any_header()`   | Expose any response header (`*`)     |
| `expose_header(header)` | Expose a specific response header    |
| `expose_headers(iter)`  | Expose multiple response headers     |
| `support_credentials()` | Enable credentials support           |
| `max_age(seconds)`      | Set preflight cache duration         |

### `AllOrSome<T>`

An enum representing either all values are allowed or only some specific values:

- `AllOrSome::All` - Everything is allowed (equivalent to `*`)
- `AllOrSome::Some(T)` - Only specific values in `T` are allowed

## License

MPL-2.0
