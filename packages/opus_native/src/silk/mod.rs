//! SILK codec implementation for voice-optimized audio decoding.
//!
//! SILK is a voice-optimized codec used in Opus for efficient speech encoding.
//! This module implements the SILK decoder which operates at internal sample rates
//! of 8/12/16 kHz for Narrowband, Mediumband, and Wideband respectively.
//!
//! The SILK decoder is used for:
//! * SILK-only mode (configurations 0-11 in RFC 6716)
//! * Low-frequency component in Hybrid mode (configurations 12-15)

mod decoder;
mod excitation_constants;
mod frame;
mod lsf_constants;
mod ltp_constants;

pub use decoder::SilkDecoder;
pub use excitation_constants::*;
pub use frame::SilkFrame;
pub use lsf_constants::*;
pub use ltp_constants::*;
