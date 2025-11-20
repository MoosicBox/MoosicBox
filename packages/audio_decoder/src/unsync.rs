//! Unsynchronized decoder API using channels.
//!
//! This module provides an alternative decoding API that decodes audio in a separate thread
//! and returns decoded buffers via a channel receiver. Unlike the main decoder API which uses
//! callbacks, this approach allows the caller to pull decoded audio at their own pace.

use flume::Receiver;
use symphonia::core::audio::AudioBuffer;
use symphonia::core::codecs::{CODEC_TYPE_NULL, CodecRegistry, DecoderOptions};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::units::Time;

#[cfg(feature = "opus")]
use moosicbox_opus::register_opus_codec;

use crate::{AudioDecodeError, DecodeError};

#[derive(Copy, Clone)]
struct PlayTrackOptions {
    track_id: u32,
    seek_ts: u64,
}

/// Decodes audio from a format reader, returning a channel receiver for decoded buffers.
///
/// This function spawns a separate thread to decode audio packets and sends the decoded
/// buffers through a channel, allowing the caller to consume audio at their own pace.
///
/// # Errors
///
/// * Returns [`DecodeError::AudioDecode`] if no supported track is found or decoding fails
/// * Returns [`DecodeError::Symphonia`] if reading packets or seeking fails
///
/// # Panics
///
/// * Panics if the reader requires reset but no supported track is available
#[cfg_attr(feature = "profiling", profiling::function)]
pub fn decode(
    mut reader: Box<dyn FormatReader>,
    track_num: Option<usize>,
    seek_time: Option<f64>,
    decode_opts: DecoderOptions,
) -> Result<Receiver<AudioBuffer<f32>>, DecodeError> {
    // If the user provided a track number, select that track if it exists, otherwise, select the
    // first track with a known codec.
    let track = track_num
        .and_then(|t| reader.tracks().get(t))
        .or_else(|| first_supported_track(reader.tracks()));

    let mut track_id = match track {
        Some(track) => track.id,
        _ => return Err(DecodeError::AudioDecode(AudioDecodeError::OpenStream)),
    };

    log::debug!("Playing track_id={track_id}");

    // If there is a seek time, seek the reader to the time specified and get the timestamp of the
    // seeked position. All packets with a timestamp < the seeked position will not be played.
    //
    // Note: This is a half-baked approach to seeking! After seeking the reader, packets should be
    // decoded and *samples* discarded up-to the exact *sample* indicated by required_ts. The
    // current approach will discard excess samples if seeking to a sample within a packet.
    let seek_ts = seek_time.map_or(0, |time| {
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
                log::warn!("seek error: {err}");
                0
            }
        }
    });

    let track_info = PlayTrackOptions { track_id, seek_ts };

    decode_track(reader, track_info, decode_opts)
}

/// Decodes a track and returns a channel receiver for decoded audio buffers.
///
/// This function spawns a background thread to perform decoding and sends decoded
/// buffers through a channel for consumption.
///
/// # Errors
///
/// * Returns [`DecodeError::AudioDecode`] if the track is not found
/// * Returns [`DecodeError::Symphonia`] if creating the codec fails
#[cfg_attr(feature = "profiling", profiling::function)]
#[allow(clippy::similar_names)]
fn decode_track(
    mut reader: Box<dyn FormatReader>,
    play_opts: PlayTrackOptions,
    decode_opts: DecoderOptions,
) -> Result<Receiver<AudioBuffer<f32>>, DecodeError> {
    let (sender, receiver) = flume::unbounded::<AudioBuffer<f32>>();

    // Get the selected track using the track ID.
    let track = reader
        .tracks()
        .iter()
        .find(|track| track.id == play_opts.track_id)
        .ok_or(DecodeError::AudioDecode(AudioDecodeError::StreamEnd))?
        .clone();

    // Create a decoder for the track.
    let codec_registry = {
        let mut registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut registry);

        #[cfg(feature = "opus")]
        register_opus_codec(&mut registry);

        registry
    };

    let mut decoder = codec_registry.make(&track.codec_params, &decode_opts)?;

    log::trace!("Spawning decoder loop");

    std::thread::spawn(move || {
        // Decode and play the packets belonging to the selected track.
        let result = loop {
            // Get the next packet from the format reader.
            let packet = match reader.next_packet() {
                Ok(packet) => packet,
                Err(err) => break Err(DecodeError::Symphonia(err)),
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
                        }

                        log::trace!("Wrote decoded to audio output");
                    } else {
                        log::trace!("Not to seeked position yet. Continuing decode");
                    }
                }
                Err(Error::DecodeError(err)) => {
                    // Decode errors are not fatal. Print the error message and try to decode the next
                    // packet as usual.
                    log::warn!("decode error: {err}");
                }
                Err(err) => break Err(DecodeError::Symphonia(err)),
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

/// Finds the first track with a supported codec.
///
/// Returns the first track that doesn't have a null codec type.
fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
}

/// Converts expected end-of-stream errors into success.
///
/// This function treats "end of stream" `UnexpectedEof` errors as successful completion,
/// while preserving other errors.
fn ignore_end_of_stream_error(result: Result<(), DecodeError>) -> Result<(), DecodeError> {
    match result {
        Err(DecodeError::Symphonia(Error::IoError(err)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use symphonia::core::codecs::CodecParameters;

    #[test]
    fn test_first_supported_track_empty() {
        let tracks: Vec<Track> = vec![];
        assert!(first_supported_track(&tracks).is_none());
    }

    #[test]
    fn test_first_supported_track_all_null() {
        let tracks = vec![
            Track::new(0, CodecParameters::new().for_codec(CODEC_TYPE_NULL).clone()),
            Track::new(1, CodecParameters::new().for_codec(CODEC_TYPE_NULL).clone()),
        ];
        assert!(first_supported_track(&tracks).is_none());
    }

    #[test]
    fn test_first_supported_track_finds_supported() {
        use symphonia::core::codecs::CODEC_TYPE_FLAC;

        let tracks = vec![
            Track::new(0, CodecParameters::new().for_codec(CODEC_TYPE_NULL).clone()),
            Track::new(1, CodecParameters::new().for_codec(CODEC_TYPE_FLAC).clone()),
            Track::new(2, CodecParameters::new().for_codec(CODEC_TYPE_FLAC).clone()),
        ];
        let result = first_supported_track(&tracks);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, 1);
    }

    #[test]
    fn test_ignore_end_of_stream_error_ok() {
        let result = ignore_end_of_stream_error(Ok(()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_ignore_end_of_stream_error_expected_eof() {
        let io_error = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "end of stream");
        let decode_error = DecodeError::Symphonia(Error::IoError(io_error));
        let result = ignore_end_of_stream_error(Err(decode_error));
        assert!(result.is_ok());
    }

    #[test]
    fn test_ignore_end_of_stream_error_unexpected_eof_different_message() {
        let io_error = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "unexpected");
        let decode_error = DecodeError::Symphonia(Error::IoError(io_error));
        let result = ignore_end_of_stream_error(Err(decode_error));
        assert!(result.is_err());
    }

    #[test]
    fn test_ignore_end_of_stream_error_other_error() {
        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let decode_error = DecodeError::Symphonia(Error::IoError(io_error));
        let result = ignore_end_of_stream_error(Err(decode_error));
        assert!(result.is_err());
    }
}
