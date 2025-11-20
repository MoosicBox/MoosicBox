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
