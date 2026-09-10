# Package catalog

Selected package and example directories. This is a navigation aid, not a count of published or production-ready libraries. Check each manifest for workspace membership, features, and platform dependencies. See the [root README](../README.md) for product setup.

### Core Application

- **[moosicbox](moosicbox)** - Main MoosicBox server application
- **[moosicbox_server](server)** - Core server implementation and HTTP handlers
- **[moosicbox_server_simulator](server/simulator)** - Server simulation utilities for testing

### Audio & Media

- **[moosicbox_audio_decoder](audio_decoder)** - Audio decoding with support for multiple formats
- **[moosicbox_audio_encoder](audio_encoder)** - Audio encoding utilities with feature-gated support
- **[moosicbox_audio_output](audio_output)** - Multi-platform audio output management
- **[moosicbox_audio_zone](audio_zone)** - Audio zone database management with CRUD operations
- **[moosicbox_audio_zone_models](audio_zone/models)** - Data models for audio zone management
- **[moosicbox_player](player)** - High-performance audio player with playback controls
- **[moosicbox_resampler](resampler)** - Audio resampling utilities for format conversion
- **[moosicbox_downloader](downloader)** - Media downloading and caching system
- **[moosicbox_opus](opus)** - Opus codec integration
- **[moosicbox_opus_native](opus_native)** - Native Opus implementation and validation tools
- **[moosicbox_opus_native_libopus](opus_native/libopus)** - LibOpus native library
- **[moosicbox_opus_native_test_vectors](opus_native/test_vectors)** - Test vectors for Opus decoder validation
- **[aconverter](aconverter)** - Audio converter helper binary

### Music Services Integration

- **[moosicbox_tidal](tidal)** - Tidal music service integration with comprehensive API
- **[moosicbox_qobuz](qobuz)** - Qobuz hi-fi music service integration
- **[moosicbox_yt](yt)** - YouTube Music API integration

### Library & Search

- **[moosicbox_library](library)** - Music library management and database operations
- **[moosicbox_library_models](library/models)** - Core data models for music library
- **[moosicbox_library_music_api](library/music_api)** - Music API integration for library
- **[moosicbox_remote_library](remote_library)** - HTTP client for remote music servers
- **[moosicbox_search](search)** - High-performance full-text search engine using Tantivy
- **[moosicbox_scan](scan)** - Library scanning and metadata extraction
- **[moosicbox_scan_models](scan/models)** - Data models for library scanning

### Music API & Models

- **[moosicbox_music_api](music_api)** - Unified music API with service integrations
- **[moosicbox_music_api_api](music_api/api)** - Core API implementations and endpoints
- **[moosicbox_music_api_helpers](music_api/helpers)** - Helper utilities for music APIs
- **[moosicbox_music_api_models](music_api/models)** - Data models for music API
- **[moosicbox_music_models](music/models)** - Core music data models and types

### Application Framework

- **[moosicbox_app_models](app/models)** - Application data models and structures
- **[moosicbox_app_state](app/state)** - Application state management system
- **[moosicbox_app_native](app/native)** - Native application components
- **[moosicbox_app_native_bundled](app/native/bundled)** - Bundled native app service
- **[moosicbox_app_native_image](app/native/image)** - Image asset management for native apps
- **[moosicbox_app_native_ui](app/native/ui)** - Native UI components and widgets

### Desktop Application (Tauri)

- **[moosicbox_app_tauri_bundled](app/tauri/bundled)** - Bundled Tauri application
- **[moosicbox_app_client](app/tauri/client)** - Tauri client utilities and bindings
- **[tauri_create_config](app/tauri/create_config)** - Tauri configuration generator
- **[moosicbox](app/tauri/src-tauri)** - MoosicBox Tauri desktop application
- **[app-tauri-plugin-player](app/tauri/tauri-plugin-player)** - Tauri plugin for audio player
- **[moosicbox_app_ws](app/tauri/ws)** - Tauri WebSocket integration

### HyperChad UI Framework

- **[hyperchad](hyperchad)** - Core HyperChad UI framework
- **[hyperchad_actions](hyperchad/actions)** - Action system for HyperChad
- **[hyperchad_app](hyperchad/app)** - HyperChad application framework
- **[hyperchad_color](hyperchad/color)** - Color utilities and theming
- **[hyperchad_js_bundler](hyperchad/js_bundler)** - JavaScript bundling for HyperChad
- **[hyperchad_markdown](hyperchad/markdown)** - Markdown to HyperChad Container conversion with GitHub Flavored Markdown support
- **[hyperchad_router](hyperchad/router)** - Client-side routing system
- **[hyperchad_state](hyperchad/state)** - State management for HyperChad
- **[hyperchad_template](hyperchad/template)** - Template system and DSL
- **[hyperchad_template_actions_dsl](hyperchad/template/actions_dsl)** - DSL for template actions
- **[hyperchad_template_macros](hyperchad/template/macros)** - Template system macros
- **[hyperchad_transformer](hyperchad/transformer)** - UI transformation system
- **[hyperchad_transformer_models](hyperchad/transformer/models)** - Models for UI transformations

### HyperChad Renderers

- **[hyperchad_renderer](hyperchad/renderer)** - Core rendering abstractions
- **[hyperchad_renderer_egui](hyperchad/renderer/egui)** - Native desktop renderer using egui
- **[hyperchad_renderer_fltk](hyperchad/renderer/fltk)** - Cross-platform native GUI renderer using FLTK
- **[hyperchad_renderer_html](hyperchad/renderer/html)** - Server-side HTML renderer
- **[hyperchad_renderer_html_actix](hyperchad/renderer/html/actix)** - Actix Web integration for HTML renderer
- **[hyperchad_renderer_html_cdn](hyperchad/renderer/html/cdn)** - CDN integration for HTML renderer
- **[hyperchad_renderer_html_http](hyperchad/renderer/html/http)** - Generic HTTP server integration
- **[hyperchad_renderer_html_lambda](hyperchad/renderer/html/lambda)** - AWS Lambda integration for serverless deployment
- **[hyperchad_renderer_html_web_server](hyperchad/renderer/html/web_server)** - Web server utilities for HTML renderer
- **[hyperchad_renderer_vanilla_js](hyperchad/renderer/vanilla_js)** - Client-side JavaScript renderer
- **[hyperchad_renderer_vanilla_js_hash](hyperchad/renderer/vanilla_js/hash)** - Content-based hash generation for cache busting

### Web & Networking

- **[switchy_web_server](switchy/web_server)** - Web server abstraction and utilities
- **[switchy_web_server_actix](switchy/web_server/actix)** - Actix Web backend for switchy_web_server
- **[switchy_web_server_core](switchy/web_server/core)** - Core web server functionality
- **[switchy_web_server_cors](switchy/web_server/cors)** - CORS middleware for web servers
- **[switchy_http](switchy/http)** - Generic HTTP client abstraction
- **[switchy_http_models](switchy/http/models)** - HTTP protocol models and types
- **[moosicbox_ws](ws)** - WebSocket utilities and abstractions
- **[moosicbox_middleware](middleware)** - HTTP middleware collection

### Networking & Discovery

- **[moosicbox_tunnel](tunnel)** - Tunneling utilities and protocols
- **[moosicbox_tunnel_sender](tunnel_sender)** - WebSocket-based tunneling client
- **[moosicbox_tunnel_server](tunnel_server)** - WebSocket-based tunneling server
- **[moosicbox_upnp](upnp)** - UPnP player integration and controls
- **[moosicbox_mdns](mdns)** - mDNS service discovery and scanning
- **[switchy_tcp](switchy/tcp)** - Generic TCP networking abstraction
- **[switchy_upnp](switchy/upnp)** - UPnP device discovery and communication
- **[switchy_mdns](switchy/mdns)** - mDNS service registration and discovery
- **[switchy_p2p](switchy/p2p)** - Peer-to-peer networking utilities
- **[moosicbox_load_balancer](load_balancer)** - Load balancing utilities
- **[openport](openport)** - Find free unused network ports

### Authentication & Security

- **[moosicbox_auth](auth)** - Authentication utilities and client registration
- **[moosicbox_profiles](profiles)** - User profile management and validation
- **[moosicbox_session](session)** - Session management utilities
- **[moosicbox_session_models](session/models)** - Data models for session management

### Database & Storage

- **[switchy_database](switchy/database)** - Database abstraction layer
- **[switchy_database_connection](switchy/database_connection)** - Database connection management
- **[switchy_schema](switchy/schema)** - Database schema and migration framework
- **[switchy_schema_cli](switchy/schema/cli)** - CLI tool for schema migrations
- **[moosicbox_schema](schema)** - Database migration system
- **[switchy_fs](switchy/fs)** - Cross-platform filesystem abstraction

### Utilities & Infrastructure

- **[moosicbox_config](config)** - Configuration utilities for applications
- **[switchy_env](switchy/env)** - Environment configuration utilities
- **[moosicbox_env_utils](env_utils)** - Environment variable parsing utilities
- **[moosicbox_logging](logging)** - Logging utilities with feature-gated modules
- **[moosicbox_log_runtime](log_runtime)** - Generic log runtime paths and initialization
- **[moosicbox_log_watch](log_watch)** - Generic log watching, filtering, and optional TUI
- **[switchy_telemetry](switchy/telemetry)** - OpenTelemetry integration for distributed tracing
- **[moosicbox_async_service](async_service)** - Service framework for async applications
- **[switchy_time](switchy/time)** - Time abstraction utilities
- **[switchy_random](switchy/random)** - Random number generation utilities
- **[switchy_uuid](switchy/uuid)** - UUID generation and handling utilities

### Data Processing & Parsing

- **[moosicbox_json_utils](json_utils)** - JSON parsing utilities and helpers
- **[moosicbox_parsing_utils](parsing_utils)** - Utilities for parsing integer sequences and ranges
- **[moosicbox_date_utils](date_utils)** - Date parsing and manipulation utilities
- **[moosicbox_paging](paging)** - Pagination utilities for data sets
- **[moosicbox_stream_utils](stream_utils)** - Byte stream utilities with ByteWriter/ByteStream
- **[moosicbox_channel_utils](channel_utils)** - Channel utilities for async communication

### Media & Image Processing

- **[moosicbox_image](image)** - Image processing and optimization utilities
- **[moosicbox_files](files)** - File handling and streaming utilities

### UI & Interface

- **[moosicbox_menu](menu)** - Menu system utilities
- **[moosicbox_menu_models](menu/models)** - Data models for menu system
- **[moosicbox_admin_htmx](admin_htmx)** - HTMX API endpoints for administrative operations
- **[moosicbox_marketing_site](marketing_site)** - Marketing website implementation
- **[moosicbox_marketing_site_ui](marketing_site/ui)** - UI components for marketing site

### Development & Testing

- **[simvar](simvar)** - Deterministic simulation facade
- **[simvar_harness](simvar/harness)** - Seeded workload orchestration and restartable simulation hosts
- **[simvar_utils](simvar/utils)** - Simulation utilities and helpers
- **[moosicbox_arb](arb)** - Arbitrary data generation for testing
- **[moosicbox_clippier](clippier)** - Workspace analysis and CI generation tool
- **[hyperchad_simulator](hyperchad/simulator)** - HyperChad simulation utilities
- **[hyperchad_test_utils](hyperchad/test_utils)** - HyperChad testing utilities
- **[switchy_schema_test_utils](switchy/schema/test_utils)** - Schema testing utilities
- **[switchy_web_server_simulator](switchy/web_server/simulator)** - Web server simulation utilities
- **[bloaty](bloaty)** - Binary analysis utilities (placeholder)

### Core Libraries

- **[switchy](switchy)** - Feature-gated re-exports for cross-platform compatibility
- **[switchy_async](switchy/async)** - Async runtime abstraction
- **[switchy_async_cargo](switchy/async/cargo)** - Cargo integration for async runtime
- **[switchy_async_macros](switchy/async/macros)** - Macros for async runtime
- **[moosicbox_assert](assert)** - Conditional assertion macros

### Transpiler & Code Generation

- **[gpipe](gpipe)** - General-purpose transpiler framework
- **[gpipe_ast](gpipe/ast)** - Abstract syntax tree for gpipe

### Examples & Demos

**Note:** The workspace includes 25+ example packages demonstrating various features across different domains:

#### Async & Concurrency

- **[cancel](switchy/async/examples/cancel)** - Async cancellation examples
- **[simulated](switchy/async/examples/simulated)** - Simulated async examples

#### Database & Persistence

- **[turso_basic](switchy/database/examples/turso_basic)** - Turso database basic usage
- **[turso_transactions](switchy/database/examples/turso_transactions)** - Turso transaction handling

#### Schema & Migrations

- **[basic_usage](switchy/schema/examples/basic_usage)** - Schema basic usage
- **[basic_migration_test](switchy/schema/examples/basic_migration_test)** - Database migration testing
- **[static_migrations](switchy/schema/examples/static_migrations)** - Static migrations
- **[borrowed_migrations](switchy/schema/examples/borrowed_migrations)** - Borrowed migrations pattern
- **[mutation_migration_test](switchy/schema/examples/mutation_migration_test)** - Mutation migrations
- **[state_migration_test](switchy/schema/examples/state_migration_test)** - State migrations

#### HTTP & Web Server

- **[simple_get](switchy/http/examples/simple_get)** - Simple HTTP GET example
- **[simple_get](switchy/web_server/examples/simple_get)** - Simple web server GET example
- **[basic_handler](switchy/web_server/examples/basic_handler)** - Basic web server handler example
- **[basic_handler_standalone](switchy/web_server/examples/basic_handler_standalone)** - Standalone basic handler
- **[nested_get](switchy/web_server/examples/nested_get)** - Nested GET routes
- **[openapi](switchy/web_server/examples/openapi)** - OpenAPI integration
- **[from_request_test](switchy/web_server/examples/from_request_test)** - Request extraction example
- **[json_extractor_standalone](switchy/web_server/examples/json_extractor_standalone)** - JSON extraction
- **[query_extractor_standalone](switchy/web_server/examples/query_extractor_standalone)** - Query parameter extraction
- **[combined_extractors_standalone](switchy/web_server/examples/combined_extractors_standalone)** - Combined extractors
- **[handler_macro_test](switchy/web_server/examples/handler_macro_test)** - Handler macro testing

#### Testing & Simulation

- **[api_testing](simvar/examples/api_testing)** - API testing with simvar
- **[basic_web_server](simvar/examples/basic_web_server)** - Basic web server with simvar

#### HyperChad UI

- **[details_summary](hyperchad/examples/details_summary)** - Details/summary collapsible elements
- **[http_events](hyperchad/examples/http_events)** - HTTP request lifecycle events
- **[markdown](hyperchad/examples/markdown)** - Markdown rendering with GitHub Flavored Markdown
- **[select_dropdown](hyperchad/examples/select_dropdown)** - Select/option dropdown elements
- **[basic_web_server](hyperchad/renderer/html/web_server/examples/basic_web_server)** - HyperChad web server

#### Filesystem

- **[temp_dir](switchy/fs/examples/temp_dir)** - Temporary directory usage

For a complete list of examples, see the workspace members in [`Cargo.toml`](../Cargo.toml).
