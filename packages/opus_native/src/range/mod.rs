//! Range coder for entropy coding in Opus.
//!
//! This module implements the range decoder used for entropy decoding in Opus packets
//! according to RFC 6716 Section 4.1. The range coder is a form of arithmetic coding
//! that efficiently encodes symbols based on their probability distributions.
//!
//! The range decoder supports:
//! * Binary symbol decoding (`ec_dec_bit_logp`)
//! * ICDF (Inverse Cumulative Distribution Function) table decoding (`ec_dec_icdf`)
//! * Raw bit extraction from packet end (`ec_dec_bits`)
//! * Uniform integer decoding (`ec_dec_uint`)
//! * Laplace distribution decoding (`ec_laplace_decode`)

/// Range decoder implementation for Opus entropy coding
pub mod decoder;

pub use decoder::RangeDecoder;
