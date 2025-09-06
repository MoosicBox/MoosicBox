#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "admin-htmx")]
pub use moosicbox_admin_htmx as admin_htmx;
#[cfg(feature = "app-models")]
pub use moosicbox_app_models as app_models;
#[cfg(feature = "app-native-ui")]
pub use moosicbox_app_native_ui as app_native_ui;
#[cfg(feature = "arb")]
pub use moosicbox_arb as arb;
#[cfg(feature = "assert")]
pub use moosicbox_assert as assert;
#[cfg(feature = "async-service")]
pub use moosicbox_async_service as async_service;
#[cfg(feature = "audio-decoder")]
pub use moosicbox_audio_decoder as audio_decoder;
#[cfg(feature = "audio-encoder")]
pub use moosicbox_audio_encoder as audio_encoder;
#[cfg(feature = "audio-output")]
pub use moosicbox_audio_output as audio_output;
#[cfg(feature = "audio-zone")]
pub use moosicbox_audio_zone as audio_zone;
#[cfg(feature = "audio-zone-models")]
pub use moosicbox_audio_zone_models as audio_zone_models;
#[cfg(feature = "auth")]
pub use moosicbox_auth as auth;
#[cfg(feature = "channel-utils")]
pub use moosicbox_channel_utils as channel_utils;
#[cfg(feature = "config")]
pub use moosicbox_config as config;
#[cfg(feature = "downloader")]
pub use moosicbox_downloader as downloader;
#[cfg(feature = "env-utils")]
pub use moosicbox_env_utils as env_utils;
#[cfg(feature = "files")]
pub use moosicbox_files as files;
#[cfg(feature = "image")]
pub use moosicbox_image as image;
#[cfg(feature = "json-utils")]
pub use moosicbox_json_utils as json_utils;
#[cfg(feature = "library")]
pub use moosicbox_library as library;
#[cfg(feature = "library-models")]
pub use moosicbox_library_models as library_models;
#[cfg(feature = "load-balancer")]
pub use moosicbox_load_balancer as load_balancer;
#[cfg(feature = "logging")]
pub use moosicbox_logging as logging;
#[cfg(feature = "menu")]
pub use moosicbox_menu as menu;
#[cfg(feature = "middleware")]
pub use moosicbox_middleware as middleware;
#[cfg(feature = "music-api")]
pub use moosicbox_music_api as music_api;
#[cfg(feature = "paging")]
pub use moosicbox_paging as paging;
#[cfg(feature = "player")]
pub use moosicbox_player as player;
#[cfg(feature = "profiles")]
pub use moosicbox_profiles as profiles;
#[cfg(feature = "qobuz")]
pub use moosicbox_qobuz as qobuz;
#[cfg(feature = "remote-library")]
pub use moosicbox_remote_library as remote_library;
#[cfg(feature = "resampler")]
pub use moosicbox_resampler as resampler;
#[cfg(feature = "scan")]
pub use moosicbox_scan as scan;
#[cfg(feature = "schema")]
pub use moosicbox_schema as schema;
#[cfg(feature = "search")]
pub use moosicbox_search as search;
#[cfg(feature = "session")]
pub use moosicbox_session as session;
#[cfg(feature = "session-models")]
pub use moosicbox_session_models as session_models;
#[cfg(feature = "stream-utils")]
pub use moosicbox_stream_utils as stream_utils;
#[cfg(feature = "tidal")]
pub use moosicbox_tidal as tidal;
#[cfg(feature = "tunnel")]
pub use moosicbox_tunnel as tunnel;
#[cfg(feature = "tunnel-sender")]
pub use moosicbox_tunnel_sender as tunnel_sender;
#[cfg(feature = "ws")]
pub use moosicbox_ws as ws;
#[cfg(feature = "yt")]
pub use moosicbox_yt as yt;
