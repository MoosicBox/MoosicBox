//! Generates a unique hash based on enabled plugin features for the `HyperChad` Vanilla JS renderer.
//!
//! This crate computes a hash string from the set of enabled plugin features to ensure
//! that different feature builds produce distinct output directories and don't overwrite
//! each other. The hash is computed at compile time using the enabled feature flags.
//!
//! # Example
//!
//! ```rust
//! use hyperchad_renderer_vanilla_js_hash::PLUGIN_HASH_HEX;
//!
//! // Use the hash to construct a unique output path
//! let output_dir = format!("dist/vanilla-js-{}", PLUGIN_HASH_HEX);
//! println!("Output directory: {}", output_dir);
//! ```
//!
//! The hash value changes based on which plugin features are enabled at compile time,
//! ensuring that builds with different feature sets don't conflict.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use const_hex::{Buffer, const_encode};
use sha2_const_stable::Sha256;

#[cfg(feature = "plugin-idiomorph")]
const PLUGIN_IDIOMORPH_HASH: &str = "-idiomorph";
#[cfg(not(feature = "plugin-idiomorph"))]
const PLUGIN_IDIOMORPH_HASH: &str = "";

#[cfg(feature = "plugin-nav")]
const PLUGIN_NAV_HASH: &str = "-nav";
#[cfg(not(feature = "plugin-nav"))]
const PLUGIN_NAV_HASH: &str = "";

#[cfg(feature = "plugin-sse")]
const PLUGIN_SSE_HASH: &str = "-sse";
#[cfg(not(feature = "plugin-sse"))]
const PLUGIN_SSE_HASH: &str = "";

#[cfg(feature = "plugin-tauri-event")]
const PLUGIN_TAURI_EVENT_HASH: &str = "-tauri-event";
#[cfg(not(feature = "plugin-tauri-event"))]
const PLUGIN_TAURI_EVENT_HASH: &str = "";

#[cfg(all(not(feature = "plugin-uuid-insecure"), feature = "plugin-uuid"))]
const PLUGIN_UUID_HASH: &str = "-uuid";
#[cfg(not(all(not(feature = "plugin-uuid-insecure"), feature = "plugin-uuid")))]
const PLUGIN_UUID_HASH: &str = "";

#[cfg(feature = "plugin-uuid-insecure")]
const PLUGIN_UUID_INSECURE_HASH: &str = "-uuid-insecure";
#[cfg(not(feature = "plugin-uuid-insecure"))]
const PLUGIN_UUID_INSECURE_HASH: &str = "";

#[cfg(feature = "plugin-routing")]
const PLUGIN_ROUTING_HASH: &str = "-routing";
#[cfg(not(feature = "plugin-routing"))]
const PLUGIN_ROUTING_HASH: &str = "";

#[cfg(feature = "plugin-event")]
const PLUGIN_EVENT_HASH: &str = "-event";
#[cfg(not(feature = "plugin-event"))]
const PLUGIN_EVENT_HASH: &str = "";

#[cfg(feature = "plugin-canvas")]
const PLUGIN_CANVAS_HASH: &str = "-canvas";
#[cfg(not(feature = "plugin-canvas"))]
const PLUGIN_CANVAS_HASH: &str = "";

#[cfg(feature = "plugin-form")]
const PLUGIN_FORM_HASH: &str = "-form";
#[cfg(not(feature = "plugin-form"))]
const PLUGIN_FORM_HASH: &str = "";

#[cfg(feature = "plugin-http-events")]
const PLUGIN_HTTP_EVENTS_HASH: &str = "-http-events";
#[cfg(not(feature = "plugin-http-events"))]
const PLUGIN_HTTP_EVENTS_HASH: &str = "";

#[cfg(feature = "plugin-actions-change")]
const PLUGIN_ACTIONS_CHANGE_HASH: &str = "-actions-change";
#[cfg(not(feature = "plugin-actions-change"))]
const PLUGIN_ACTIONS_CHANGE_HASH: &str = "";

#[cfg(feature = "plugin-actions-click")]
const PLUGIN_ACTIONS_CLICK_HASH: &str = "-actions-click";
#[cfg(not(feature = "plugin-actions-click"))]
const PLUGIN_ACTIONS_CLICK_HASH: &str = "";

#[cfg(feature = "plugin-actions-click-outside")]
const PLUGIN_ACTIONS_CLICK_OUTSIDE_HASH: &str = "-actions-click-outside";
#[cfg(not(feature = "plugin-actions-click-outside"))]
const PLUGIN_ACTIONS_CLICK_OUTSIDE_HASH: &str = "";

#[cfg(feature = "plugin-actions-event")]
const PLUGIN_ACTIONS_EVENT_HASH: &str = "-actions-event";
#[cfg(not(feature = "plugin-actions-event"))]
const PLUGIN_ACTIONS_EVENT_HASH: &str = "";

#[cfg(feature = "plugin-actions-event-key-down")]
const PLUGIN_ACTIONS_EVENT_KEY_DOWN_HASH: &str = "-actions-event-key-down";
#[cfg(not(feature = "plugin-actions-event-key-down"))]
const PLUGIN_ACTIONS_EVENT_KEY_DOWN_HASH: &str = "";

#[cfg(feature = "plugin-actions-event-key-up")]
const PLUGIN_ACTIONS_EVENT_KEY_UP_HASH: &str = "-actions-event-key-up";
#[cfg(not(feature = "plugin-actions-event-key-up"))]
const PLUGIN_ACTIONS_EVENT_KEY_UP_HASH: &str = "";

#[cfg(feature = "plugin-actions-immediate")]
const PLUGIN_ACTIONS_IMMEDIATE_HASH: &str = "-actions-immediate";
#[cfg(not(feature = "plugin-actions-immediate"))]
const PLUGIN_ACTIONS_IMMEDIATE_HASH: &str = "";

#[cfg(feature = "plugin-actions-mouse-down")]
const PLUGIN_ACTIONS_MOUSE_DOWN_HASH: &str = "-actions-mouse-down";
#[cfg(not(feature = "plugin-actions-mouse-down"))]
const PLUGIN_ACTIONS_MOUSE_DOWN_HASH: &str = "";

#[cfg(feature = "plugin-actions-key-down")]
const PLUGIN_ACTIONS_KEY_DOWN_HASH: &str = "-actions-key-down";
#[cfg(not(feature = "plugin-actions-key-down"))]
const PLUGIN_ACTIONS_KEY_DOWN_HASH: &str = "";

#[cfg(feature = "plugin-actions-key-up")]
const PLUGIN_ACTIONS_KEY_UP_HASH: &str = "-actions-key-up";
#[cfg(not(feature = "plugin-actions-key-up"))]
const PLUGIN_ACTIONS_KEY_UP_HASH: &str = "";

#[cfg(feature = "plugin-actions-mouse-over")]
const PLUGIN_ACTIONS_MOUSE_OVER_HASH: &str = "-actions-mouse-over";
#[cfg(not(feature = "plugin-actions-mouse-over"))]
const PLUGIN_ACTIONS_MOUSE_OVER_HASH: &str = "";

#[cfg(feature = "plugin-actions-resize")]
const PLUGIN_ACTIONS_RESIZE_HASH: &str = "-actions-resize";
#[cfg(not(feature = "plugin-actions-resize"))]
const PLUGIN_ACTIONS_RESIZE_HASH: &str = "";

/// Concatenated string of all enabled plugin feature names.
///
/// This string is built at compile time by concatenating the names of all enabled
/// plugin features. It serves as the input for generating the hash that uniquely
/// identifies this particular feature configuration.
pub const PLUGIN_HASH: &str = const_format::concatcp!(
    "plugins",
    PLUGIN_IDIOMORPH_HASH,
    PLUGIN_NAV_HASH,
    PLUGIN_SSE_HASH,
    PLUGIN_TAURI_EVENT_HASH,
    PLUGIN_UUID_HASH,
    PLUGIN_UUID_INSECURE_HASH,
    PLUGIN_ROUTING_HASH,
    PLUGIN_EVENT_HASH,
    PLUGIN_ACTIONS_CHANGE_HASH,
    PLUGIN_ACTIONS_CLICK_HASH,
    PLUGIN_ACTIONS_CLICK_OUTSIDE_HASH,
    PLUGIN_ACTIONS_EVENT_HASH,
    PLUGIN_ACTIONS_EVENT_KEY_DOWN_HASH,
    PLUGIN_ACTIONS_EVENT_KEY_UP_HASH,
    PLUGIN_ACTIONS_IMMEDIATE_HASH,
    PLUGIN_ACTIONS_MOUSE_DOWN_HASH,
    PLUGIN_ACTIONS_MOUSE_OVER_HASH,
    PLUGIN_ACTIONS_KEY_DOWN_HASH,
    PLUGIN_ACTIONS_KEY_UP_HASH,
    PLUGIN_ACTIONS_RESIZE_HASH,
    PLUGIN_CANVAS_HASH,
    PLUGIN_FORM_HASH,
    PLUGIN_HTTP_EVENTS_HASH,
);

/// SHA-256 hash of the plugin feature string as raw bytes.
///
/// This is the raw 32-byte SHA-256 digest computed from [`PLUGIN_HASH`].
/// It uniquely identifies the current feature configuration at compile time.
pub const RAW_HASH: [u8; Sha256::DIGEST_SIZE] =
    Sha256::new().update(PLUGIN_HASH.as_bytes()).finalize();

/// Hexadecimal encoding buffer for the raw hash.
///
/// This buffer stores the hexadecimal representation of [`RAW_HASH`].
pub const HEX_BUF: Buffer<{ Sha256::DIGEST_SIZE }> = const_encode(&RAW_HASH);

/// Hexadecimal string representation of the plugin configuration hash.
///
/// This is the primary export of this crate - a compile-time constant string
/// containing the hexadecimal encoding of the SHA-256 hash of all enabled plugin
/// features. Use this value to construct unique paths or identifiers for different
/// feature builds to prevent conflicts.
///
/// # Example
///
/// ```rust
/// use hyperchad_renderer_vanilla_js_hash::PLUGIN_HASH_HEX;
///
/// // Create a unique output directory name based on enabled features
/// let build_dir = format!("target/renderer-{}", PLUGIN_HASH_HEX);
///
/// // Or use it as a cache key
/// let cache_key = format!("hyperchad-js-cache-{}", PLUGIN_HASH_HEX);
/// ```
pub const PLUGIN_HASH_HEX: &str = HEX_BUF.as_str();
