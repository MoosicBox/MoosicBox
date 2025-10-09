#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

use std::ffi::c_int;

pub const OPUS_OK: c_int = 0;
pub const OPUS_APPLICATION_VOIP: c_int = 2048;
pub const OPUS_APPLICATION_AUDIO: c_int = 2049;

#[repr(C)]
pub struct OpusEncoder {
    _private: [u8; 0],
}

#[repr(C)]
pub struct OpusDecoder {
    _private: [u8; 0],
}

unsafe extern "C" {
    pub fn opus_encoder_create(
        fs: c_int,
        channels: c_int,
        application: c_int,
        error: *mut c_int,
    ) -> *mut OpusEncoder;

    pub fn opus_encode(
        st: *mut OpusEncoder,
        pcm: *const i16,
        frame_size: c_int,
        data: *mut u8,
        max_data_bytes: c_int,
    ) -> c_int;

    pub fn opus_encoder_destroy(st: *mut OpusEncoder);

    pub fn opus_decoder_create(fs: c_int, channels: c_int, error: *mut c_int) -> *mut OpusDecoder;

    pub fn opus_decode(
        st: *mut OpusDecoder,
        data: *const u8,
        len: c_int,
        pcm: *mut i16,
        frame_size: c_int,
        decode_fec: c_int,
    ) -> c_int;

    pub fn opus_decoder_destroy(st: *mut OpusDecoder);
}

pub mod safe {
    use super::{
        OPUS_OK, OpusDecoder, OpusEncoder, opus_decode, opus_decoder_create, opus_decoder_destroy,
        opus_encode, opus_encoder_create, opus_encoder_destroy,
    };
    use std::ffi::c_int;

    #[derive(Debug)]
    pub enum OpusError {
        EncoderCreateFailed(c_int),
        DecoderCreateFailed(c_int),
        EncodeFailed(c_int),
        DecodeFailed(c_int),
        InvalidSampleRate,
        InvalidChannels,
    }

    pub struct Encoder {
        ptr: *mut OpusEncoder,
    }

    impl Encoder {
        /// # Errors
        ///
        /// * `InvalidSampleRate` - sample rate not one of 8000, 12000, 16000, 24000, 48000 Hz
        /// * `InvalidChannels` - channels not 1 or 2
        /// * `EncoderCreateFailed` - libopus encoder creation failed
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

    pub struct Decoder {
        ptr: *mut OpusDecoder,
    }

    impl Decoder {
        /// # Errors
        ///
        /// * `InvalidSampleRate` - sample rate not one of 8000, 12000, 16000, 24000, 48000 Hz
        /// * `InvalidChannels` - channels not 1 or 2
        /// * `DecoderCreateFailed` - libopus decoder creation failed
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
