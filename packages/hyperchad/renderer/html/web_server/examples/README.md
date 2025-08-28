# HyperChad Web Server Examples

This directory contains examples demonstrating how to use the HyperChad web_server backend with moosicbox_web_server.

## Available Examples

### [Basic Web Server](./basic_web_server/)

A comprehensive example showcasing all the key features of the web_server backend:

- **Multiple Routes**: Home, About, Contact, API endpoints
- **Interactive Features**: Forms, real-time updates, API calls
- **Static Assets**: CSS, JavaScript, images
- **Responsive Design**: Mobile-first, modern styling
- **Error Handling**: 404 pages, form validation

**Quick Start:**
```bash
cd basic_web_server
cargo run --bin basic_web_server
```

Visit `http://localhost:8343` to see the example in action.

## Features Demonstrated

### Core Web Server Backend
- ✅ `router_to_web_server()` function usage
- ✅ Actix backend integration
- ✅ Route handling with async functions
- ✅ Request/response processing

### Static Asset Serving
- ✅ CSS file serving
- ✅ JavaScript file serving
- ✅ Image serving (planned)
- ✅ Embedded assets with `include_str!`

### Interactive Features
- ✅ Form handling and validation
- ✅ API endpoints with JSON responses
- ✅ Client-side JavaScript integration
- ✅ Real-time user interactions

### Modern Web Development
- ✅ Responsive CSS design
- ✅ Dark mode support
- ✅ Mobile-first approach
- ✅ Progressive enhancement

### Testing & Development
- ✅ Simulator integration (optional)
- ✅ Development-friendly setup
- ✅ Easy customization

## Getting Started

### Prerequisites

- Rust toolchain
- MoosicBox project setup

### Running Examples

From the MoosicBox root directory:

```bash
# Basic web server example
cd packages/hyperchad/renderer/html/web_server/examples/basic_web_server
cargo run --bin basic_web_server

# With simulator support
cargo run --bin basic_web_server --features simulator

# Using Nix development shell (NixOS)
nix develop .#fltk-hyperchad --command cargo run --bin basic_web_server
```

### Environment Variables

- `PORT`: Server port (default: 8343)
- `RUST_LOG`: Logging level (e.g., `debug`, `info`)

## Architecture Overview

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│   HyperChad     │    │ moosicbox_web_   │    │     Actix       │
│   Templates     │───▶│     server       │───▶│   Web Server    │
│   & Routing     │    │   (Backend)      │    │   (HTTP Layer)  │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

The web_server backend provides:

1. **HTTP Layer**: Powered by actix-web for high performance
2. **Routing**: HyperChad router with async handlers
3. **Templates**: HyperChad HTML templates with modern styling
4. **Assets**: Static file serving with efficient caching
5. **Features**: Actions, SSE, form handling, API endpoints

## Key Benefits

### Developer Experience
- **Easy Setup**: Simple `router_to_web_server()` call
- **Hot Reload**: Fast development iteration
- **Type Safety**: Full Rust type checking
- **Modern Tooling**: Cargo, clippy, rustfmt integration

### Performance
- **Actix Backend**: High-performance HTTP handling
- **Efficient Assets**: Optimized static file serving
- **Minimal JavaScript**: Enhanced UX without bloat
- **Responsive Design**: Fast loading on all devices

### Flexibility
- **Multiple Backends**: Actix or simulator modes
- **Customizable**: Easy to extend and modify
- **Scalable**: Production-ready architecture
- **Testable**: Built-in simulator support

## Common Patterns

### Basic Route Handler
```rust
router.get("/path", |_req: RouteRequest| async move {
    Ok(Some(hyperchad_renderer::Content::View(
        hyperchad_renderer::View {
            immediate: create_page(),
            deferred: None,
        },
    )))
});
```

### API Endpoint
```rust
router.get("/api/data", |_req: RouteRequest| async move {
    let data = serde_json::json!({"key": "value"});
    Ok(Some(hyperchad_renderer::Content::Json(data)))
});
```

### Static Asset
```rust
StaticAssetRoute {
    route: "/static/style.css".to_string(),
    target: AssetPathTarget::FileContents(include_str!("style.css").to_string()),
}
```

### Form Handler
```rust
router.post("/contact", |req: RouteRequest| async move {
    // Process form data from req.body
    Ok(Some(hyperchad_renderer::Content::View(
        hyperchad_renderer::View {
            immediate: create_success_page(),
            deferred: None,
        },
    )))
});
```

## Best Practices

### Project Structure
```
your_app/
├── Cargo.toml
├── src/
│   ├── main.rs          # Application entry point
│   ├── routes/          # Route handlers
│   ├── templates/       # HTML templates
│   └── models/          # Data structures
├── static/
│   ├── css/            # Stylesheets
│   ├── js/             # JavaScript
│   └── images/         # Images
└── README.md
```

### Error Handling
- Always provide fallback routes for 404 errors
- Validate user input on both client and server
- Use proper HTTP status codes
- Provide helpful error messages

### Performance
- Use `include_str!` for small assets
- Implement proper caching headers
- Optimize CSS and JavaScript
- Use responsive images

### Security
- Validate all user input
- Use HTTPS in production
- Implement proper CORS policies
- Sanitize HTML content

## Troubleshooting

### Common Issues

**Port Already in Use**
```bash
PORT=8080 cargo run --bin basic_web_server
```

**Build Errors**
```bash
# Check dependencies
cargo check -p hyperchad_renderer_html_web_server

# Clean build
cargo clean && cargo build
```

**Static Assets Not Loading**
- Ensure files are included with `include_str!`
- Check route paths match asset routes
- Verify file permissions

### Getting Help

1. Check the example README files
2. Review the source code comments
3. Test with the simulator feature
4. Check the MoosicBox documentation

## Contributing

To add new examples:

1. Create a new directory under `examples/`
2. Follow the existing structure and patterns
3. Include comprehensive documentation
4. Test thoroughly with both actix and simulator
5. Submit a pull request

## Future Examples

Planned examples include:

- **Database Integration**: Using SQLite/PostgreSQL
- **Authentication**: User login/logout
- **WebSocket Chat**: Real-time communication
- **File Upload**: Handling multipart forms
- **API Gateway**: Microservice integration
- **SSE Dashboard**: Real-time monitoring

## License

These examples are part of the MoosicBox project and follow the same license terms.