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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_hash_starts_with_plugins() {
        // PLUGIN_HASH should always start with "plugins" regardless of enabled features
        assert!(
            PLUGIN_HASH.starts_with("plugins"),
            "PLUGIN_HASH should start with 'plugins', got: {PLUGIN_HASH}",
        );
    }

    #[test]
    fn test_plugin_hash_is_not_empty() {
        // PLUGIN_HASH should never be empty - at minimum it contains "plugins"
        const PLUGIN_MIN_LEN: usize = "plugins".len();
        assert!(
            PLUGIN_HASH.len() >= PLUGIN_MIN_LEN,
            "PLUGIN_HASH should be at least as long as 'plugins'"
        );
    }

    #[test]
    fn test_raw_hash_has_correct_size() {
        // SHA-256 produces a 32-byte (256-bit) digest
        assert_eq!(
            RAW_HASH.len(),
            32,
            "RAW_HASH should be 32 bytes for SHA-256"
        );
    }

    #[test]
    fn test_plugin_hash_hex_format() {
        // SHA-256 hash as hex should be 64 characters (32 bytes * 2 hex chars per byte)
        assert_eq!(
            PLUGIN_HASH_HEX.len(),
            64,
            "PLUGIN_HASH_HEX should be 64 characters long"
        );

        // Should only contain valid hex characters
        assert!(
            PLUGIN_HASH_HEX.chars().all(|c| c.is_ascii_hexdigit()),
            "PLUGIN_HASH_HEX should only contain hex digits, got: {PLUGIN_HASH_HEX}",
        );

        // Should be lowercase (const-hex uses lowercase by default)
        assert!(
            PLUGIN_HASH_HEX.chars().all(|c| !c.is_ascii_uppercase()),
            "PLUGIN_HASH_HEX should be lowercase"
        );
    }

    #[test]
    fn test_hash_computation_is_deterministic() {
        // The hash should be consistent - recomputing should give same result
        use sha2_const_stable::Sha256;

        let recomputed_hash = Sha256::new().update(PLUGIN_HASH.as_bytes()).finalize();

        assert_eq!(
            RAW_HASH, recomputed_hash,
            "Hash computation should be deterministic"
        );
    }

    #[test]
    fn test_hex_encoding_matches_raw_hash() {
        // Verify that PLUGIN_HASH_HEX is the correct hex encoding of RAW_HASH
        use const_hex::{Buffer, const_encode};

        let recomputed_hex: Buffer<{ Sha256::DIGEST_SIZE }> = const_encode(&RAW_HASH);

        assert_eq!(
            PLUGIN_HASH_HEX,
            recomputed_hex.as_str(),
            "PLUGIN_HASH_HEX should match the hex encoding of RAW_HASH"
        );
    }

    #[test]
    fn test_hash_differs_from_empty_string() {
        // The hash should not be the hash of an empty string
        use sha2_const_stable::Sha256;

        let empty_hash = Sha256::new().update(b"").finalize();

        assert_ne!(
            RAW_HASH, empty_hash,
            "Hash should not be the hash of an empty string since PLUGIN_HASH is not empty"
        );
    }

    #[test]
    fn test_hash_differs_from_just_plugins_string() {
        // The hash should differ from just "plugins" if any features are enabled
        use sha2_const_stable::Sha256;

        let plugins_only_hash = Sha256::new().update(b"plugins").finalize();

        // With default features enabled, hash should differ from just "plugins"
        #[cfg(feature = "all-plugins")]
        assert_ne!(
            RAW_HASH, plugins_only_hash,
            "With all-plugins feature enabled, hash should differ from just 'plugins'"
        );
    }

    #[test]
    fn test_plugin_hash_contains_feature_markers() {
        // When all-plugins is enabled, PLUGIN_HASH should contain feature markers
        #[cfg(feature = "all-plugins")]
        {
            // Should be longer than just "plugins" since features are enabled
            assert!(
                PLUGIN_HASH.len() > "plugins".len(),
                "PLUGIN_HASH should contain feature markers when all-plugins is enabled"
            );
        }

        // When no features are enabled, should just be "plugins"
        #[cfg(not(feature = "all-plugins"))]
        {
            assert_eq!(
                PLUGIN_HASH, "plugins",
                "PLUGIN_HASH should be just 'plugins' when no plugin features are enabled"
            );
        }
    }

    #[test]
    fn test_hex_buf_is_valid_buffer() {
        // HEX_BUF should be a valid buffer that can be converted to string
        let hex_str = HEX_BUF.as_str();

        assert_eq!(
            hex_str.len(),
            64,
            "HEX_BUF as string should be 64 characters"
        );
        assert_eq!(
            hex_str, PLUGIN_HASH_HEX,
            "HEX_BUF.as_str() should equal PLUGIN_HASH_HEX"
        );
    }

    #[test]
    fn test_specific_feature_inclusion() {
        // Test that specific features are included in the hash string when enabled
        #[cfg(feature = "plugin-idiomorph")]
        assert!(
            PLUGIN_HASH.contains("-idiomorph"),
            "PLUGIN_HASH should contain '-idiomorph' when plugin-idiomorph feature is enabled"
        );

        #[cfg(feature = "plugin-nav")]
        assert!(
            PLUGIN_HASH.contains("-nav"),
            "PLUGIN_HASH should contain '-nav' when plugin-nav feature is enabled"
        );

        #[cfg(feature = "plugin-sse")]
        assert!(
            PLUGIN_HASH.contains("-sse"),
            "PLUGIN_HASH should contain '-sse' when plugin-sse feature is enabled"
        );

        #[cfg(feature = "plugin-routing")]
        assert!(
            PLUGIN_HASH.contains("-routing"),
            "PLUGIN_HASH should contain '-routing' when plugin-routing feature is enabled"
        );

        #[cfg(feature = "plugin-uuid")]
        assert!(
            PLUGIN_HASH.contains("-uuid"),
            "PLUGIN_HASH should contain '-uuid' when plugin-uuid feature is enabled"
        );
    }

    #[test]
    fn test_uuid_insecure_exclusivity() {
        // When uuid-insecure is enabled, the regular uuid should not be in the hash
        #[cfg(feature = "plugin-uuid-insecure")]
        {
            assert!(
                PLUGIN_HASH.contains("-uuid-insecure"),
                "PLUGIN_HASH should contain '-uuid-insecure' when plugin-uuid-insecure is enabled"
            );
        }

        // When plugin-uuid is enabled without uuid-insecure, should have -uuid
        #[cfg(all(feature = "plugin-uuid", not(feature = "plugin-uuid-insecure")))]
        {
            assert!(
                PLUGIN_HASH.contains("-uuid"),
                "PLUGIN_HASH should contain '-uuid' when plugin-uuid is enabled without uuid-insecure"
            );
            assert!(
                !PLUGIN_HASH.contains("-uuid-insecure"),
                "PLUGIN_HASH should not contain '-uuid-insecure' when only plugin-uuid is enabled"
            );
        }
    }

    #[test]
    fn test_actions_plugin_features() {
        // Test various action plugin features
        #[cfg(feature = "plugin-actions-click")]
        assert!(
            PLUGIN_HASH.contains("-actions-click"),
            "PLUGIN_HASH should contain '-actions-click' when plugin-actions-click is enabled"
        );

        #[cfg(feature = "plugin-actions-change")]
        assert!(
            PLUGIN_HASH.contains("-actions-change"),
            "PLUGIN_HASH should contain '-actions-change' when plugin-actions-change is enabled"
        );

        #[cfg(feature = "plugin-actions-key-down")]
        assert!(
            PLUGIN_HASH.contains("-actions-key-down"),
            "PLUGIN_HASH should contain '-actions-key-down' when plugin-actions-key-down is enabled"
        );

        #[cfg(feature = "plugin-actions-key-up")]
        assert!(
            PLUGIN_HASH.contains("-actions-key-up"),
            "PLUGIN_HASH should contain '-actions-key-up' when plugin-actions-key-up is enabled"
        );
    }
}
