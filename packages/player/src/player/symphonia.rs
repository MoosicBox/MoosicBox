use std::{fs::File, path::Path};

use moosicbox_audio_decoder::{decode, AudioDecodeHandler, DecodeError};
use symphonia::core::{
    codecs::DecoderOptions, formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions,
    probe::Hint,
};
use thiserror::Error;
use tokio::task::JoinError;

impl From<std::io::Error> for PlaybackError {
    fn from(err: std::io::Error) -> Self {
        PlaybackError::Symphonia(symphonia::core::errors::Error::IoError(err))
    }
}

#[derive(Debug, Error)]
pub enum PlaybackError {
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error(transparent)]
    Symphonia(#[from] symphonia::core::errors::Error),
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error("No audio outputs")]
    NoAudioOutputs,
    #[error("Invalid source")]
    InvalidSource,
}

pub async fn play_file_path_str_async(
    path_str: &str,
    get_audio_output_handler: impl FnOnce() -> GetAudioDecodeHandlerRet + Send + 'static,
    enable_gapless: bool,
    verify: bool,
    track_num: Option<usize>,
    seek: Option<f64>,
) -> Result<i32, PlaybackError> {
    let path_str = path_str.to_owned();
    moosicbox_task::spawn_blocking("audio_decoder: Play file path", move || {
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
    if let Some(extension) = path.extension() {
        if let Some(extension_str) = extension.to_str() {
            hint.with_extension(extension_str);
        }
    }

    let source = Box::new(File::open(path)?);

    // Create the media source stream using the boxed media source from above.
    let mss = MediaSourceStream::new(source, Default::default());

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

pub type GetAudioDecodeHandlerRet = Result<AudioDecodeHandler, PlaybackError>;

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
    moosicbox_task::spawn_blocking("audio_decoder: Play media source", move || {
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

#[allow(clippy::too_many_arguments)]
fn play_media_source(
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
    let metadata_opts: MetadataOptions = Default::default();

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
                &decode_opts,
            )?)
        }
        Err(err) => {
            // The input was not supported by any format reader.
            log::info!("the input is not supported: {err:?}");
            Err(PlaybackError::Symphonia(err))
        }
    }
}
