//! Minimal FFI bindings to libopus for test vector generation.
//!
//! This crate provides low-level FFI bindings to libopus along with safe Rust wrappers
//! for encoding and decoding Opus audio. It is intended for internal use within `MoosicBox`
//! for generating test vectors and is not published to crates.io.
//!
//! # Usage
//!
//! For most use cases, prefer the safe wrappers in the [`safe`] module:
//!
//! ```rust
//! use moosicbox_opus_native_libopus::{OPUS_APPLICATION_AUDIO, safe::{Encoder, Decoder}};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create encoder and decoder
//! let mut encoder = Encoder::new(48000, 1, OPUS_APPLICATION_AUDIO)?;
//! let mut decoder = Decoder::new(48000, 1)?;
//!
//! // Encode PCM audio
//! let input_pcm = vec![0i16; 960];
//! let mut packet = vec![0u8; 4000];
//! let packet_len = encoder.encode(&input_pcm, 960, &mut packet)?;
//!
//! // Decode back to PCM
//! let mut output_pcm = vec![0i16; 960];
//! let samples = decoder.decode(&packet[..packet_len], &mut output_pcm, 960, false)?;
//! # Ok(())
//! # }
//! ```
//!
//! # FFI Functions
//!
//! Raw FFI functions are also exported for advanced use cases requiring direct libopus access.
//! These functions are unsafe and require careful memory management.

#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::multiple_crate_versions
)]

use std::ffi::c_int;

/// Success return code from libopus functions.
pub const OPUS_OK: c_int = 0;

/// Application mode optimized for voice over IP.
pub const OPUS_APPLICATION_VOIP: c_int = 2048;

/// Application mode optimized for general audio encoding.
pub const OPUS_APPLICATION_AUDIO: c_int = 2049;

/// Opaque handle to a libopus encoder instance.
///
/// This struct represents a low-level FFI binding to the libopus encoder.
/// For a safe Rust API, use [`safe::Encoder`] instead.
#[repr(C)]
pub struct OpusEncoder {
    _private: [u8; 0],
}

/// Opaque handle to a libopus decoder instance.
///
/// This struct represents a low-level FFI binding to the libopus decoder.
/// For a safe Rust API, use [`safe::Decoder`] instead.
#[repr(C)]
pub struct OpusDecoder {
    _private: [u8; 0],
}

unsafe extern "C" {
    /// Creates a new Opus encoder.
    ///
    /// # Safety
    ///
    /// * `error` must be a valid pointer to writable memory for storing the error code
    /// * The returned pointer must be freed with [`opus_encoder_destroy`]
    pub fn opus_encoder_create(
        fs: c_int,
        channels: c_int,
        application: c_int,
        error: *mut c_int,
    ) -> *mut OpusEncoder;

    /// Encodes an Opus frame.
    ///
    /// # Safety
    ///
    /// * `st` must be a valid encoder created by [`opus_encoder_create`]
    /// * `pcm` must point to at least `frame_size * channels` valid `i16` samples
    /// * `data` must point to at least `max_data_bytes` writable bytes
    pub fn opus_encode(
        st: *mut OpusEncoder,
        pcm: *const i16,
        frame_size: c_int,
        data: *mut u8,
        max_data_bytes: c_int,
    ) -> c_int;

    /// Destroys an Opus encoder and frees its resources.
    ///
    /// # Safety
    ///
    /// * `st` must be a valid encoder created by [`opus_encoder_create`]
    /// * `st` must not be used after this call
    pub fn opus_encoder_destroy(st: *mut OpusEncoder);

    /// Creates a new Opus decoder.
    ///
    /// # Safety
    ///
    /// * `error` must be a valid pointer to writable memory for storing the error code
    /// * The returned pointer must be freed with [`opus_decoder_destroy`]
    pub fn opus_decoder_create(fs: c_int, channels: c_int, error: *mut c_int) -> *mut OpusDecoder;

    /// Decodes an Opus packet.
    ///
    /// # Safety
    ///
    /// * `st` must be a valid decoder created by [`opus_decoder_create`]
    /// * `data` must point to at least `len` valid bytes
    /// * `pcm` must point to at least `frame_size * channels` writable `i16` samples
    pub fn opus_decode(
        st: *mut OpusDecoder,
        data: *const u8,
        len: c_int,
        pcm: *mut i16,
        frame_size: c_int,
        decode_fec: c_int,
    ) -> c_int;

    /// Destroys an Opus decoder and frees its resources.
    ///
    /// # Safety
    ///
    /// * `st` must be a valid decoder created by [`opus_decoder_create`]
    /// * `st` must not be used after this call
    pub fn opus_decoder_destroy(st: *mut OpusDecoder);
}

/// Safe Rust wrapper around the libopus FFI.
///
/// This module provides safe, idiomatic Rust APIs for Opus encoding and decoding.
/// All FFI interactions are encapsulated, memory is managed automatically,
/// and errors are returned as `Result` types.
pub mod safe {
    use super::{
        OPUS_OK, OpusDecoder, OpusEncoder, opus_decode, opus_decoder_create, opus_decoder_destroy,
        opus_encode, opus_encoder_create, opus_encoder_destroy,
    };
    use std::ffi::c_int;

    /// Errors that can occur during Opus encoding and decoding operations.
    ///
    /// This enum represents all possible error conditions from the safe Opus API,
    /// including encoder/decoder creation failures and encode/decode operation failures.
    #[derive(thiserror::Error, Debug)]
    pub enum OpusError {
        /// Failed to create an encoder, with libopus error code.
        #[error("Failed to create an encoder, with libopus error code: {0}")]
        EncoderCreateFailed(c_int),
        /// Failed to create a decoder, with libopus error code.
        #[error("Failed to create a decoder, with libopus error code: {0}")]
        DecoderCreateFailed(c_int),
        /// Failed to encode audio, with libopus error code.
        #[error("Failed to encode audio, with libopus error code: {0}")]
        EncodeFailed(c_int),
        /// Failed to decode audio, with libopus error code.
        #[error("Failed to decode audio, with libopus error code: {0}")]
        DecodeFailed(c_int),
        /// Sample rate not one of 8000, 12000, 16000, 24000, or 48000 Hz.
        #[error("Sample rate not one of 8000, 12000, 16000, 24000, or 48000 Hz")]
        InvalidSampleRate,
        /// Channel count not 1 or 2.
        #[error("Channel count not 1 or 2")]
        InvalidChannels,
    }

    /// Safe wrapper around an Opus encoder.
    ///
    /// Automatically manages the lifecycle of the underlying libopus encoder.
    pub struct Encoder {
        ptr: *mut OpusEncoder,
    }

    impl Encoder {
        /// Creates a new Opus encoder.
        ///
        /// # Errors
        ///
        /// * `InvalidSampleRate` - sample rate not one of 8000, 12000, 16000, 24000, 48000 Hz
        /// * `InvalidChannels` - channels not 1 or 2
        /// * `EncoderCreateFailed` - libopus encoder creation failed
        #[must_use]
        pub fn new(sample_rate: u32, channels: u8, application: c_int) -> Result<Self, OpusError> {
            if ![8000, 12000, 16000, 24000, 48000].contains(&sample_rate) {
                return Err(OpusError::InvalidSampleRate);
            }
            if channels == 0 || channels > 2 {
                return Err(OpusError::InvalidChannels);
            }

            let mut error: c_int = 0;
            let ptr = unsafe {
                opus_encoder_create(
                    sample_rate as c_int,
                    c_int::from(channels),
                    application,
                    &raw mut error,
                )
            };

            if error != OPUS_OK || ptr.is_null() {
                return Err(OpusError::EncoderCreateFailed(error));
            }

            Ok(Self { ptr })
        }

        /// Encodes PCM audio data to Opus format.
        ///
        /// Returns the number of bytes written to the output buffer.
        ///
        /// # Errors
        ///
        /// * `EncodeFailed` - libopus encoding failed with error code
        pub fn encode(
            &mut self,
            pcm: &[i16],
            frame_size: usize,
            output: &mut [u8],
        ) -> Result<usize, OpusError> {
            let result = unsafe {
                opus_encode(
                    self.ptr,
                    pcm.as_ptr(),
                    frame_size as c_int,
                    output.as_mut_ptr(),
                    output.len() as c_int,
                )
            };

            if result < 0 {
                return Err(OpusError::EncodeFailed(result));
            }

            Ok(result as usize)
        }
    }

    impl Drop for Encoder {
        fn drop(&mut self) {
            unsafe {
                opus_encoder_destroy(self.ptr);
            }
        }
    }

    unsafe impl Send for Encoder {}

    /// Safe wrapper around an Opus decoder.
    ///
    /// Automatically manages the lifecycle of the underlying libopus decoder.
    pub struct Decoder {
        ptr: *mut OpusDecoder,
    }

    impl Decoder {
        /// Creates a new Opus decoder.
        ///
        /// # Errors
        ///
        /// * `InvalidSampleRate` - sample rate not one of 8000, 12000, 16000, 24000, 48000 Hz
        /// * `InvalidChannels` - channels not 1 or 2
        /// * `DecoderCreateFailed` - libopus decoder creation failed
        #[must_use]
        pub fn new(sample_rate: u32, channels: u8) -> Result<Self, OpusError> {
            if ![8000, 12000, 16000, 24000, 48000].contains(&sample_rate) {
                return Err(OpusError::InvalidSampleRate);
            }
            if channels == 0 || channels > 2 {
                return Err(OpusError::InvalidChannels);
            }

            let mut error: c_int = 0;
            let ptr = unsafe {
                opus_decoder_create(sample_rate as c_int, c_int::from(channels), &raw mut error)
            };

            if error != OPUS_OK || ptr.is_null() {
                return Err(OpusError::DecoderCreateFailed(error));
            }

            Ok(Self { ptr })
        }

        /// Decodes Opus-encoded audio data to PCM format.
        ///
        /// Returns the number of samples decoded per channel.
        ///
        /// # Errors
        ///
        /// * `DecodeFailed` - libopus decoding failed with error code
        pub fn decode(
            &mut self,
            data: &[u8],
            output: &mut [i16],
            frame_size: usize,
            decode_fec: bool,
        ) -> Result<usize, OpusError> {
            let result = unsafe {
                opus_decode(
                    self.ptr,
                    data.as_ptr(),
                    data.len() as c_int,
                    output.as_mut_ptr(),
                    frame_size as c_int,
                    c_int::from(decode_fec),
                )
            };

            if result < 0 {
                return Err(OpusError::DecodeFailed(result));
            }

            Ok(result as usize)
        }
    }

    impl Drop for Decoder {
        fn drop(&mut self) {
            unsafe {
                opus_decoder_destroy(self.ptr);
            }
        }
    }

    unsafe impl Send for Decoder {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_decode_roundtrip() {
        let sample_rate = 48000;
        let channels = 1;
        let frame_size = 960;

        let mut encoder =
            safe::Encoder::new(sample_rate, channels, OPUS_APPLICATION_AUDIO).unwrap();
        let mut decoder = safe::Decoder::new(sample_rate, channels).unwrap();

        let input_pcm = vec![0i16; frame_size];
        let mut packet = vec![0u8; 4000];

        let packet_len = encoder.encode(&input_pcm, frame_size, &mut packet).unwrap();
        assert!(packet_len > 0);

        packet.truncate(packet_len);

        let mut output_pcm = vec![0i16; frame_size];
        let decoded_samples = decoder
            .decode(&packet, &mut output_pcm, frame_size, false)
            .unwrap();

        assert_eq!(decoded_samples, frame_size);
    }
}
