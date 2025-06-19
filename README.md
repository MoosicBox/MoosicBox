# MoosicBox Server

A music server for cows

![MoosicBox](https://github.com/MoosicBox/Files/blob/master/animation.gif?raw=true)

## Project Status

**⚠️ Development Notice:** This is currently a personal side project in active development. Most features are experimental and may not work as expected. While the goal is to eventually create a stable, user-friendly music app that others can enjoy, the project is not yet ready for general use. Your mileage may vary significantly at this stage.

## Features

Implemented:

- Audio playback controls
    - Next/previous track, seek track, queue tracks, adjust volume, etc
- Control playback across applications (web and desktop)
    - Supports multi simultaneous audio outputs
- Audio encoding on the fly
    - AAC (m4a, mp4), mp3, Opus
- Hi-Fi audio player
- Automatic image optimization for requested size on demand
- Tunnel server reverse proxy - allows access to local server from internet without any firewall configuration
- Tidal and Qobuz integration
- No internet connection required, ever.
- Global search functionality
- Postgres, MySQL, and SQLite database support
- Android app
- Audio file visualization on seek bar

To-do:

See the [prioritized backlog](https://github.com/orgs/MoosicBox/projects/1/views/1)

## Workspace Packages

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
- **[hyperchad_renderer_html_http](packages/hyperchad/renderer/html/http)** - Generic HTTP server integration
- **[hyperchad_renderer_html_lambda](packages/hyperchad/renderer/html/lambda)** - AWS Lambda integration for serverless deployment
- **[hyperchad_renderer_vanilla_js](packages/hyperchad/renderer/vanilla_js)** - Client-side JavaScript renderer
- **[hyperchad_renderer_vanilla_js_hash](packages/hyperchad/renderer/vanilla_js/hash)** - Content-based hash generation for cache busting
- **[@hyperchad/vanilla-js](packages/hyperchad/renderer/vanilla_js/web)** - Client-side JavaScript/TypeScript library for browser runtime

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
- **[moosicbox_schema](packages/schema)** - Database migration system
- **[switchy_fs](packages/fs)** - Cross-platform filesystem abstraction

### Utilities & Infrastructure

- **[moosicbox_config](packages/config)** - Configuration utilities for applications
- **[moosicbox_env_utils](packages/env_utils)** - Environment variable parsing utilities
- **[moosicbox_logging](packages/logging)** - Logging utilities with feature-gated modules
- **[switchy_telemetry](packages/telemetry)** - OpenTelemetry integration for distributed tracing
- **[moosicbox_async_service](packages/async_service)** - Service framework for async applications
- **[moosicbox_task](packages/task)** - Task spawning utilities and abstractions
- **[switchy_time](packages/time)** - Time abstraction utilities
- **[switchy_random](packages/random)** - Random number generation utilities

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
- **[bloaty](packages/bloaty)** - Binary analysis utilities (placeholder)

### Core Libraries

- **[switchy](packages/switchy)** - Feature-gated re-exports for cross-platform compatibility
- **[switchy_async](packages/async)** - Async runtime abstraction
- **[switchy_async_cargo](packages/async/cargo)** - Cargo integration for async runtime
- **[switchy_async_macros](packages/async/macros)** - Macros for async runtime
- **[moosicbox_assert](packages/assert)** - Conditional assertion macros

### Examples & Demos

- **[cancel](packages/async/examples/cancel)** - Async cancellation examples
- **[simulated](packages/async/examples/simulated)** - Simulated async examples
- **[simple_get](packages/http/examples/simple_get)** - Simple HTTP GET example
- **[nested_get](packages/web_server/examples/nested_get)** - Nested GET endpoint example
- **[openapi](packages/web_server/examples/openapi)** - OpenAPI integration example
- **[simple_get](packages/web_server/examples/simple_get)** - Simple web server example

</details>

## Local Server

### Dependencies

- pkg-config (optional for OPUS)
- libtool (optional for OPUS)
- [vips](https://www.libvips.org/install.html) (optional for libvips image optimization)

### Run

`cargo server 8001`

### Debug

`RUST_BACKTRACE=1 RUST_LOG="moosicbox=debug" cargo server:debug 8001`

### Deploy

`WS_HOST="wss://tunnel2.moosicbox.com/ws" TUNNEL_ACCESS_TOKEN='your access token here' STATIC_TOKEN='your static token here' ./do-deploy.sh moosicbox-tunnel-server`

## Tunnel Server

### Run

`TUNNEL_ACCESS_TOKEN='your access token here' cargo tunnel-server 8005`

### Development

`TUNNEL_ACCESS_TOKEN='your access token here' RUST_BACKTRACE=1 RUST_LOG="moosicbox=debug" cargo tunnel-server:debug 8005`

### Deploy

`TUNNEL_ACCESS_TOKEN='your access token here' AWS_ACCESS_KEY_ID='token here' AWS_SECRET_ACCESS_KEY='key here' ./do-deploy.sh moosicbox-tunnel-server`

## Database

### Server

The SQLite database stores the music library data:

- Artist metadata
- Album metadata
- Track metadata
- Local WebSocket connection metadata
- Audio Player configurations
- Playback Sessions

#### Migrations

##### SQLite

###### Run

`diesel migration run --migration-dir migrations/server/sqlite --database-url library.db`

###### Revert

`diesel migration revert --migration-dir migrations/server/sqlite --database-url library.db`

###### New Migration

`diesel migration generate --migration-dir migrations/server/sqlite migration_name`

##### Postgres

###### Run

`diesel migration run --migration-dir migrations/server/postgres --database-url postgres://username:password@host/dbname`

###### Revert

`diesel migration revert --migration-dir migrations/server/postgres --database-url postgres://username:password@host/dbname`

###### New Migration

`diesel migration generate --migration-dir migrations/server/postgres migration_name`

### Tunnel

#### Postgres

The Postgres database stores the tunnel server configurations:

- WebSocket connection mappings
    - Enables the tunnel server to know which WebSocket connection to tunnel data from

##### Migrations

###### Run

`diesel migration run --migration-dir migrations/tunnel/postgres --database-url postgres://username:password@host/dbname`

###### Revert

`diesel migration revert --migration-dir migrations/tunnel/postgres --database-url postgres://username:password@host/dbname`

###### New Migration

`diesel migration generate --migration-dir migrations/tunnel/postgres migration_name`

#### MySQL

The MySQL database stores the tunnel server configurations:

- WebSocket connection mappings
    - Enables the tunnel server to know which WebSocket connection to tunnel data from

##### Migrations

###### Run

`diesel migration run --migration-dir migrations/tunnel/mysql --database-url mysql://username:password@host/dbname`

###### Revert

`diesel migration revert --migration-dir migrations/tunnel/mysql --database-url mysql://username:password@host/dbname`

###### New Migration

`diesel migration generate --migration-dir migrations/tunnel/mysql migration_name`
