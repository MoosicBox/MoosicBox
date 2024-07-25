use flume::Receiver;
use moosicbox_audio_decoder::{unsync::decode, DecodeError};
use symphonia::core::{
    audio::AudioBuffer, codecs::DecoderOptions, formats::FormatOptions, io::MediaSourceStream,
    meta::MetadataOptions, probe::Hint,
};
use thiserror::Error;

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
}

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
            Ok(decode(probed.format, track_num, seek_time, &decode_opts)?)
        }
        Err(err) => {
            // The input was not supported by any format reader.
            log::info!("the input is not supported: {err:?}");
            Err(PlaybackError::Symphonia(err))
        }
    }
}
