//! CELT codec implementation for full-spectrum audio decoding.
//!
//! CELT (Constrained Energy Lapped Transform) is a low-latency, full-bandwidth audio codec
//! used in Opus for music and general audio. This module implements the CELT decoder which
//! operates internally at 48 kHz and supports all Opus bandwidths (NB/WB/SWB/FB).
//!
//! The CELT decoder is used for:
//! * CELT-only mode (configurations 16-31 in RFC 6716)
//! * High-frequency component in Hybrid mode (configurations 12-15)

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

mod constants;
mod decoder;
/// Fixed-point arithmetic utilities for CELT decoding
pub mod fixed_point;
mod pvq;

pub use constants::CELT_NUM_BANDS;
pub use decoder::CeltDecoder;
