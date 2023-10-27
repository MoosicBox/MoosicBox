use symphonia::core::audio::{AudioBufferRef, SignalSpec};
use symphonia::core::units::Duration;

use std::ops::Deref;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{channel, Receiver};
use std::{cell::RefCell, rc::Rc, time::SystemTime};

use crate::output::pulseaudio::common::map_channels_to_pa_channelmap;
use crate::output::{AudioOutput, AudioOutputError, Result};

use pulse::context::{Context, FlagSet as ContextFlagSet};
use pulse::mainloop::standard::{IterateResult, Mainloop};
use pulse::proplist::Proplist;
use pulse::stream::{FlagSet as StreamFlagSet, Stream};
use symphonia::core::audio::*;

use libpulse_binding as pulse;

use log::{debug, error, info, trace};

pub struct PulseAudioOutput {
    mainloop: Rc<RefCell<Mainloop>>,
    stream: Rc<RefCell<pulse::stream::Stream>>,
    context: Rc<RefCell<pulse::context::Context>>,
    write_lock: Receiver<usize>,
    sample_buf: RawSampleBuffer<f32>,
    bytes: AtomicUsize,
}

impl PulseAudioOutput {
    pub fn try_open(spec: SignalSpec, duration: Duration) -> Result<Box<dyn AudioOutput>> {
        let pa = {
            // An interleaved buffer is required to send data to PulseAudio. Use a SampleBuffer to
            // move data between Symphonia AudioBuffers and the byte buffers required by PulseAudio.
            let sample_buf = RawSampleBuffer::<f32>::new(duration, spec);

            // Create a PulseAudio stream specification.
            let pa_spec = pulse::sample::Spec {
                format: pulse::sample::Format::FLOAT32NE,
                channels: spec.channels.count() as u8,
                rate: spec.rate,
            };

            assert!(pa_spec.is_valid());

            let pa_ch_map = map_channels_to_pa_channelmap(spec.channels);

            let mainloop = Rc::new(RefCell::new(
                Mainloop::new().expect("Failed to create mainloop"),
            ));

            let mut proplist = Proplist::new().unwrap();
            proplist
                .set_str(
                    pulse::proplist::properties::APPLICATION_NAME,
                    "MoosicBox Symphonia Player",
                )
                .unwrap();

            let context = Rc::new(RefCell::new(
                Context::new_with_proplist(mainloop.borrow().deref(), "FooAppContext", &proplist)
                    .expect("Failed to create new context"),
            ));

            context
                .borrow_mut()
                .connect(None, ContextFlagSet::NOFLAGS, None)
                .expect("Failed to connect context");

            wait_context(
                &mut mainloop.borrow_mut(),
                &mut context.borrow_mut(),
                pulse::context::State::Ready,
            )?;

            let stream = Rc::new(RefCell::new(
                Stream::new(
                    &mut context.borrow_mut(),
                    "Music",
                    &pa_spec,
                    pa_ch_map.as_ref(),
                )
                .expect("Failed to create new stream"),
            ));

            let (tx, rx) = channel();

            {
                let mut strm = stream.borrow_mut();
                strm.connect_playback(None, None, StreamFlagSet::START_CORKED, None, None)
                    .expect("Failed to connect playback");

                strm.set_moved_callback(Some(Box::new(|| {
                    debug!("MOVED");
                })));
                strm.set_started_callback(Some(Box::new(|| {
                    debug!("STARTED");
                })));
                strm.set_overflow_callback(Some(Box::new(|| {
                    debug!("OVERFLOW");
                })));
                strm.set_underflow_callback(Some(Box::new(|| {
                    debug!("UNDERFLOW");
                })));
                strm.set_write_callback(Some(Box::new(move |buf_size| {
                    debug!("WRITE: {buf_size:?}");
                    tx.send(buf_size).unwrap();
                })));
                strm.set_event_callback(Some(Box::new(|evt, _props| {
                    debug!("EVENT: {evt}");
                })));
                strm.set_suspended_callback(Some(Box::new(|| {
                    debug!("SUSPENDED");
                })));
                strm.set_latency_update_callback(Some(Box::new(|| {
                    debug!("LATENCY_UPDATE");
                })));
                strm.set_buffer_attr_callback(Some(Box::new(|| {
                    debug!("BUFFER_ATTR");
                })));
                strm.set_read_callback(Some(Box::new(|buf_size| {
                    debug!("READ: {buf_size}");
                })));
            }

            PulseAudioOutput {
                mainloop: mainloop.clone(),
                stream: stream.clone(),
                context: context.clone(),
                write_lock: rx, //write_lock.clone(),
                sample_buf,
                bytes: AtomicUsize::new(0),
            }
        };

        Ok(Box::new(pa))
    }
}

fn wait_context(
    mainloop: &mut Mainloop,
    context: &mut Context,
    expected_state: pulse::context::State,
) -> Result<()> {
    loop {
        match mainloop.iterate(false) {
            IterateResult::Quit(_) | IterateResult::Err(_) => {
                error!("Iterate state was not success, quitting...");
                return Err(AudioOutputError::StreamClosed);
            }
            IterateResult::Success(_) => {}
        }
        let state = context.get_state();
        if state == expected_state {
            break Ok(());
        }
        match state {
            pulse::context::State::Ready => {
                info!("Context is ready");
            }
            pulse::context::State::Failed => {
                error!("Context state failed, quitting...");
                return Err(AudioOutputError::StreamClosed);
            }
            pulse::context::State::Terminated => {
                error!("Context state terminated, quitting...");
                return Err(AudioOutputError::StreamClosed);
            }
            _ => {
                debug!("Context state {state:?}");
            }
        }
    }
}

fn wait_stream(
    mainloop: &mut Mainloop,
    stream: &mut Stream,
    expected_state: pulse::stream::State,
) -> Result<()> {
    loop {
        match mainloop.iterate(false) {
            IterateResult::Quit(_) | IterateResult::Err(_) => {
                error!("Iterate state was not success, quitting...");
                return Err(AudioOutputError::StreamClosed);
            }
            IterateResult::Success(_) => {}
        }
        let state = stream.get_state();
        if state == expected_state {
            break Ok(());
        }
        match state {
            pulse::stream::State::Ready => {
                info!("Stream is ready");
            }
            pulse::stream::State::Failed => {
                error!("Stream state failed, quitting...");
                return Err(AudioOutputError::StreamClosed);
            }
            pulse::stream::State::Terminated => {
                error!("Stream state terminated, quitting...");
                return Err(AudioOutputError::StreamClosed);
            }
            _ => {
                debug!("Stream state {state:?}");
            }
        }
    }
}

fn write_bytes(stream: &mut Stream, bytes: &[u8]) -> Result<usize> {
    let byte_count = bytes.len();
    let buffer = stream.begin_write(Some(byte_count)).unwrap().unwrap();
    buffer.copy_from_slice(bytes);

    let size_left = stream.writable_size().unwrap();
    // stream.begin_write(Some(byte_count)).unwrap();
    trace!("Writing to pulse audio {byte_count} bytes ({size_left} left)");
    let start = SystemTime::now();
    // Write interleaved samples to PulseAudio.
    match stream.write(buffer, None, 0, pulse::stream::SeekMode::Relative) {
        Err(err) => {
            error!("audio output stream write error: {}", err);

            Err(AudioOutputError::StreamClosed)
        }
        _ => {
            let end = SystemTime::now();
            let took_ms = end.duration_since(start).unwrap().as_millis();
            if took_ms >= 500 {
                error!("Detected audio interrupt");
                return Err(AudioOutputError::Interrupt);
            }

            if stream.is_corked().unwrap() {
                stream.uncork(None);
            }
            Ok(byte_count)
        }
    }
}

fn drain(mainloop: &mut Mainloop, stream: &mut Stream) -> Result<()> {
    info!("Draining...");
    // Wait for our data to be played
    let drained = Rc::new(RefCell::new(false));
    let _o = {
        let drain_state_ref = Rc::clone(&drained);
        trace!("Attempting drain");
        stream.drain(Some(Box::new(move |success: bool| {
            trace!("Drain success: {success}");
            *drain_state_ref.borrow_mut() = true;
        })))
    };
    while !(*drained.borrow_mut()) {
        match mainloop.iterate(false) {
            IterateResult::Quit(_) | IterateResult::Err(_) => {
                error!("Iterate state was not success, quitting...");
                return Err(AudioOutputError::StreamClosed);
            }
            IterateResult::Success(_) => {}
        }
    }
    *drained.borrow_mut() = false;
    info!("Drained.");
    Ok(())
}

impl AudioOutput for PulseAudioOutput {
    fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<usize> {
        let frame_count = decoded.frames();
        // Do nothing if there are no audio frames.
        if frame_count == 0 {
            trace!("No decoded frames. Returning");
            return Ok(0);
        }

        // Interleave samples from the audio buffer into the sample buffer.
        self.sample_buf.copy_interleaved_ref(decoded);

        // Wait for context to be ready
        wait_context(
            &mut self.mainloop.borrow_mut(),
            &mut self.context.borrow_mut(),
            pulse::context::State::Ready,
        )?;
        wait_stream(
            &mut self.mainloop.borrow_mut(),
            &mut self.stream.borrow_mut(),
            pulse::stream::State::Ready,
        )?;

        let mut bytes = self.sample_buf.as_bytes();
        let byte_count = bytes.len();
        let bytes_available = self.stream.borrow().writable_size().unwrap();

        debug!("{bytes_available} bytes available");
        let next_bytes = if bytes_available < byte_count {
            if bytes_available == 0 {
                debug!("Waiting for write lock...");
                let start = SystemTime::now();
                let _ = self
                    .write_lock
                    .recv_timeout(std::time::Duration::from_millis(140));
                let end = SystemTime::now();
                let took_ms = end.duration_since(start).unwrap().as_millis();
                debug!("Waiting for write lock took {took_ms}ms");
                None
            } else {
                let next_bytes = &bytes[bytes_available..];
                bytes = &bytes[..bytes_available];

                Some(next_bytes)
            }
        } else {
            None
        };

        let start = SystemTime::now();
        trace!("Writing bytes");
        let mut result = write_bytes(&mut self.stream.borrow_mut(), bytes)?;

        if let Some(next_bytes) = next_bytes {
            debug!("Writing second buffer");
            result += write_bytes(&mut self.stream.borrow_mut(), next_bytes)?;
        }

        let total_bytes = self
            .bytes
            .fetch_add(result, std::sync::atomic::Ordering::SeqCst)
            + result;

        let end = SystemTime::now();
        let took_ms = end.duration_since(start).unwrap().as_millis();
        trace!("Successfully wrote to pulse audio (total {total_bytes} bytes). Took {took_ms}ms");

        Ok(result)
    }

    fn flush(&mut self) -> Result<()> {
        drain(
            &mut self.mainloop.borrow_mut(),
            &mut self.stream.borrow_mut(),
        )
    }
}

impl Drop for PulseAudioOutput {
    fn drop(&mut self) {
        info!("Shutting PulseAudioOutput down");
        match self.stream.borrow_mut().disconnect() {
            Ok(()) => info!("Disconnected stream"),
            Err(err) => error!("Failed to disconnect stream: {err:?}"),
        };
        match wait_stream(
            &mut self.mainloop.borrow_mut(),
            &mut self.stream.borrow_mut(),
            pulse::stream::State::Terminated,
        ) {
            Ok(()) => info!("Terminated stream"),
            Err(err) => error!("Failed to terminate stream: {err:?}"),
        }

        self.context.borrow_mut().disconnect()
    }
}

pub fn try_open(spec: SignalSpec, duration: Duration) -> Result<Box<dyn AudioOutput>> {
    PulseAudioOutput::try_open(spec, duration)
}
