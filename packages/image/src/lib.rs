//! Image resizing and format conversion utilities.
//!
//! This crate provides high-performance image resizing with support for multiple backends
//! and output formats. It offers both synchronous and asynchronous APIs for image manipulation.
//!
//! # Backends
//!
//! * `libvips` - High-performance image processing using libvips (not available on Windows)
//! * `image` - Pure Rust image processing using the `image` crate
//!
//! # Supported Formats
//!
//! * JPEG - Lossy compression with configurable quality
//! * WebP - Modern image format with better compression
//!
//! # Features
//!
//! * `libvips` - Enable libvips backend for high-performance processing
//! * `image` - Enable pure Rust image backend
//!
//! # Examples
//!
//! ```rust,no_run
//! # #[cfg(feature = "image")]
//! # {
//! # use moosicbox_image::Encoding;
//! use moosicbox_image::image::try_resize_local_file;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Resize an image to 800x600 JPEG with quality 85
//! let resized = try_resize_local_file(
//!     800,
//!     600,
//!     "/path/to/image.jpg",
//!     Encoding::Jpeg,
//!     85,
//! )?;
//! # Ok(())
//! # }
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use strum_macros::{AsRefStr, Display, EnumString};

/// Image manipulation utilities using the `image` crate.
#[cfg(feature = "image")]
pub mod image;
/// Image manipulation utilities using `libvips`.
#[cfg(not(target_os = "windows"))]
#[cfg(feature = "libvips")]
pub mod libvips;

/// Image encoding format.
#[derive(Debug, Copy, Clone, Display, EnumString, AsRefStr)]
#[strum(serialize_all = "SCREAMING_SNAKE_CASE")]
#[must_use]
pub enum Encoding {
    /// JPEG encoding format.
    Jpeg,
    /// WebP encoding format.
    Webp,
}
