//! `MoosicBox` unified package.
//!
//! This crate serves as a facade that re-exports all `MoosicBox` components based on feature flags.
//! Each module corresponds to a separate `MoosicBox` crate that can be enabled via Cargo features.
//!
//! # Features
//!
//! * `all` - Enable all available components
//! * `all-default` - Enable all components with their default features
//! * `all-sources` - Enable all streaming sources (Qobuz, Tidal, `YouTube`)
//!
//! Individual components can be enabled through their respective feature flags (e.g., `player`, `auth`, `library`).

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// HTMX-based admin interface for managing `MoosicBox` server configuration.
#[cfg(feature = "admin-htmx")]
pub use moosicbox_admin_htmx as admin_htmx;
/// Data models for application configuration including connections and music API integrations.
#[cfg(feature = "app-models")]
pub use moosicbox_app_models as app_models;
/// Native desktop application providing UI rendering and playback visualization.
#[cfg(feature = "app-native-ui")]
pub use moosicbox_app_native_ui as app_native_ui;
/// Arbitrary value generators for property-based testing.
#[cfg(feature = "arb")]
pub use moosicbox_arb as arb;
/// Environment-controlled assertion macros for conditional debugging.
#[cfg(feature = "assert")]
pub use moosicbox_assert as assert;
/// Asynchronous service management framework with command processing.
#[cfg(feature = "async-service")]
pub use moosicbox_async_service as async_service;
/// Audio decoding using Symphonia with streaming capabilities.
#[cfg(feature = "audio-decoder")]
pub use moosicbox_audio_decoder as audio_decoder;
/// Audio encoding for AAC, FLAC, MP3, and Opus formats.
#[cfg(feature = "audio-encoder")]
pub use moosicbox_audio_encoder as audio_encoder;
/// Audio output management with automatic resampling support.
#[cfg(feature = "audio-output")]
pub use moosicbox_audio_output as audio_output;
/// Audio zone management for logical groupings of audio players.
#[cfg(feature = "audio-zone")]
pub use moosicbox_audio_zone as audio_zone;
/// Data models for audio zone management with synchronized playback.
#[cfg(feature = "audio-zone-models")]
pub use moosicbox_audio_zone_models as audio_zone_models;
/// Authentication and authorization using access tokens and magic tokens.
#[cfg(feature = "auth")]
pub use moosicbox_auth as auth;
/// Channel utilities with prioritization support for message passing.
#[cfg(feature = "channel-utils")]
pub use moosicbox_channel_utils as channel_utils;
/// Configuration management including directory paths and profile management.
#[cfg(feature = "config")]
pub use moosicbox_config as config;
/// Music download management for tracks, album covers, and artist covers.
#[cfg(feature = "downloader")]
pub use moosicbox_downloader as downloader;
/// Compile-time environment variable parsing utilities.
#[cfg(feature = "env-utils")]
pub use moosicbox_env_utils as env_utils;
/// File handling utilities including HTTP file operations and media covers.
#[cfg(feature = "files")]
pub use moosicbox_files as files;
/// Image resizing and format conversion with high-performance backends.
#[cfg(feature = "image")]
pub use moosicbox_image as image;
/// Utilities for converting JSON and database values to Rust types.
#[cfg(feature = "json-utils")]
pub use moosicbox_json_utils as json_utils;
/// Music library management functionality for accessing artists, albums, and tracks.
#[cfg(feature = "library")]
pub use moosicbox_library as library;
/// Data models for the local music library representing artists, albums, and tracks.
#[cfg(feature = "library-models")]
pub use moosicbox_library_models as library_models;
/// HTTP/HTTPS load balancer built on Pingora for routing requests.
#[cfg(feature = "load-balancer")]
pub use moosicbox_load_balancer as load_balancer;
/// Logging utilities with feature-gated modules.
#[cfg(feature = "logging")]
pub use moosicbox_logging as logging;
/// Menu functionality for managing music library content.
#[cfg(feature = "menu")]
pub use moosicbox_menu as menu;
/// Actix-web middleware components and configuration utilities.
#[cfg(feature = "middleware")]
pub use moosicbox_middleware as middleware;
/// Music API abstraction layer providing a unified interface for multiple sources.
#[cfg(feature = "music-api")]
pub use moosicbox_music_api as music_api;
/// Pagination utilities for total-based and cursor-based pagination.
#[cfg(feature = "paging")]
pub use moosicbox_paging as paging;
/// Audio playback engine handling decoding, streaming, and playback control.
#[cfg(feature = "player")]
pub use moosicbox_player as player;
/// Profile management with global registry for user profiles.
#[cfg(feature = "profiles")]
pub use moosicbox_profiles as profiles;
/// Qobuz music streaming service integration implementing `MusicApi` trait.
#[cfg(feature = "qobuz")]
pub use moosicbox_qobuz as qobuz;
/// Remote `MoosicBox` server music library API client.
#[cfg(feature = "remote-library")]
pub use moosicbox_remote_library as remote_library;
/// Audio resampling for converting between sample rates using FFT-based algorithms.
#[cfg(feature = "resampler")]
pub use moosicbox_resampler as resampler;
/// Music library scanning and indexing from local filesystem and remote services.
#[cfg(feature = "scan")]
pub use moosicbox_scan as scan;
/// Database schema migration management for `PostgreSQL` and `SQLite`.
#[cfg(feature = "schema")]
pub use moosicbox_schema as schema;
/// Full-text search implementation built on Tantivy for music library indexing.
#[cfg(feature = "search")]
pub use moosicbox_search as search;
/// Session and connection management for playback sessions and audio zones.
#[cfg(feature = "session")]
pub use moosicbox_session as session;
/// Data models for playback sessions, connections, and player registration.
#[cfg(feature = "session-models")]
pub use moosicbox_session_models as session_models;
/// Utilities for broadcasting and streaming data to multiple consumers.
#[cfg(feature = "stream-utils")]
pub use moosicbox_stream_utils as stream_utils;
/// Tidal music streaming service integration with API client.
#[cfg(feature = "tidal")]
pub use moosicbox_tidal as tidal;
/// Tunneling protocol for HTTP and WebSocket requests over persistent connections.
#[cfg(feature = "tunnel")]
pub use moosicbox_tunnel as tunnel;
/// Tunnel sender implementation for client-side tunnel connectivity.
#[cfg(feature = "tunnel-sender")]
pub use moosicbox_tunnel_sender as tunnel_sender;
/// WebSocket message handling for real-time communication.
#[cfg(feature = "ws")]
pub use moosicbox_ws as ws;
/// `YouTube` Music API client implementing `MusicApi` trait.
#[cfg(feature = "yt")]
pub use moosicbox_yt as yt;
