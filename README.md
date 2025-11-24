# MoosicBox

**A modern, high-fidelity music server and streaming platform for cows**

![MoosicBox](https://github.com/MoosicBox/Files/blob/master/animation.gif?raw=true)

## 🎵 Listen to your HiFi music anywhere

MoosicBox is a powerful, self-hosted music server that lets you stream your personal music library and access premium music services from anywhere. Built with performance and audio quality in mind, MoosicBox provides a seamless listening experience across all your devices.

**[📱 Download MoosicBox](https://moosicbox.com/download)** | **[🏠 Visit Website](https://moosicbox.com)**

## ⚠️ Project Status

**Development Notice:** This is currently a personal side project in active development. Most features are experimental and may not work as expected. While the goal is to eventually create a stable, user-friendly music app that others can enjoy, the project is not yet ready for general use. Your mileage may vary significantly at this stage.

---

## ✨ Key Features

### 🎧 **Hi-Fi Audio Experience**

- **Lossless audio streaming** with support for FLAC, ALAC, and other high-quality formats
- **Real-time audio encoding** (AAC, MP3, Opus) optimized for your connection
- **High-resolution audio support** for audiophile-grade listening

### 🌐 **Multi-Platform Access**

- **Web interface** - Stream from any browser
- **Desktop applications** - Native apps for Windows, macOS, and Linux
- **Mobile apps** - Android (in-progress)
- **Cross-device sync** - Continue listening where you left off

### 🎼 **Music Service Integration**

- **Local library** - Your personal music collection
- **Tidal** - Access millions of hi-fi tracks and MQA content
- **Qobuz** - Studio-quality streaming with large catalog
- **Global search** across all connected services and local library

### 🏠 **Self-Hosted & Private**

- **Complete ownership** of your music server
- **No internet required** for local music playback
- **Privacy-focused** - your data stays on your devices

### 🔧 **Advanced Features**

- **Multi-zone audio** - Play different music in different rooms
- **Multiple simultaneous outputs** - Stream to multiple devices at once
- **Automatic image optimization** - Perfect album art for any screen size
- **Database flexibility** - Support for PostgreSQL, MySQL, and SQLite
- **Audio visualization** - See waveforms and track progress
- **Remote library access** - Connect to other MoosicBox servers

---

## 🚀 Getting Started

### Quick Start Options

1. **📱 Download Apps**: Get native apps at [moosicbox.com/download](https://moosicbox.com/download)
2. **🛠️ Self-Host**: Set up your own server using the instructions below

### Self-Hosting Your MoosicBox Server

#### Prerequisites

- **Rust toolchain** (latest stable)

#### Installation & Setup

1. **Clone the repository**:

    ```bash
    git clone https://github.com/MoosicBox/MoosicBox.git
    cd MoosicBox
    ```

2. **Start the server**:

    ```bash
    # Default port is 8000
    cargo run -p moosicbox_server

    # Or specify a custom port
    PORT=8001 cargo run -p moosicbox_server
    ```

3. **Access your server**:
    - Open your browser to `http://localhost:8000` (or your custom port)
    - Start adding your music library and connecting services

#### Advanced Configuration

**Development mode with debugging**:

```bash
RUST_BACKTRACE=1 RUST_LOG="moosicbox=debug" cargo run -p moosicbox_server
```

---

## 🏗️ Architecture

MoosicBox is built with a modular, high-performance architecture:

### Core Technologies

- **🦀 Rust** - Memory-safe, high-performance backend
- **🌐 Web Technologies** - Modern web interface with TypeScript
- **🎨 HyperChad UI Framework** - Custom reactive UI system
- **📱 Tauri** - Cross-platform desktop applications
- **🔄 Real-time sync** - WebSocket-based live updates

### Supported Platforms

- **🖥️ Desktop**: Windows, macOS, Linux (via Tauri)
- **📱 Mobile**: Android
- **🌐 Web**: All modern browsers
- **🐧 Server**: Linux, Docker, cloud deployments

---

## 📊 Database Support

MoosicBox supports multiple database backends for maximum flexibility:

- **SQLite** - Perfect for personal use and getting started
- **PostgreSQL** - Recommended for production and multi-user setups
- **MySQL** - Full feature support with basic MySQL compatibility

## 🔧 Development

MoosicBox is built as a comprehensive Rust workspace with 150+ packages:

### Key Development Commands

```bash
# Run the main server (default port 8000)
cargo run -p moosicbox_server

# Run with custom port
PORT=8001 cargo run -p moosicbox_server

# Run with debug logging
RUST_LOG="moosicbox=debug" cargo run -p moosicbox_server

# Run tests
cargo test

# Check code quality
cargo clippy --all-targets --all-features

# Format code
cargo fmt
```

### Project Structure

- **`packages/`** - Modular Rust packages (audio, networking, UI, etc.)
- **`app-website/`** - Marketing website source
- **`kubernetes/`** - Kubernetes deployment manifests
- **`terraform/`** - Infrastructure as code

---

## 📄 License

MoosicBox is licensed under the [Mozilla Public License 2.0](LICENSE).

## 🔗 Links

- **🌐 Website**: [moosicbox.com](https://moosicbox.com)
- **📱 Downloads**: [moosicbox.com/download](https://moosicbox.com/download)
- **🐛 Issues**: [GitHub Issues](https://github.com/MoosicBox/MoosicBox/issues)

---

<details>
<summary><strong>📦 All Workspace Packages (Click to expand)</strong></summary>

### Core Application

- **[moosicbox](packages/moosicbox)** - Main MoosicBox server application
- **[moosicbox_server](packages/server)** - Core server implementation and HTTP handlers
- **[moosicbox_server_simulator](packages/server/simulator)** - Server simulation utilities for testing

### Audio & Media

- **[moosicbox_audio_decoder](packages/audio_decoder)** - Audio decoding with support for multiple formats
- **[moosicbox_audio_encoder](packages/audio_encoder)** - Audio encoding utilities with feature-gated support
- **[moosicbox_audio_output](packages/audio_output)** - Multi-platform audio output management
- **[moosicbox_audio_zone](packages/audio_zone)** - Audio zone database management with CRUD operations
- **[moosicbox_audio_zone_models](packages/audio_zone/models)** - Data models for audio zone management
- **[moosicbox_player](packages/player)** - High-performance audio player with playback controls
- **[moosicbox_resampler](packages/resampler)** - Audio resampling utilities for format conversion
- **[moosicbox_downloader](packages/downloader)** - Media downloading and caching system
- **[moosicbox_opus](packages/opus)** - Opus codec integration
- **[moosicbox_opus_native](packages/opus_native)** - Native Opus codec bindings
- **[moosicbox_opus_native_libopus](packages/opus_native/libopus)** - LibOpus native library
- **[aconverter](packages/aconverter)** - Audio converter helper binary

### Music Services Integration

- **[moosicbox_tidal](packages/tidal)** - Tidal music service integration with comprehensive API
- **[moosicbox_qobuz](packages/qobuz)** - Qobuz hi-fi music service integration
- **[moosicbox_yt](packages/yt)** - YouTube Music API integration

### Library & Search

- **[moosicbox_library](packages/library)** - Music library management and database operations
- **[moosicbox_library_models](packages/library/models)** - Core data models for music library
- **[moosicbox_library_music_api](packages/library/music_api)** - Music API integration for library
- **[moosicbox_remote_library](packages/remote_library)** - HTTP client for remote music servers
- **[moosicbox_search](packages/search)** - High-performance full-text search engine using Tantivy
- **[moosicbox_scan](packages/scan)** - Library scanning and metadata extraction
- **[moosicbox_scan_models](packages/scan/models)** - Data models for library scanning

### Music API & Models

- **[moosicbox_music_api](packages/music_api)** - Unified music API with service integrations
- **[moosicbox_music_api_api](packages/music_api/api)** - Core API implementations and endpoints
- **[moosicbox_music_api_helpers](packages/music_api/helpers)** - Helper utilities for music APIs
- **[moosicbox_music_api_models](packages/music_api/models)** - Data models for music API
- **[moosicbox_music_models](packages/music/models)** - Core music data models and types

### Application Framework

- **[moosicbox_app_models](packages/app/models)** - Application data models and structures
- **[moosicbox_app_state](packages/app/state)** - Application state management system
- **[moosicbox_app_native](packages/app/native)** - Native application components
- **[moosicbox_app_native_bundled](packages/app/native/bundled)** - Bundled native app service
- **[moosicbox_app_native_image](packages/app/native/image)** - Image asset management for native apps
- **[moosicbox_app_native_ui](packages/app/native/ui)** - Native UI components and widgets

### Desktop Application (Tauri)

- **[moosicbox_app_tauri_bundled](packages/app/tauri/bundled)** - Bundled Tauri application
- **[moosicbox_app_client](packages/app/tauri/client)** - Tauri client utilities and bindings
- **[tauri_create_config](packages/app/tauri/create_config)** - Tauri configuration generator
- **[moosicbox](packages/app/tauri/src-tauri)** - MoosicBox Tauri desktop application
- **[app-tauri-plugin-player](packages/app/tauri/tauri-plugin-player)** - Tauri plugin for audio player
- **[moosicbox_app_ws](packages/app/tauri/ws)** - Tauri WebSocket integration

### HyperChad UI Framework

- **[hyperchad](packages/hyperchad)** - Core HyperChad UI framework
- **[hyperchad_actions](packages/hyperchad/actions)** - Action system for HyperChad
- **[hyperchad_app](packages/hyperchad/app)** - HyperChad application framework
- **[hyperchad_color](packages/hyperchad/color)** - Color utilities and theming
- **[hyperchad_js_bundler](packages/hyperchad/js_bundler)** - JavaScript bundling for HyperChad
- **[hyperchad_markdown](packages/hyperchad/markdown)** - Markdown to HyperChad Container conversion with GitHub Flavored Markdown support
- **[hyperchad_router](packages/hyperchad/router)** - Client-side routing system
- **[hyperchad_state](packages/hyperchad/state)** - State management for HyperChad
- **[hyperchad_template](packages/hyperchad/template)** - Template system and DSL
- **[hyperchad_template_actions_dsl](packages/hyperchad/template/actions_dsl)** - DSL for template actions
- **[hyperchad_template_macros](packages/hyperchad/template/macros)** - Template system macros
- **[hyperchad_transformer](packages/hyperchad/transformer)** - UI transformation system
- **[hyperchad_transformer_models](packages/hyperchad/transformer/models)** - Models for UI transformations

### HyperChad Renderers

- **[hyperchad_renderer](packages/hyperchad/renderer)** - Core rendering abstractions
- **[hyperchad_renderer_egui](packages/hyperchad/renderer/egui)** - Native desktop renderer using egui
- **[hyperchad_renderer_fltk](packages/hyperchad/renderer/fltk)** - Cross-platform native GUI renderer using FLTK
- **[hyperchad_renderer_html](packages/hyperchad/renderer/html)** - Server-side HTML renderer
- **[hyperchad_renderer_html_actix](packages/hyperchad/renderer/html/actix)** - Actix Web integration for HTML renderer
- **[hyperchad_renderer_html_cdn](packages/hyperchad/renderer/html/cdn)** - CDN integration for HTML renderer
- **[hyperchad_renderer_html_http](packages/hyperchad/renderer/html/http)** - Generic HTTP server integration
- **[hyperchad_renderer_html_lambda](packages/hyperchad/renderer/html/lambda)** - AWS Lambda integration for serverless deployment
- **[hyperchad_renderer_html_web_server](packages/hyperchad/renderer/html/web_server)** - Web server utilities for HTML renderer
- **[hyperchad_renderer_vanilla_js](packages/hyperchad/renderer/vanilla_js)** - Client-side JavaScript renderer
- **[hyperchad_renderer_vanilla_js_hash](packages/hyperchad/renderer/vanilla_js/hash)** - Content-based hash generation for cache busting

### Web & Networking

- **[moosicbox_web_server](packages/web_server)** - Web server abstraction and utilities
- **[moosicbox_web_server_core](packages/web_server/core)** - Core web server functionality
- **[moosicbox_web_server_cors](packages/web_server/cors)** - CORS middleware for web servers
- **[switchy_http](packages/http)** - Generic HTTP client abstraction
- **[switchy_http_models](packages/http/models)** - HTTP protocol models and types
- **[moosicbox_ws](packages/ws)** - WebSocket utilities and abstractions
- **[moosicbox_middleware](packages/middleware)** - HTTP middleware collection

### Networking & Discovery

- **[moosicbox_tunnel](packages/tunnel)** - Tunneling utilities and protocols
- **[moosicbox_tunnel_sender](packages/tunnel_sender)** - WebSocket-based tunneling client
- **[moosicbox_tunnel_server](packages/tunnel_server)** - WebSocket-based tunneling server
- **[switchy_tcp](packages/tcp)** - Generic TCP networking abstraction
- **[switchy_upnp](packages/upnp)** - UPnP device discovery and communication
- **[switchy_mdns](packages/mdns)** - mDNS service registration and discovery
- **[switchy_p2p](packages/p2p)** - Peer-to-peer networking utilities
- **[moosicbox_load_balancer](packages/load_balancer)** - Load balancing utilities
- **[openport](packages/openport)** - Find free unused network ports

### Authentication & Security

- **[moosicbox_auth](packages/auth)** - Authentication utilities and client registration
- **[moosicbox_profiles](packages/profiles)** - User profile management and validation
- **[moosicbox_session](packages/session)** - Session management utilities
- **[moosicbox_session_models](packages/session/models)** - Data models for session management

### Database & Storage

- **[switchy_database](packages/database)** - Database abstraction layer
- **[switchy_database_connection](packages/database_connection)** - Database connection management
- **[switchy_schema](packages/switchy/schema)** - Database schema and migration framework
- **[switchy_schema_cli](packages/switchy/schema/cli)** - CLI tool for schema migrations
- **[moosicbox_schema](packages/schema)** - Database migration system
- **[switchy_fs](packages/fs)** - Cross-platform filesystem abstraction

### Utilities & Infrastructure

- **[moosicbox_config](packages/config)** - Configuration utilities for applications
- **[switchy_env](packages/env)** - Environment configuration utilities
- **[moosicbox_env_utils](packages/env_utils)** - Environment variable parsing utilities
- **[moosicbox_logging](packages/logging)** - Logging utilities with feature-gated modules
- **[switchy_telemetry](packages/telemetry)** - OpenTelemetry integration for distributed tracing
- **[moosicbox_async_service](packages/async_service)** - Service framework for async applications
- **[switchy_time](packages/time)** - Time abstraction utilities
- **[switchy_random](packages/random)** - Random number generation utilities
- **[switchy_uuid](packages/uuid)** - UUID generation and handling utilities

### Data Processing & Parsing

- **[moosicbox_json_utils](packages/json_utils)** - JSON parsing utilities and helpers
- **[moosicbox_parsing_utils](packages/parsing_utils)** - Utilities for parsing integer sequences and ranges
- **[moosicbox_date_utils](packages/date_utils)** - Date parsing and manipulation utilities
- **[moosicbox_paging](packages/paging)** - Pagination utilities for data sets
- **[moosicbox_stream_utils](packages/stream_utils)** - Byte stream utilities with ByteWriter/ByteStream
- **[moosicbox_channel_utils](packages/channel_utils)** - Channel utilities for async communication

### Media & Image Processing

- **[moosicbox_image](packages/image)** - Image processing and optimization utilities
- **[moosicbox_files](packages/files)** - File handling and streaming utilities

### UI & Interface

- **[moosicbox_menu](packages/menu)** - Menu system utilities
- **[moosicbox_menu_models](packages/menu/models)** - Data models for menu system
- **[moosicbox_admin_htmx](packages/admin_htmx)** - HTMX API endpoints for administrative operations
- **[moosicbox_marketing_site](packages/marketing_site)** - Marketing website implementation
- **[moosicbox_marketing_site_ui](packages/marketing_site/ui)** - UI components for marketing site

### Development & Testing

- **[simvar](packages/simvar)** - Simulation variable system
- **[simvar_harness](packages/simvar/harness)** - Simulation testing framework
- **[simvar_utils](packages/simvar/utils)** - Simulation utilities and helpers
- **[moosicbox_arb](packages/arb)** - Arbitrary data generation for testing
- **[moosicbox_clippier](packages/clippier)** - Workspace analysis and CI generation tool
- **[clippier_test_utilities](packages/clippier/test_utilities)** - Testing utilities for Clippier
- **[hyperchad_simulator](packages/hyperchad/simulator)** - HyperChad simulation utilities
- **[hyperchad_test_utils](packages/hyperchad/test_utils)** - HyperChad testing utilities
- **[switchy_schema_test_utils](packages/switchy/schema/test_utils)** - Schema testing utilities
- **[web_server_simulator](packages/web_server/simulator)** - Web server simulation utilities
- **[bloaty](packages/bloaty)** - Binary analysis utilities (placeholder)

### Core Libraries

- **[switchy](packages/switchy)** - Feature-gated re-exports for cross-platform compatibility
- **[switchy_async](packages/async)** - Async runtime abstraction
- **[switchy_async_cargo](packages/async/cargo)** - Cargo integration for async runtime
- **[switchy_async_macros](packages/async/macros)** - Macros for async runtime
- **[moosicbox_assert](packages/assert)** - Conditional assertion macros

### Transpiler & Code Generation

- **[gpipe](packages/gpipe)** - General-purpose transpiler framework
- **[gpipe_ast](packages/gpipe/ast)** - Abstract syntax tree for gpipe

### Examples & Demos

**Note:** The workspace includes 25+ example packages demonstrating various features across different domains:

#### Async & Concurrency

- **[cancel](packages/async/examples/cancel)** - Async cancellation examples
- **[simulated](packages/async/examples/simulated)** - Simulated async examples

#### Database & Persistence

- **[turso_basic](packages/database/examples/turso_basic)** - Turso database basic usage
- **[turso_transactions](packages/database/examples/turso_transactions)** - Turso transaction handling

#### Schema & Migrations

- **[basic_usage](packages/switchy/schema/examples/basic_usage)** - Schema basic usage
- **[basic_migration_test](packages/switchy/schema/examples/basic_migration_test)** - Database migration testing
- **[static_migrations](packages/switchy/schema/examples/static_migrations)** - Static migrations
- **[borrowed_migrations](packages/switchy/schema/examples/borrowed_migrations)** - Borrowed migrations pattern
- **[mutation_migration_test](packages/switchy/schema/examples/mutation_migration_test)** - Mutation migrations
- **[state_migration_test](packages/switchy/schema/examples/state_migration_test)** - State migrations

#### HTTP & Web Server

- **[simple_get](packages/http/examples/simple_get)** - Simple HTTP GET example
- **[basic_handler](packages/web_server/examples/basic_handler)** - Basic web server handler example
- **[basic_handler_standalone](packages/web_server/examples/basic_handler_standalone)** - Standalone basic handler
- **[nested_get](packages/web_server/examples/nested_get)** - Nested GET routes
- **[openapi](packages/web_server/examples/openapi)** - OpenAPI integration
- **[from_request_test](packages/web_server/examples/from_request_test)** - Request extraction example
- **[json_extractor_standalone](packages/web_server/examples/json_extractor_standalone)** - JSON extraction
- **[query_extractor_standalone](packages/web_server/examples/query_extractor_standalone)** - Query parameter extraction
- **[combined_extractors_standalone](packages/web_server/examples/combined_extractors_standalone)** - Combined extractors
- **[handler_macro_test](packages/web_server/examples/handler_macro_test)** - Handler macro testing

#### Testing & Simulation

- **[api_testing](packages/simvar/examples/api_testing)** - API testing with simvar
- **[basic_web_server](packages/simvar/examples/basic_web_server)** - Basic web server with simvar

#### HyperChad UI

- **[basic_web_server](packages/hyperchad/renderer/html/web_server/examples/basic_web_server)** - HyperChad web server

#### Filesystem

- **[temp_dir](packages/fs/examples/temp_dir)** - Temporary directory usage

For a complete list of examples, see the workspace members in `Cargo.toml`.

</details>
