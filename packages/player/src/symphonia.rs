//! Asynchronous audio file playback using Symphonia.
//!
//! This module provides functions for playing audio files asynchronously using the
//! Symphonia decoder. It supports various audio formats and handles audio decoding
//! with options for gapless playback and verification.

use std::{fs::File, path::Path};

use moosicbox_audio_decoder::{AudioDecodeHandler, DecodeError, decode};
use switchy_async::task::JoinError;
use symphonia::core::{
    codecs::DecoderOptions,
    formats::FormatOptions,
    io::{MediaSourceStream, MediaSourceStreamOptions},
    meta::MetadataOptions,
    probe::Hint,
};
use thiserror::Error;

impl From<std::io::Error> for PlaybackError {
    fn from(err: std::io::Error) -> Self {
        Self::Symphonia(symphonia::core::errors::Error::IoError(err))
    }
}

/// Errors that can occur during asynchronous audio playback.
#[derive(Debug, Error)]
pub enum PlaybackError {
    /// Error from audio decoding
    #[error(transparent)]
    Decode(#[from] DecodeError),
    /// Error from the Symphonia decoder
    #[error(transparent)]
    Symphonia(#[from] symphonia::core::errors::Error),
    /// Error joining async task
    #[error(transparent)]
    Join(#[from] JoinError),
    /// No audio output devices available
    #[error("No audio outputs")]
    NoAudioOutputs,
    /// Invalid audio source
    #[error("Invalid source")]
    InvalidSource,
}

/// Plays an audio file asynchronously from a file path string.
///
/// Loads and plays the audio file at the specified path using the Symphonia decoder
/// in a blocking task, with optional gapless playback and verification.
///
/// # Errors
///
/// * If failed to play the file path
/// * If the tokio task failed to join
pub async fn play_file_path_str_async(
    path_str: &str,
    get_audio_output_handler: impl FnOnce() -> GetAudioDecodeHandlerRet + Send + 'static,
    enable_gapless: bool,
    verify: bool,
    track_num: Option<usize>,
    seek: Option<f64>,
) -> Result<i32, PlaybackError> {
    let path_str = path_str.to_owned();
    switchy_async::runtime::Handle::current()
        .spawn_blocking_with_name("audio_decoder: Play file path", move || {
            let mut handler = get_audio_output_handler()?;
            play_file_path_str(
                &path_str,
                &mut handler,
                enable_gapless,
                verify,
                track_num,
                seek,
            )
        })
        .await?
}

#[allow(clippy::too_many_arguments)]
fn play_file_path_str(
    path_str: &str,
    audio_decode_handler: &mut AudioDecodeHandler,
    enable_gapless: bool,
    verify: bool,
    track_num: Option<usize>,
    seek: Option<f64>,
) -> Result<i32, PlaybackError> {
    // Create a hint to help the format registry guess what format reader is appropriate.
    let mut hint = Hint::new();

    let path = Path::new(path_str);

    // Provide the file extension as a hint.
    if let Some(extension) = path.extension()
        && let Some(extension_str) = extension.to_str()
    {
        hint.with_extension(extension_str);
    }

    let source = Box::new(File::open(path)?);

    // Create the media source stream using the boxed media source from above.
    let mss = MediaSourceStream::new(source, MediaSourceStreamOptions::default());

    play_media_source(
        mss,
        &hint,
        audio_decode_handler,
        enable_gapless,
        verify,
        track_num,
        seek,
    )
}

/// Return type for functions that provide audio decode handlers.
///
/// This type alias represents a `Result` that either contains an `AudioDecodeHandler`
/// for processing decoded audio or a `PlaybackError` if handler creation fails.
pub type GetAudioDecodeHandlerRet = Result<AudioDecodeHandler, PlaybackError>;

/// Plays audio from a media source stream asynchronously.
///
/// Decodes and plays audio from the provided `MediaSourceStream` in a blocking task,
/// using the Symphonia decoder with optional gapless playback and verification.
///
/// # Errors
///
/// * If failed to play the `MediaSourceStream`
/// * If the tokio task failed to join
pub async fn play_media_source_async(
    media_source_stream: MediaSourceStream,
    hint: &Hint,
    get_audio_output_handler: impl FnOnce() -> GetAudioDecodeHandlerRet + Send + 'static,
    enable_gapless: bool,
    verify: bool,
    track_num: Option<usize>,
    seek: Option<f64>,
) -> Result<i32, PlaybackError> {
    let hint = hint.clone();
    switchy_async::runtime::Handle::current()
        .spawn_blocking_with_name("audio_decoder: Play media source", move || {
            let mut handler = get_audio_output_handler()?;
            play_media_source(
                media_source_stream,
                &hint,
                &mut handler,
                enable_gapless,
                verify,
                track_num,
                seek,
            )
        })
        .await?
}

/// Plays audio from a media source stream synchronously.
///
/// Decodes and plays audio from the provided `MediaSourceStream` using the Symphonia decoder,
/// probing the stream format and processing audio through the provided handler.
///
/// # Errors
///
/// * If failed to play the `MediaSourceStream`
#[allow(clippy::too_many_arguments)]
pub fn play_media_source(
    media_source_stream: MediaSourceStream,
    hint: &Hint,
    audio_decode_handler: &mut AudioDecodeHandler,
    enable_gapless: bool,
    verify: bool,
    track_num: Option<usize>,
    seek: Option<f64>,
) -> Result<i32, PlaybackError> {
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
            Ok(decode(
                probed.format,
                audio_decode_handler,
                track_num,
                seek_time,
                decode_opts,
            )?)
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
    fn test_playback_error_display_no_audio_outputs() {
        let error = PlaybackError::NoAudioOutputs;
        assert!(error.to_string().contains("No audio outputs"));
    }

    #[test_log::test]
    fn test_playback_error_display_invalid_source() {
        let error = PlaybackError::InvalidSource;
        assert!(error.to_string().contains("Invalid source"));
    }

    #[test_log::test]
    fn test_playback_error_from_io_error() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let playback_error: PlaybackError = io_error.into();

        // Should be converted to Symphonia(IoError)
        assert!(matches!(playback_error, PlaybackError::Symphonia(_)));
        assert!(playback_error.to_string().contains("file not found"));
    }

    #[test_log::test]
    fn test_playback_error_from_decode_error() {
        // Use AudioDecode variant with OpenStream error type
        let decode_error = DecodeError::AudioDecode(AudioDecodeError::OpenStream);
        let playback_error: PlaybackError = decode_error.into();

        assert!(matches!(playback_error, PlaybackError::Decode(_)));
        // OpenStream error should have "OpenStreamError" in its display
        assert!(!playback_error.to_string().is_empty());
    }

    #[test_log::test]
    fn test_playback_error_decode_variants() {
        // Test multiple AudioDecodeError variants
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
