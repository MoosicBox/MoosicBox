use std::cell::RefCell;

use crate::output::{AudioOutput, AudioOutputError};
use crate::resampler::Resampler;

use log::{debug, info};
use symphonia::core::audio::AudioBufferRef;
use symphonia::core::audio::*;
use symphonia::core::conv::ReversibleSample;
use symphonia::core::units::Duration;

const STEREO_10MS: usize = 48000 * 2 * 10 / 1000;
const STEREO_MIN: usize = 240;

pub struct OpusEncoder<T>
where
    T: ReversibleSample<f32> + 'static,
{
    output: RefCell<Vec<u8>>,
    buf: [f32; STEREO_10MS],
    pos: usize,
    time: usize,
    resampler: Resampler<T>,
}

impl<T> OpusEncoder<T>
where
    T: ReversibleSample<f32> + 'static,
{
    pub fn try_open(
        spec: SignalSpec,
        duration: Duration,
    ) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
        info!("Resampling {} Hz to {} Hz", spec.rate, 48000);
        Ok(Box::new(OpusEncoder {
            output: RefCell::new(vec![]),
            buf: [0.0; STEREO_10MS],
            pos: 0,
            time: 0,
            resampler: Resampler::<T>::new(spec, 48000_usize, duration),
        }))
    }
}

fn write_output(input: &[f32], output: &RefCell<Vec<u8>>, buf_size: usize, time: usize) {
    let mut read = 0;
    let mut written = 0;
    let output_buf = &mut vec![0_u8; buf_size];
    loop {
        match moosicbox_converter::encode_opus_float(
            &input[read..read + buf_size],
            &mut output_buf[written..],
        ) {
            Ok(info) => {
                output
                    .borrow_mut()
                    .extend_from_slice(&output_buf[written..written + info.output_size]);
                read += info.input_consumed;
                written += info.output_size;
                if time % 1000 == 0 {
                    debug!(
                        "Info: read={} written={} input_consumed={} output_size={} len={}",
                        read,
                        written,
                        info.input_consumed,
                        info.output_size,
                        output.borrow().len()
                    );
                }

                if read >= input.len() {
                    break;
                }
            }
            Err(err) => {
                panic!("Failed to convert: {err:?}");
            }
        }
    }
}

impl<T> AudioOutput for OpusEncoder<T>
where
    T: ReversibleSample<f32> + 'static,
{
    fn write(&mut self, decoded: AudioBufferRef<'_>) -> Result<usize, AudioOutputError> {
        let decoded = self
            .resampler
            .resample(decoded)
            .ok_or(AudioOutputError::StreamEnd)?;

        let n_samples = decoded.len();

        for sample in decoded.iter() {
            if self.pos == STEREO_10MS {
                self.time += 10;
                write_output(&self.buf, &self.output, STEREO_10MS, self.time);
                self.pos = 0;
                if self.time % 1000 == 0 {
                    debug!("time: {}", self.time / 1000);
                }
            }
            self.buf[self.pos] = (*sample).into_sample();
            self.pos += 1;
        }

        if self.pos == STEREO_10MS {
            self.time += 10;
            write_output(&self.buf, &self.output, STEREO_10MS, self.time);
            self.pos = 0;
            if self.time % 1000 == 0 {
                debug!("time: {}", self.time / 1000);
            }
        }

        Ok(n_samples)
    }

    fn flush(&mut self) -> Result<(), AudioOutputError> {
        debug!("Flushing");
        let decoded = self.resampler.flush().ok_or(AudioOutputError::StreamEnd)?;

        for sample in decoded.iter() {
            if self.pos == STEREO_10MS {
                self.time += 10;
                write_output(&self.buf, &self.output, STEREO_10MS, self.time);
                self.pos = 0;
                if self.time % 1000 == 0 {
                    debug!("time: {}", self.time / 1000);
                }
            }
            self.buf[self.pos] = (*sample).into_sample();
            self.pos += 1;
        }

        if self.pos == STEREO_10MS {
            self.time += 10;
            write_output(&self.buf, &self.output, STEREO_10MS, self.time);
            self.pos = 0;
            if self.time % 1000 == 0 {
                debug!("time: {}", self.time / 1000);
            }
        }

        if self.pos > 0 {
            write_output(&self.buf, &self.output, STEREO_MIN, self.time);
        }

        Ok(())
    }
}

pub fn try_open(
    spec: SignalSpec,
    duration: Duration,
) -> Result<Box<dyn AudioOutput>, AudioOutputError> {
    OpusEncoder::<i16>::try_open(spec, duration)
}
