//! Unsynchronized decoder API using channels.
//!
//! This module provides an alternative decoding API that decodes audio in a separate thread
//! and returns decoded buffers via a channel receiver. Unlike the main decoder API which uses
//! callbacks, this approach allows the caller to pull decoded audio at their own pace.

use flume::Receiver;
use symphonia::core::audio::AudioBuffer;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::codecs::audio::{AudioDecoderOptions, CODEC_ID_NULL_AUDIO};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::units::{Time, Timestamp};

use crate::{AudioDecodeError, DecodeError, DecodeOptions};

#[derive(Copy, Clone)]
struct PlayTrackOptions {
    track_id: u32,
    seek_ts: Option<Timestamp>,
}

/// Decodes audio from a format reader, returning a channel receiver for decoded buffers.
///
/// This function spawns a separate thread to decode audio packets and sends the decoded
/// buffers through a channel, allowing the caller to consume audio at their own pace.
///
/// # Examples
///
/// Most callers should use [`crate::decode_file_path_str`] for simple file decoding.
/// Use this function when you need pull-based buffer consumption via a channel receiver.
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
    reader: Box<dyn FormatReader>,
    track_num: Option<usize>,
    seek_time: Option<f64>,
    decode_opts: AudioDecoderOptions,
) -> Result<Receiver<AudioBuffer<f32>>, DecodeError> {
    decode_with_options(
        reader,
        track_num,
        seek_time,
        DecodeOptions {
            decoder: decode_opts,
            ..DecodeOptions::default()
        },
    )
}

/// Decode using an explicit backend and decoder options.
///
/// # Errors
/// * Returns decoding/seek errors and an error if the selected backend is unavailable.
///
/// # Panics
/// * Panics if a reset reader no longer contains a supported track.
pub fn decode_with_options(
    reader: Box<dyn FormatReader>,
    track_num: Option<usize>,
    seek_time: Option<f64>,
    options: DecodeOptions,
) -> Result<Receiver<AudioBuffer<f32>>, DecodeError> {
    decode_with_options_impl(reader, track_num, seek_time, options, Result::ok)
}

/// Decode audio with terminal worker failures delivered after any successful PCM buffers.
///
/// The outer result reports initialization/seek failures. The receiver emits `Ok` PCM
/// buffers followed by at most one terminal `Err`; successful completion closes it.
/// Recoverable corrupt-packet errors retain the existing log-and-continue policy.
///
/// # Errors
/// * Returns track selection, seeking, or decoder initialization errors before spawning.
///
/// # Panics
/// * Panics if a reset reader no longer contains a supported track.
pub fn decode_with_options_result(
    reader: Box<dyn FormatReader>,
    track_num: Option<usize>,
    seek_time: Option<f64>,
    options: DecodeOptions,
) -> Result<Receiver<Result<AudioBuffer<f32>, DecodeError>>, DecodeError> {
    decode_with_options_impl(reader, track_num, seek_time, options, Some)
}

fn decode_with_options_impl<T: Send + 'static>(
    mut reader: Box<dyn FormatReader>,
    track_num: Option<usize>,
    seek_time: Option<f64>,
    options: DecodeOptions,
    map: fn(Result<AudioBuffer<f32>, DecodeError>) -> Option<T>,
) -> Result<Receiver<T>, DecodeError> {
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

    // An explicit seek trims decoded frames, not whole packets. Without a seek,
    // retain valid decoder output even when encoder delay makes packet PTS negative.
    let seek_ts = if let Some(time) = seek_time {
        let seek_to = SeekTo::Time {
            time: Time::try_from_secs_f64(time).ok_or(DecodeError::InvalidSource)?,
            track_id: Some(track_id),
        };

        // Attempt the seek. If the seek fails, ignore the error and return no seek target so
        // that no samples are trimmed.
        match reader.seek(SeekMode::Accurate, seek_to) {
            Ok(seeked_to) => Some(seeked_to.required_ts),
            Err(Error::ResetRequired) => {
                track_id = first_supported_track(reader.tracks()).unwrap().id;
                None
            }
            Err(err) => {
                // Don't give-up on a seek error.
                log::warn!("seek error: {err}");
                None
            }
        }
    } else {
        None
    };

    let track_info = PlayTrackOptions { track_id, seek_ts };

    decode_track(reader, track_info, options, map)
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
fn decode_track<T: Send + 'static>(
    mut reader: Box<dyn FormatReader>,
    play_opts: PlayTrackOptions,
    options: DecodeOptions,
    map: fn(Result<AudioBuffer<f32>, DecodeError>) -> Option<T>,
) -> Result<Receiver<T>, DecodeError> {
    let decode_opts = options.decoder;
    let (sender, receiver) = flume::unbounded::<T>();

    // Get the selected track using the track ID.
    let track = reader
        .tracks()
        .iter()
        .find(|track| track.id == play_opts.track_id)
        .ok_or(DecodeError::AudioDecode(AudioDecodeError::StreamEnd))?
        .clone();

    let mut decoder = crate::options::make_decoder(&track, options)?;

    log::trace!("Spawning decoder loop");

    std::thread::spawn(move || {
        // Decode and play the packets belonging to the selected track.
        let result = loop {
            // Get the next packet from the format reader.
            let packet = match reader.next_packet() {
                Ok(None) => break Ok(()),
                Ok(Some(packet)) => packet,
                Err(err) => break Err(DecodeError::Symphonia(err)),
            };

            // If the packet does not belong to the selected track, skip it.
            if packet.track_id != play_opts.track_id {
                continue;
            }

            log::trace!("Decoding packet");
            // Decode the packet into audio samples.
            match decoder.decode(&packet) {
                Ok(decoded) => {
                    log::trace!("Decoded packet");

                    let skip = crate::seek_frames(
                        &packet,
                        &track,
                        play_opts.seek_ts,
                        decoded.spec().rate(),
                        decoded.frames(),
                        decode_opts.gapless,
                    );

                    // Write the decoded audio samples to the audio output if the presentation timestamp
                    // for the packet is >= the seeked position (0 if not seeking).
                    if skip < decoded.frames() {
                        log::debug!("Writing {} frames", decoded.frames());
                        let mut buf = AudioBuffer::new(decoded.spec().clone(), decoded.capacity());
                        buf.resize_uninit(decoded.frames());
                        decoded.copy_to(&mut buf);
                        buf.trim(skip, 0);
                        if let Some(item) = map(Ok(buf))
                            && sender.send(item).is_err()
                        {
                            log::debug!("Receiver dropped");
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

        if let Err(error) = ignore_end_of_stream_error(result) {
            log::error!("Decode worker failed: {error}");
            if let Some(item) = map(Err(error)) {
                let _ = sender.send(item);
            }
        }
    });

    log::trace!("Returning AudioBuffer stream");

    Ok(receiver)
}

/// Finds the first track with a supported codec.
///
/// Returns the first track that doesn't have a null codec type.
fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks.iter().find(|t| {
        t.codec_params
            .as_ref()
            .and_then(CodecParameters::audio)
            .is_some_and(|params| params.codec != CODEC_ID_NULL_AUDIO)
    })
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
    use symphonia::core::audio::Audio;
    use symphonia::core::formats::{FormatOptions, probe::Hint};
    use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
    use symphonia::core::meta::MetadataOptions;

    use super::*;
    use pretty_assertions::assert_eq;
    use symphonia::core::codecs::audio::{AudioCodecParameters, well_known::CODEC_ID_FLAC};

    #[test_log::test]
    fn test_first_supported_track_empty() {
        let tracks: Vec<Track> = vec![];
        assert!(first_supported_track(&tracks).is_none());
    }

    #[test_log::test]
    fn test_first_supported_track_all_null() {
        let tracks = vec![
            Track::new(0)
                .with_codec_params(CodecParameters::Audio(
                    AudioCodecParameters::new()
                        .for_codec(CODEC_ID_NULL_AUDIO)
                        .clone(),
                ))
                .clone(),
            Track::new(1)
                .with_codec_params(CodecParameters::Audio(
                    AudioCodecParameters::new()
                        .for_codec(CODEC_ID_NULL_AUDIO)
                        .clone(),
                ))
                .clone(),
        ];
        assert!(first_supported_track(&tracks).is_none());
    }

    #[test_log::test]
    fn test_first_supported_track_finds_supported() {
        let tracks = vec![
            Track::new(0)
                .with_codec_params(CodecParameters::Audio(
                    AudioCodecParameters::new()
                        .for_codec(CODEC_ID_NULL_AUDIO)
                        .clone(),
                ))
                .clone(),
            Track::new(1)
                .with_codec_params(CodecParameters::Audio(
                    AudioCodecParameters::new().for_codec(CODEC_ID_FLAC).clone(),
                ))
                .clone(),
            Track::new(2)
                .with_codec_params(CodecParameters::Audio(
                    AudioCodecParameters::new().for_codec(CODEC_ID_FLAC).clone(),
                ))
                .clone(),
        ];
        let result = first_supported_track(&tracks);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, 1);
    }

    #[test_log::test]
    fn test_ignore_end_of_stream_error_ok() {
        let result = ignore_end_of_stream_error(Ok(()));
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_ignore_end_of_stream_error_expected_eof() {
        let io_error = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "end of stream");
        let decode_error = DecodeError::Symphonia(Error::IoError(io_error));
        let result = ignore_end_of_stream_error(Err(decode_error));
        assert!(result.is_ok());
    }

    #[test_log::test]
    fn test_ignore_end_of_stream_error_unexpected_eof_different_message() {
        let io_error = std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "unexpected");
        let decode_error = DecodeError::Symphonia(Error::IoError(io_error));
        let result = ignore_end_of_stream_error(Err(decode_error));
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_ignore_end_of_stream_error_other_error() {
        let io_error = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "access denied");
        let decode_error = DecodeError::Symphonia(Error::IoError(io_error));
        let result = ignore_end_of_stream_error(Err(decode_error));
        assert!(result.is_err());
    }

    #[test_log::test]
    fn test_ignore_end_of_stream_error_audio_decode_error() {
        // Test that non-Symphonia DecodeError variants pass through unchanged
        let audio_decode_error = DecodeError::AudioDecode(AudioDecodeError::StreamEnd);
        let result = ignore_end_of_stream_error(Err(audio_decode_error));
        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(DecodeError::AudioDecode(AudioDecodeError::StreamEnd))
        ));
    }

    #[test_log::test]
    fn test_ignore_end_of_stream_error_no_audio_outputs() {
        let error = DecodeError::NoAudioOutputs;
        let result = ignore_end_of_stream_error(Err(error));
        assert!(result.is_err());
        assert!(matches!(result, Err(DecodeError::NoAudioOutputs)));
    }

    #[test_log::test]
    fn test_ignore_end_of_stream_error_invalid_source() {
        let error = DecodeError::InvalidSource;
        let result = ignore_end_of_stream_error(Err(error));
        assert!(result.is_err());
        assert!(matches!(result, Err(DecodeError::InvalidSource)));
    }

    #[test_log::test]
    fn test_decode_pcm_preserves_samples_and_handles_end_of_stream() {
        // Two signed PCM samples in a mono 16-bit WAV stream.
        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&40_u32.to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&8000_u32.to_le_bytes());
        wav.extend_from_slice(&16000_u32.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&4_u32.to_le_bytes());
        wav.extend_from_slice(&16384_i16.to_le_bytes());
        wav.extend_from_slice(&(-16384_i16).to_le_bytes());
        let stream = MediaSourceStream::new(
            Box::new(std::io::Cursor::new(wav)),
            MediaSourceStreamOptions::default(),
        );
        let reader = symphonia::default::get_probe()
            .probe(
                &Hint::new(),
                stream,
                FormatOptions::default(),
                MetadataOptions::default(),
            )
            .unwrap();
        let receiver = decode(reader, None, None, AudioDecoderOptions::default()).unwrap();
        let samples: Vec<f32> = receiver
            .iter()
            .flat_map(|buffer| {
                assert_eq!(buffer.spec().rate(), 8000);
                buffer.plane(0).unwrap().to_vec()
            })
            .collect();
        assert_eq!(samples, [0.5, -0.5]);
    }

    #[test_log::test]
    fn test_first_supported_track_first_track_is_supported() {
        let tracks = vec![
            Track::new(0)
                .with_codec_params(CodecParameters::Audio(
                    AudioCodecParameters::new().for_codec(CODEC_ID_FLAC).clone(),
                ))
                .clone(),
            Track::new(1)
                .with_codec_params(CodecParameters::Audio(
                    AudioCodecParameters::new()
                        .for_codec(CODEC_ID_NULL_AUDIO)
                        .clone(),
                ))
                .clone(),
        ];
        let result = first_supported_track(&tracks);
        assert!(result.is_some());
        assert_eq!(result.unwrap().id, 0);
    }
}
