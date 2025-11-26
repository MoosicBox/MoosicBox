//! Synchronous audio decoding using Symphonia.
//!
//! This module provides synchronous (non-async) audio playback functions using the
//! Symphonia decoder. It's used for environments that don't require async operations
//! or when blocking playback is acceptable.

use flume::Receiver;
use moosicbox_audio_decoder::{DecodeError, unsync::decode};
use symphonia::core::{
    audio::AudioBuffer, codecs::DecoderOptions, formats::FormatOptions, io::MediaSourceStream,
    meta::MetadataOptions, probe::Hint,
};
use thiserror::Error;

impl From<std::io::Error> for PlaybackError {
    fn from(err: std::io::Error) -> Self {
        Self::Symphonia(symphonia::core::errors::Error::IoError(err))
    }
}

/// Errors that can occur during synchronous audio playback.
#[derive(Debug, Error)]
pub enum PlaybackError {
    /// Error from audio decoding
    #[error(transparent)]
    Decode(#[from] DecodeError),
    /// Error from the Symphonia decoder
    #[error(transparent)]
    Symphonia(#[from] symphonia::core::errors::Error),
}

/// Plays audio from a media source stream synchronously.
///
/// Probes the media stream format and returns a receiver for decoded audio buffers.
/// This is a non-async version suitable for blocking contexts.
///
/// # Errors
///
/// * If failed to play the `MediaSourceStream`
/// * If failed to probe for the media metadata format
#[allow(clippy::too_many_arguments)]
pub fn play_media_source(
    media_source_stream: MediaSourceStream,
    hint: &Hint,
    enable_gapless: bool,
    verify: bool,
    track_num: Option<usize>,
    seek: Option<f64>,
) -> Result<Receiver<AudioBuffer<f32>>, PlaybackError> {
    // Use the default options for format readers other than for gapless playback.
    let format_opts = FormatOptions {
        enable_gapless,
        ..Default::default()
    };

    // Use the default options for metadata readers.
    let metadata_opts = MetadataOptions::default();

    // Probe the media source stream for metadata and get the format reader.
    match symphonia::default::get_probe().format(
        hint,
        media_source_stream,
        &format_opts,
        &metadata_opts,
    ) {
        Ok(probed) => {
            // If present, parse the seek argument.
            let seek_time = seek;

            // Set the decoder options.
            let decode_opts = DecoderOptions { verify };

            // Play it!
            Ok(decode(probed.format, track_num, seek_time, decode_opts)?)
        }
        Err(err) => {
            // The input was not supported by any format reader.
            log::info!("the input is not supported: {err:?}");
            Err(PlaybackError::Symphonia(err))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use moosicbox_audio_decoder::AudioDecodeError;

    #[test_log::test]
    fn test_playback_error_from_io_error() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "test file not found");
        let playback_error: PlaybackError = io_error.into();

        // Should be converted to Symphonia(IoError)
        assert!(matches!(playback_error, PlaybackError::Symphonia(_)));
        assert!(playback_error.to_string().contains("test file not found"));
    }

    #[test_log::test]
    fn test_playback_error_from_decode_error() {
        // Use AudioDecode variant with StreamClosed error type
        let decode_error = DecodeError::AudioDecode(AudioDecodeError::StreamClosed);
        let playback_error: PlaybackError = decode_error.into();

        assert!(matches!(playback_error, PlaybackError::Decode(_)));
        assert!(!playback_error.to_string().is_empty());
    }

    #[test_log::test]
    fn test_playback_error_display_variants() {
        // Test Decode variant display with PlayStream error
        let error = PlaybackError::Decode(DecodeError::AudioDecode(AudioDecodeError::PlayStream));
        assert!(!error.to_string().is_empty());

        // Test that errors can be debugged
        let debug_str = format!("{error:?}");
        assert!(!debug_str.is_empty());
    }

    #[test_log::test]
    fn test_playback_error_decode_multiple_variants() {
        // Test multiple AudioDecodeError variants in the unsync context
        let errors = [
            AudioDecodeError::OpenStream,
            AudioDecodeError::PlayStream,
            AudioDecodeError::StreamClosed,
            AudioDecodeError::StreamEnd,
            AudioDecodeError::Interrupt,
        ];

        for error in errors {
            let decode_error = DecodeError::AudioDecode(error);
            let playback_error: PlaybackError = decode_error.into();
            assert!(matches!(playback_error, PlaybackError::Decode(_)));
            // All variants should have non-empty display output
            assert!(!playback_error.to_string().is_empty());
        }
    }
}
