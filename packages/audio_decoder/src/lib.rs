#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
#![warn(rust_2018_idioms)]
#![forbid(unsafe_code)]

use std::fs::File;
use std::path::Path;

use switchy_async::task::JoinError;
use switchy_async::util::CancellationToken;
use symphonia::core::audio::{AudioBuffer, SignalSpec};
use symphonia::core::codecs::{CODEC_TYPE_NULL, DecoderOptions, FinalizeResult};
use symphonia::core::errors::Error;
use symphonia::core::formats::{FormatOptions, FormatReader, Packet, SeekMode, SeekTo, Track};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::{Duration, Time};
use thiserror::Error;

pub mod media_sources;
pub mod unsync;

#[allow(dead_code)]
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Error)]
pub enum AudioDecodeError {
    #[error("OpenStreamError")]
    OpenStream,
    #[error("PlayStreamError")]
    PlayStream,
    #[error("StreamClosedError")]
    StreamClosed,
    #[error("StreamEndError")]
    StreamEnd,
    #[error("InterruptError")]
    Interrupt,
    #[error(transparent)]
    IO(#[from] std::io::Error),
    #[error(transparent)]
    Other(#[from] Box<dyn std::error::Error + Send + Sync>),
}

pub trait AudioDecode {
    /// # Errors
    ///
    /// * If the audio failed to decode
    fn decoded(
        &mut self,
        decoded: AudioBuffer<f32>,
        packet: &Packet,
        track: &Track,
    ) -> Result<(), AudioDecodeError>;

    /// # Errors
    ///
    /// * If the audio failed to flush
    fn flush(&mut self) -> Result<(), AudioDecodeError> {
        // Default implementation does nothing
        Ok(())
    }
}

type InnerType = Box<dyn AudioDecode>;
pub type OpenAudioDecodeHandler =
    Box<dyn FnMut(SignalSpec, Duration) -> Result<InnerType, AudioDecodeError> + Send>;
type AudioFilter =
    Box<dyn FnMut(&mut AudioBuffer<f32>, &Packet, &Track) -> Result<(), AudioDecodeError> + Send>;

pub struct AudioDecodeHandler {
    pub cancellation_token: Option<CancellationToken>,
    filters: Vec<AudioFilter>,
    open_decode_handlers: Vec<OpenAudioDecodeHandler>,
    outputs: Vec<InnerType>,
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl AudioDecodeHandler {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cancellation_token: None,
            filters: vec![],
            open_decode_handlers: vec![],
            outputs: vec![],
        }
    }

    #[must_use]
    pub fn with_filter(mut self, filter: AudioFilter) -> Self {
        self.filters.push(filter);
        self
    }

    #[must_use]
    pub fn with_output(mut self, open_output: OpenAudioDecodeHandler) -> Self {
        self.open_decode_handlers.push(open_output);
        self
    }

    #[must_use]
    pub fn with_cancellation_token(mut self, cancellation_token: CancellationToken) -> Self {
        self.cancellation_token.replace(cancellation_token);
        self
    }

    fn run_filters(
        &mut self,
        decoded: &mut AudioBuffer<f32>,
        packet: &Packet,
        track: &Track,
    ) -> Result<(), AudioDecodeError> {
        for filter in &mut self.filters {
            log::trace!("Running audio filter");
            filter(decoded, packet, track)?;
        }
        Ok(())
    }

    /// # Errors
    ///
    /// * If the audio failed to write
    pub fn write(
        &mut self,
        mut decoded: AudioBuffer<f32>,
        packet: &Packet,
        track: &Track,
    ) -> Result<(), AudioDecodeError> {
        self.run_filters(&mut decoded, packet, track)?;

        let len = self.outputs.len();

        for (i, output) in self.outputs.iter_mut().enumerate() {
            if i == len - 1 {
                output.decoded(decoded, packet, track)?;
                break;
            }

            output.decoded(decoded.clone(), packet, track)?;
        }

        Ok(())
    }

    /// # Errors
    ///
    /// * If the audio failed to write
    pub fn flush(&mut self) -> Result<(), AudioDecodeError> {
        let outputs_count = self.outputs.len();
        log::debug!("🔊 AudioDecodeHandler::flush() called - flushing {outputs_count} outputs");

        // Flush all audio outputs to ensure complete playback
        for (i, output) in self.outputs.iter_mut().enumerate() {
            log::debug!(
                "🔊 AudioDecodeHandler: flushing output {}/{}",
                i + 1,
                outputs_count
            );
            output.flush()?;
        }

        log::debug!("🔊 AudioDecodeHandler::flush() completed");
        Ok(())
    }

    #[must_use]
    pub fn contains_outputs_to_open(&self) -> bool {
        !self.open_decode_handlers.is_empty()
    }

    /// # Errors
    ///
    /// * If any of the `open_func`s fail to open the `Box<dyn AudioDecode>` decoders
    pub fn try_open(
        &mut self,
        spec: SignalSpec,
        duration: Duration,
    ) -> Result<(), AudioDecodeError> {
        for mut open_func in self.open_decode_handlers.drain(..) {
            self.outputs.push((*open_func)(spec, duration)?);
        }
        Ok(())
    }
}

impl Default for AudioDecodeHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl From<std::io::Error> for DecodeError {
    fn from(err: std::io::Error) -> Self {
        Self::Symphonia(Error::IoError(err))
    }
}

#[derive(Debug, Error)]
pub enum DecodeError {
    #[error(transparent)]
    AudioDecode(#[from] AudioDecodeError),
    #[error(transparent)]
    Symphonia(#[from] Error),
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error("No audio outputs")]
    NoAudioOutputs,
    #[error("Invalid source")]
    InvalidSource,
}

#[derive(Copy, Clone)]
struct PlayTrackOptions {
    track_id: u32,
    seek_ts: u64,
}

/// # Errors
///
/// * If the audio fails to decode
pub async fn decode_file_path_str_async(
    path_str: &str,
    get_audio_output_handler: impl FnOnce() -> GetAudioDecodeHandlerRet + Send + 'static,
    enable_gapless: bool,
    verify: bool,
    track_num: Option<usize>,
    seek: Option<f64>,
) -> Result<i32, DecodeError> {
    let path_str = path_str.to_owned();
    switchy_async::runtime::Handle::current()
        .spawn_blocking_with_name("audio_decoder: Play file path", move || {
            let mut handler = get_audio_output_handler()?;
            decode_file_path_str(
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

/// # Errors
///
/// * If the audio fails to decode
#[cfg_attr(feature = "profiling", profiling::function)]
#[allow(clippy::too_many_arguments)]
pub fn decode_file_path_str(
    path_str: &str,
    audio_output_handler: &mut AudioDecodeHandler,
    enable_gapless: bool,
    verify: bool,
    track_num: Option<usize>,
    seek: Option<f64>,
) -> Result<i32, DecodeError> {
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

    decode_media_source(
        mss,
        &hint,
        audio_output_handler,
        enable_gapless,
        verify,
        track_num,
        seek,
    )
}

pub type GetAudioDecodeHandlerRet = Result<AudioDecodeHandler, DecodeError>;

/// # Errors
///
/// * If the audio fails to decode
pub async fn decode_media_source_async(
    media_source_stream: MediaSourceStream,
    hint: &Hint,
    get_audio_output_handler: impl FnOnce() -> GetAudioDecodeHandlerRet + Send + 'static,
    enable_gapless: bool,
    verify: bool,
    track_num: Option<usize>,
    seek: Option<f64>,
) -> Result<i32, DecodeError> {
    let hint = hint.clone();
    switchy_async::runtime::Handle::current()
        .spawn_blocking_with_name("audio_decoder: Play media source", move || {
            let mut handler = get_audio_output_handler()?;
            decode_media_source(
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

#[cfg_attr(feature = "profiling", profiling::function)]
#[allow(clippy::too_many_arguments)]
fn decode_media_source(
    media_source_stream: MediaSourceStream,
    hint: &Hint,
    audio_output_handler: &mut AudioDecodeHandler,
    enable_gapless: bool,
    verify: bool,
    track_num: Option<usize>,
    seek: Option<f64>,
) -> Result<i32, DecodeError> {
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
            decode(
                probed.format,
                audio_output_handler,
                track_num,
                seek_time,
                decode_opts,
            )
        }
        Err(err) => {
            // The input was not supported by any format reader.
            log::info!("the input is not supported: {err:?}");
            Err(DecodeError::Symphonia(err))
        }
    }
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
    audio_output_handler: &mut AudioDecodeHandler,
    track_num: Option<usize>,
    seek_time: Option<f64>,
    decode_opts: DecoderOptions,
) -> Result<i32, DecodeError> {
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

    let mut track_info = PlayTrackOptions { track_id, seek_ts };

    let result = loop {
        match play_track(&mut reader, audio_output_handler, track_info, decode_opts) {
            Err(DecodeError::Symphonia(Error::ResetRequired)) => {
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

    let result = if matches!(
        result,
        Err(DecodeError::AudioDecode(AudioDecodeError::StreamEnd))
    ) {
        Ok(0)
    } else {
        result
    };

    match &result {
        Ok(code) => {
            if *code == 2 {
                log::debug!("🔊 AUDIO DECODE ABORTED - not flushing");
            } else {
                log::debug!("🔊 AUDIO DECODE COMPLETED - attempting to flush audio outputs");
                audio_output_handler.flush()?;
            }
        }
        Err(DecodeError::AudioDecode(AudioDecodeError::Interrupt)) => {
            log::debug!("🔊 AUDIO INTERRUPT DETECTED - not flushing");
        }
        Err(err) => {
            log::error!("🔊 AUDIO DECODE ERROR - not flushing: {err:?}");
        }
    }

    result
}

#[cfg_attr(feature = "profiling", profiling::function)]
#[allow(
    clippy::similar_names,
    clippy::cognitive_complexity,
    clippy::too_many_lines
)]
fn play_track(
    reader: &mut Box<dyn FormatReader>,
    audio_output_handler: &mut AudioDecodeHandler,
    play_opts: PlayTrackOptions,
    decode_opts: DecoderOptions,
) -> Result<i32, DecodeError> {
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
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &decode_opts)?;

    log::debug!(
        "Starting packet decode loop with verification={}",
        decode_opts.verify
    );
    let mut packet_count = 0;
    let mut decode_errors = 0;

    // Decode and play the packets belonging to the selected track.
    let result = loop {
        #[cfg(feature = "profiling")]
        profiling::function_scope!("decoder loop");

        if audio_output_handler
            .cancellation_token
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
        {
            log::debug!("Decoder loop cancelled via cancellation token");
            return Ok(2);
        }

        // Get the next packet from the format reader.
        let packet = {
            #[cfg(feature = "profiling")]
            profiling::function_scope!("read");

            match reader.next_packet() {
                Ok(packet) => {
                    packet_count += 1;
                    log::trace!(
                        "Successfully read packet #{packet_count} for track {}",
                        packet.track_id()
                    );
                    packet
                }
                Err(err) => {
                    log::debug!("Failed to read next packet after {packet_count} packets: {err:?}");
                    // Check if this is an expected end-of-stream vs unexpected error
                    match &err {
                        Error::IoError(io_err)
                            if io_err.kind() == std::io::ErrorKind::UnexpectedEof =>
                        {
                            log::debug!("Received UnexpectedEof - stream appears to be finished");
                        }
                        Error::ResetRequired => {
                            log::debug!("Received ResetRequired");
                        }
                        _ => {
                            log::warn!("Unexpected reader error: {err:?}");
                        }
                    }
                    break Err(DecodeError::Symphonia(err));
                }
            }
        };

        // If the packet does not belong to the selected track, skip it.
        if packet.track_id() != play_opts.track_id {
            log::trace!(
                "Skipping packet for track {} (want track {})",
                packet.track_id(),
                play_opts.track_id
            );
            continue;
        }

        let decoded = {
            #[cfg(feature = "profiling")]
            profiling::function_scope!("decode");
            log::trace!("Decoding packet");

            decoder.decode(&packet)
        };

        // Decode the packet into audio samples.
        match decoded {
            Ok(decoded) => {
                log::trace!(
                    "Decoded packet - frames: {}, spec: {:?}",
                    decoded.frames(),
                    decoded.spec()
                );

                if audio_output_handler.contains_outputs_to_open() {
                    #[cfg(feature = "profiling")]
                    profiling::function_scope!("open audio output handler");

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
                    log::trace!(
                        "Writing decoded to audio output - ts: {ts}, seek_ts: {}",
                        play_opts.seek_ts
                    );
                    let mut buf = {
                        #[cfg(feature = "profiling")]
                        profiling::function_scope!("make_equivalent");

                        decoded.make_equivalent()
                    };
                    {
                        #[cfg(feature = "profiling")]
                        profiling::function_scope!("convert");

                        decoded.convert(&mut buf);
                    }
                    {
                        #[cfg(feature = "profiling")]
                        profiling::function_scope!("write");

                        audio_output_handler.write(buf, &packet, &track)?;
                    }
                    log::trace!("Wrote decoded to audio output");
                } else {
                    log::trace!(
                        "Not to seeked position yet. Continuing decode - ts: {ts}, seek_ts: {}",
                        play_opts.seek_ts
                    );
                }
            }
            Err(Error::DecodeError(err)) => {
                // Decode errors are not fatal. Print the error message and try to decode the next
                // packet as usual.
                decode_errors += 1;
                log::warn!("decode error #{decode_errors}: {err}");
            }
            Err(err) => {
                log::debug!("Fatal decode error after {packet_count} packets: {err:?}");
                break Err(DecodeError::Symphonia(err));
            }
        }
        log::trace!("Finished processing packet #{packet_count}");
    };

    log::debug!(
        "Decode loop finished - processed {packet_count} packets, {decode_errors} decode errors, result: {result:?}"
    );

    // Return if a fatal error occurred.
    ignore_end_of_stream_error(result)?;

    log::debug!("Starting decoder finalization for verification");
    let finalization_result = decoder.finalize();
    log::debug!(
        "Decoder finalized - verify_ok: {:?}",
        finalization_result.verify_ok
    );

    // Finalize the decoder and return the verification result if it's been enabled.
    Ok(do_verification(finalization_result))
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
            log::debug!("Ignoring expected 'end of stream' UnexpectedEof error");
            Ok(())
        }
        Err(DecodeError::Symphonia(Error::IoError(err)))
            if err.kind() == std::io::ErrorKind::UnexpectedEof =>
        {
            log::debug!(
                "Received UnexpectedEof with message: '{err}' - NOT ignoring (not 'end of stream')"
            );
            Err(DecodeError::Symphonia(Error::IoError(err)))
        }
        Err(err) => {
            log::debug!("Received non-EOF error: {err:?}");
            Err(err)
        }
        Ok(()) => {
            log::debug!("No error to ignore");
            Ok(())
        }
    }
}

fn do_verification(finalization: FinalizeResult) -> i32 {
    finalization.verify_ok.map_or_else(|| {
        log::debug!("verification: no verification performed (verify_ok is None)");
        0
    }, |is_ok| {
        // Got a verification result.
        log::debug!(
            "verification result received: {}",
            if is_ok { "passed" } else { "failed" }
        );
        if !is_ok {
            log::warn!(
                "Verification failed - this may indicate data corruption, incomplete stream processing, or premature stream termination"
            );
        }
        i32::from(!is_ok)
    })
}
