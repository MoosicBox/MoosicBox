use std::io;

use flume::Receiver;
use symphonia::core::audio::AudioBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;
use thiserror::Error;

use crate::output::AudioOutputError;

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
            play(probed.format, track_num, seek_time, &decode_opts)
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
    track_num: Option<usize>,
    seek_time: Option<f64>,
    decode_opts: &DecoderOptions,
) -> Result<Receiver<AudioBuffer<f32>>, PlaybackError> {
    // If the user provided a track number, select that track if it exists, otherwise, select the
    // first track with a known codec.
    let track = track_num
        .and_then(|t| reader.tracks().get(t))
        .or_else(|| first_supported_track(reader.tracks()));

    let mut track_id = match track {
        Some(track) => track.id,
        _ => return Err(PlaybackError::AudioOutput(AudioOutputError::OpenStream)),
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

    let track_info = PlayTrackOptions { track_id, seek_ts };

    play_track(reader, track_info, decode_opts)
}

fn play_track(
    mut reader: Box<dyn FormatReader>,
    play_opts: PlayTrackOptions,
    decode_opts: &DecoderOptions,
) -> Result<Receiver<AudioBuffer<f32>>, PlaybackError> {
    let (sender, receiver) = flume::unbounded::<AudioBuffer<f32>>();

    // Get the selected track using the track ID.
    let track = reader
        .tracks()
        .iter()
        .find(|track| track.id == play_opts.track_id)
        .ok_or(PlaybackError::AudioOutput(AudioOutputError::StreamEnd))?
        .clone();

    // Create a decoder for the track.
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, decode_opts)?;

    log::trace!("Spawning decoder loop");

    std::thread::spawn(move || {
        // Decode and play the packets belonging to the selected track.
        let result = loop {
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

                    let ts = packet.ts();

                    // Write the decoded audio samples to the audio output if the presentation timestamp
                    // for the packet is >= the seeked position (0 if not seeking).
                    if ts >= play_opts.seek_ts {
                        log::debug!("Writing {} frames", decoded.frames());
                        let mut buf = decoded.make_equivalent();
                        decoded.convert(&mut buf);
                        if let Err(err) = sender.send(buf) {
                            log::error!("Receiver dropped: {err:?}");
                            break Ok(());
                        } else {
                            log::trace!("Wrote decoded to audio output");
                        }
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

        if decoder.finalize().verify_ok.is_some_and(|is_ok| is_ok) {
            log::debug!("verification: passed");
        } else {
            log::debug!("verification: failed");
        }

        ignore_end_of_stream_error(result)
    });

    log::trace!("Returning AudioBuffer stream");

    Ok(receiver)
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
