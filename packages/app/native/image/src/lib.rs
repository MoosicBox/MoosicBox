#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
//! Embedded static assets for native applications.
//!
//! This crate provides compile-time embedded access to static assets (images, icons, etc.)
//! from the `public/` directory using the `rust_embed` library. Assets are embedded directly
//! into the binary, eliminating the need for runtime file system access.
//!
//! # Primary Use Cases
//!
//! * Embedding application icons and images in native GUI applications
//! * Providing fallback assets when file system access is unavailable
//! * Ensuring assets are always available without external dependencies
//!
//! # Example
//!
//! ```rust
//! use moosicbox_app_native_image::{Asset, get_asset_arc_bytes};
//! use rust_embed::RustEmbed;
//!
//! // Access an embedded asset by path
//! if let Some(asset) = Asset::get("/public/icon.png") {
//!     let bytes = get_asset_arc_bytes(asset);
//!     // Use the bytes...
//! }
//! ```
//!
//! # Main Entry Points
//!
//! * [`Asset`] - The embedded asset collection
//! * [`get_asset_arc_bytes`] - Convert embedded files to `Arc<Bytes>`

use std::{borrow::Cow, sync::Arc};

use bytes::Bytes;
use rust_embed::{Embed, EmbeddedFile};

/// Embedded static assets from the `public/` directory.
///
/// This struct provides access to files embedded at compile time using `rust_embed`.
/// Assets are prefixed with `/public/` in their paths.
#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../public/"]
#[prefix = "/public/"]
pub struct Asset;

/// Converts a `Cow<[u8]>` into an `Arc<Bytes>`.
///
/// This internal helper handles both owned and borrowed byte slices, converting them
/// into a reference-counted byte buffer for efficient sharing.
#[must_use]
fn cow_to_arc_bytes(cow: Cow<'_, [u8]>) -> Arc<Bytes> {
    Arc::new(match cow {
        Cow::Owned(vec) => Bytes::from(vec),
        Cow::Borrowed(slice) => Bytes::copy_from_slice(slice),
    })
}

/// Converts an embedded asset file into an `Arc<Bytes>`.
///
/// This function takes an `EmbeddedFile` and converts its data into a reference-counted
/// byte buffer for efficient sharing across threads.
#[must_use]
pub fn get_asset_arc_bytes(asset: EmbeddedFile) -> Arc<Bytes> {
    cow_to_arc_bytes(asset.data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    /// Tests that `cow_to_arc_bytes` correctly handles owned byte vectors.
    ///
    /// This verifies that when passed an owned `Vec<u8>`, the function converts
    /// it to `Bytes` without copying and wraps it in an `Arc`.
    #[test]
    fn test_cow_to_arc_bytes_owned() {
        let data = vec![1, 2, 3, 4, 5];
        let expected = data.clone();
        let cow: Cow<'_, [u8]> = Cow::Owned(data);

        let result = cow_to_arc_bytes(cow);

        assert_eq!(&**result, expected.as_slice());
    }

    /// Tests that `cow_to_arc_bytes` correctly handles borrowed byte slices.
    ///
    /// This verifies that when passed a borrowed slice, the function copies
    /// the data to create a new `Bytes` instance wrapped in an `Arc`.
    #[test]
    fn test_cow_to_arc_bytes_borrowed() {
        let data: &[u8] = &[10, 20, 30, 40, 50];
        let cow: Cow<'_, [u8]> = Cow::Borrowed(data);

        let result = cow_to_arc_bytes(cow);

        assert_eq!(&**result, data);
    }

    /// Tests that `cow_to_arc_bytes` correctly handles empty byte sequences.
    ///
    /// This edge case verifies that both owned and borrowed empty data
    /// are handled correctly without panicking.
    #[test]
    fn test_cow_to_arc_bytes_empty() {
        // Test with owned empty vec
        let owned_cow: Cow<'_, [u8]> = Cow::Owned(Vec::new());
        let owned_result = cow_to_arc_bytes(owned_cow);
        assert_eq!(&**owned_result, &[]);

        // Test with borrowed empty slice
        let borrowed_cow: Cow<'_, [u8]> = Cow::Borrowed(&[]);
        let borrowed_result = cow_to_arc_bytes(borrowed_cow);
        assert_eq!(&**borrowed_result, &[]);
    }

    /// Tests that `get_asset_arc_bytes` correctly converts an `EmbeddedFile`.
    ///
    /// This verifies that the function properly extracts the data from an
    /// embedded file and wraps it in an `Arc<Bytes>`.
    #[test]
    fn test_get_asset_arc_bytes() {
        let data = vec![100, 101, 102, 103];
        let expected = data.clone();
        // Create a minimal metadata struct using the internal constructor
        let metadata = rust_embed::Metadata::__rust_embed_new([0u8; 32], None, None);
        let embedded_file = EmbeddedFile {
            data: Cow::Owned(data),
            metadata,
        };

        let result = get_asset_arc_bytes(embedded_file);

        assert_eq!(&**result, expected.as_slice());
    }

    /// Tests that `Asset::get` can successfully retrieve an embedded asset.
    ///
    /// This integration test verifies that the `rust_embed` macro correctly
    /// embeds files and that they can be retrieved and converted to `Arc<Bytes>`.
    ///
    /// Uses `/public/favicon.ico` as a known embedded asset for testing.
    #[test]
    fn test_asset_get_existing_file() {
        let asset = Asset::get("/public/favicon.ico");

        assert!(
            asset.is_some(),
            "Expected favicon.ico to be embedded in the binary"
        );

        let asset_file = asset.unwrap();
        let bytes = get_asset_arc_bytes(asset_file);

        // Verify we got some data (favicon.ico should not be empty)
        assert!(!bytes.is_empty(), "Expected embedded asset to contain data");

        // Verify it's a valid ICO file by checking the magic bytes
        // ICO files start with 00 00 01 00 (reserved, type=icon)
        assert_eq!(
            &bytes[0..4],
            &[0x00, 0x00, 0x01, 0x00],
            "Expected valid ICO file header"
        );
    }

    /// Tests that `Asset::get` returns `None` for non-existent assets.
    ///
    /// This verifies that attempting to retrieve an asset that was not
    /// embedded returns `None` rather than panicking.
    #[test]
    fn test_asset_get_nonexistent_file() {
        let asset = Asset::get("/public/nonexistent-file.xyz");

        assert!(
            asset.is_none(),
            "Expected None for non-existent embedded asset"
        );
    }

    /// Tests that multiple `Arc<Bytes>` references can share the same data.
    ///
    /// This verifies the reference-counting behavior by creating multiple
    /// clones and ensuring they all point to the same underlying data.
    #[test]
    fn test_arc_bytes_sharing() {
        let data = vec![1, 2, 3, 4, 5];
        let cow: Cow<'_, [u8]> = Cow::Owned(data);

        let arc1 = cow_to_arc_bytes(cow);
        let arc2 = Arc::clone(&arc1);
        let arc3 = Arc::clone(&arc1);

        // All should point to the same data
        assert_eq!(&**arc1, &[1, 2, 3, 4, 5]);
        assert_eq!(&**arc2, &[1, 2, 3, 4, 5]);
        assert_eq!(&**arc3, &[1, 2, 3, 4, 5]);

        // Verify they're actually sharing (same pointer)
        assert_eq!(Arc::strong_count(&arc1), 3);
    }

    /// Tests that embedded assets can be retrieved with different path formats.
    ///
    /// This verifies that the `/public/` prefix is correctly handled and
    /// assets can be accessed consistently.
    #[test]
    fn test_asset_path_handling() {
        // Asset should be accessible with /public/ prefix
        let with_prefix = Asset::get("/public/favicon.ico");
        assert!(with_prefix.is_some(), "Expected asset with /public/ prefix");

        // Asset should NOT be accessible without /public/ prefix
        // (because rust_embed uses the prefix parameter)
        let without_prefix = Asset::get("favicon.ico");
        assert!(
            without_prefix.is_none(),
            "Expected None without /public/ prefix"
        );
    }
}
