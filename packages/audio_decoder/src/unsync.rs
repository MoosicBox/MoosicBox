use flume::Receiver;
use symphonia::core::audio::AudioBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::units::Time;

use crate::{AudioDecodeError, DecodeError};

#[derive(Copy, Clone)]
struct PlayTrackOptions {
    track_id: u32,
    seek_ts: u64,
}

/// # Panics
///
/// * If fails to get the first supported track
///
/// # Errors
///
/// * If the audio fails to decode
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
                log::warn!("seek error: {}", err);
                0
            }
        }
    });

    let track_info = PlayTrackOptions { track_id, seek_ts };

    decode_track(reader, track_info, decode_opts)
}

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
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &decode_opts)?;

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
                    log::warn!("decode error: {}", err);
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

fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
}

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
