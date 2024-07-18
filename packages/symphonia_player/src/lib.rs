#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]

use std::fs::File;
use std::io;
use std::path::Path;

use output::{AudioOutputError, AudioOutputHandler};
use symphonia::core::codecs::{DecoderOptions, FinalizeResult, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;
use thiserror::Error;
use tokio::task::JoinError;

pub mod media_sources;
pub mod output;
pub mod resampler;
pub mod signal_chain;
pub mod unsync;
pub mod volume_mixer;

impl From<io::Error> for PlaybackError {
    fn from(err: io::Error) -> Self {
        PlaybackError::Symphonia(Error::IoError(err))
    }
}

#[derive(Debug, Error)]
pub enum PlaybackError {
    #[error(transparent)]
    AudioOutput(#[from] AudioOutputError),
    #[error(transparent)]
    Symphonia(#[from] Error),
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error("No audio outputs")]
    NoAudioOutputs,
    #[error("Invalid source")]
    InvalidSource,
}

pub async fn play_file_path_str_async(
    path_str: &str,
    get_audio_output_handler: impl FnOnce() -> GetAudioOutputHandlerRet + Send + 'static,
    enable_gapless: bool,
    verify: bool,
    track_num: Option<usize>,
    seek: Option<f64>,
) -> Result<i32, PlaybackError> {
    let path_str = path_str.to_owned();
    moosicbox_task::spawn_blocking("symphonia_player: Play file path", move || {
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
    audio_output_handler: &mut AudioOutputHandler,
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
        audio_output_handler,
        enable_gapless,
        verify,
        track_num,
        seek,
    )
}

pub type GetAudioOutputHandlerRet = Result<AudioOutputHandler, PlaybackError>;

pub async fn play_media_source_async(
    media_source_stream: MediaSourceStream,
    hint: &Hint,
    get_audio_output_handler: impl FnOnce() -> GetAudioOutputHandlerRet + Send + 'static,
    enable_gapless: bool,
    verify: bool,
    track_num: Option<usize>,
    seek: Option<f64>,
) -> Result<i32, PlaybackError> {
    let hint = hint.clone();
    moosicbox_task::spawn_blocking("symphonia_player: Play media source", move || {
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
    audio_output_handler: &mut AudioOutputHandler,
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
            play(
                probed.format,
                audio_output_handler,
                track_num,
                seek_time,
                &decode_opts,
            )
        }
        Err(err) => {
            // The input was not supported by any format reader.
            log::info!("the input is not supported: {err:?}");
            Err(PlaybackError::Symphonia(err))
        }
    }
}

#[derive(Copy, Clone)]
struct PlayTrackOptions {
    track_id: u32,
    seek_ts: u64,
}

fn play(
    mut reader: Box<dyn FormatReader>,
    audio_output_handler: &mut AudioOutputHandler,
    track_num: Option<usize>,
    seek_time: Option<f64>,
    decode_opts: &DecoderOptions,
) -> Result<i32, PlaybackError> {
    // If the user provided a track number, select that track if it exists, otherwise, select the
    // first track with a known codec.
    let track = track_num
        .and_then(|t| reader.tracks().get(t))
        .or_else(|| first_supported_track(reader.tracks()));

    let mut track_id = match track {
        Some(track) => track.id,
        _ => return Ok(0),
    };

    log::debug!("Playing track_id={track_id}");

    // If there is a seek time, seek the reader to the time specified and get the timestamp of the
    // seeked position. All packets with a timestamp < the seeked position will not be played.
    //
    // Note: This is a half-baked approach to seeking! After seeking the reader, packets should be
    // decoded and *samples* discarded up-to the exact *sample* indicated by required_ts. The
    // current approach will discard excess samples if seeking to a sample within a packet.
    let seek_ts = if let Some(time) = seek_time {
        let seek_to = SeekTo::Time {
            time: Time::from(time),
            track_id: Some(track_id),
        };

        // Attempt the seek. If the seek fails, ignore the error and return a seek timestamp of 0 so
        // that no samples are trimmed.
        match reader.seek(SeekMode::Accurate, seek_to) {
            Ok(seeked_to) => seeked_to.required_ts,
            Err(Error::ResetRequired) => {
                track_id = first_supported_track(reader.tracks()).unwrap().id;
                0
            }
            Err(err) => {
                // Don't give-up on a seek error.
                log::warn!("seek error: {}", err);
                0
            }
        }
    } else {
        // If not seeking, the seek timestamp is 0.
        0
    };

    let mut track_info = PlayTrackOptions { track_id, seek_ts };

    let result = loop {
        match play_track(&mut reader, audio_output_handler, track_info, decode_opts) {
            Err(PlaybackError::Symphonia(Error::ResetRequired)) => {
                // Select the first supported track since the user's selected track number might no
                // longer be valid or make sense.
                let track_id = first_supported_track(reader.tracks()).unwrap().id;
                track_info = PlayTrackOptions {
                    track_id,
                    seek_ts: 0,
                };
            }
            res => break res,
        }
    };

    let result = if let Err(PlaybackError::AudioOutput(AudioOutputError::StreamEnd)) = result {
        Ok(0)
    } else {
        result
    };

    match result {
        Ok(code) => match code {
            2 => log::debug!("Aborted"),
            _ => {
                log::debug!("Attempting to get audio_output to flush");
                audio_output_handler.flush()?;
            }
        },
        Err(PlaybackError::AudioOutput(AudioOutputError::Interrupt)) => {
            log::info!("Audio interrupt detected. Not flushing");
        }
        Err(ref err) => {
            log::error!("Encountered error {err:?}");
        }
    };

    result
}

fn play_track(
    reader: &mut Box<dyn FormatReader>,
    audio_output_handler: &mut AudioOutputHandler,
    play_opts: PlayTrackOptions,
    decode_opts: &DecoderOptions,
) -> Result<i32, PlaybackError> {
    // Get the selected track using the track ID.
    let track = match reader
        .tracks()
        .iter()
        .find(|track| track.id == play_opts.track_id)
    {
        Some(track) => track,
        _ => return Ok(0),
    }
    .clone();

    // Create a decoder for the track.
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, decode_opts)?;

    // Decode and play the packets belonging to the selected track.
    let result = loop {
        if audio_output_handler
            .cancellation_token
            .as_ref()
            .is_some_and(|token| token.is_cancelled())
        {
            return Ok(2);
        }
        // Get the next packet from the format reader.
        let packet = match reader.next_packet() {
            Ok(packet) => packet,
            Err(err) => break Err(PlaybackError::Symphonia(err)),
        };

        // If the packet does not belong to the selected track, skip it.
        if packet.track_id() != play_opts.track_id {
            continue;
        }

        log::trace!("Decoding packet");
        // Decode the packet into audio samples.
        match decoder.decode(&packet) {
            Ok(decoded) => {
                log::trace!("Decoded packet");

                if audio_output_handler.contains_outputs_to_open() {
                    log::trace!("Getting audio spec");
                    // Get the audio buffer specification. This is a description of the decoded
                    // audio buffer's sample format and sample rate.
                    let spec = *decoded.spec();

                    // Get the capacity of the decoded buffer. Note that this is capacity, not
                    // length! The capacity of the decoded buffer is constant for the life of the
                    // decoder, but the length is not.
                    let duration = decoded.capacity() as u64;

                    audio_output_handler.try_open(spec, duration)?;
                }

                let ts = packet.ts();

                // Write the decoded audio samples to the audio output if the presentation timestamp
                // for the packet is >= the seeked position (0 if not seeking).
                if ts >= play_opts.seek_ts {
                    log::trace!("Writing decoded to audio output");
                    let mut buf = decoded.make_equivalent();
                    decoded.convert(&mut buf);
                    audio_output_handler.write(buf, &packet, &track)?;
                    log::trace!("Wrote decoded to audio output");
                } else {
                    log::trace!("Not to seeked position yet. Continuing decode");
                }
            }
            Err(Error::DecodeError(err)) => {
                // Decode errors are not fatal. Print the error message and try to decode the next
                // packet as usual.
                log::warn!("decode error: {}", err);
            }
            Err(err) => break Err(PlaybackError::Symphonia(err)),
        }
        log::trace!("Finished processing packet");
    };

    // Return if a fatal error occurred.
    ignore_end_of_stream_error(result)?;

    // Finalize the decoder and return the verification result if it's been enabled.
    do_verification(decoder.finalize())
}

fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
}

fn ignore_end_of_stream_error(result: Result<(), PlaybackError>) -> Result<(), PlaybackError> {
    match result {
        Err(PlaybackError::Symphonia(Error::IoError(err)))
            if err.kind() == std::io::ErrorKind::UnexpectedEof
                && err.to_string() == "end of stream" =>
        {
            // Do not treat "end of stream" as a fatal error. It's the currently only way a
            // format reader can indicate the media is complete.
            Ok(())
        }
        _ => result,
    }
}

fn do_verification(finalization: FinalizeResult) -> Result<i32, PlaybackError> {
    match finalization.verify_ok {
        Some(is_ok) => {
            // Got a verification result.
            log::debug!("verification: {}", if is_ok { "passed" } else { "failed" });

            Ok(i32::from(!is_ok))
        }
        // Verification not enabled by user, or unsupported by the codec.
        _ => Ok(0),
    }
}
