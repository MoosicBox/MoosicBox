#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! # `MoosicBox` Opus Codec
//!
//! `RFC 6716` compliant Opus audio codec decoder for Symphonia.
//!
//! This crate is under development.

pub mod decoder;
pub mod error;
pub mod frame;
pub mod packet;
pub mod registry;
pub mod toc;

pub use decoder::OpusDecoder;
pub use error::{Error, Result};
pub use frame::{FramePacking, OpusFrame, decode_frame_length};
pub use packet::OpusPacket;
pub use registry::{create_opus_registry, register_opus_codec};
pub use toc::{Bandwidth, OpusMode, TocByte};
