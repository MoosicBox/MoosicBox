//! File-only qualification of public native APIs; no device or C codec is used.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "celt")]
mod implementation {
    use std::error::Error;
    use std::fs::File;
    use std::io::{BufReader, BufWriter, Read, Write};
    use std::path::Path;
    use std::time::Instant;

    use moosicbox_opus_native::{Channels, Decoder, Encoder, EncoderConfig, SampleRate, framing};

    type ProbeResult<T> = Result<T, Box<dyn Error>>;

    // Every record is a little-endian u32 byte length followed by that many bytes.
    // This preserves packet boundaries (including empty packets) in the fingerprint.
    fn record(writer: &mut impl Write, bytes: &[u8]) -> ProbeResult<()> {
        writer.write_all(&u32::try_from(bytes.len())?.to_le_bytes())?;
        writer.write_all(bytes)?;
        Ok(())
    }

    /// Exercise the deterministic malformed corpus and record framing/decode outcomes.
    ///
    /// # Errors
    /// * Returns codec or filesystem errors, or an error if impossible framing is accepted.
    pub fn malformed(directory: &Path) -> ProbeResult<()> {
        let mut packets = BufWriter::new(File::create(directory.join("corpus.packets"))?);
        let mut outcomes = BufWriter::new(File::create(directory.join("corpus.outcomes"))?);
        let mut details = BufWriter::new(File::create(directory.join("corpus.details"))?);
        let mut pcm = vec![0_i16; 11_520];
        // Structurally impossible framing, not merely undecodable compressed data.
        let invalid: &[&[u8]] = &[
            &[],
            &[0xfb],
            &[0xfb, 0],
            &[0xfb, 49],
            &[0xf9, 0],
            &[0xfa],
            &[0xfa, 252],
            &[0xfb, 0x41, 255],
        ];
        for packet in invalid {
            let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo)?;
            if framing::parse_frames(packet).is_ok()
                || decoder.decode(Some(packet), &mut pcm, false).is_ok()
            {
                return Err(format!("malformed framing accepted: {packet:?}").into());
            }
            record(&mut packets, packet)?;
            outcomes.write_all(b"invalid\n")?;
        }
        // Fixed, bounded fuzz-style corpus: every TOC, all framing codes, padding,
        // truncation and length boundaries. Errors are legitimate; panics are not.
        for toc in 0..=255_u8 {
            for length in [1, 2, 3, 4, 7, 16, 255, 256, 1276] {
                let mut packet = vec![0_u8; length];
                packet[0] = toc;
                let mut state = u32::from(toc) + 1;
                for byte in &mut packet[1..] {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    *byte = state.to_le_bytes()[3];
                }
                let parsed = framing::parse_frames(&packet).is_ok();
                let mut decoder = Decoder::new(SampleRate::Hz48000, Channels::Stereo)?;
                let outcome = decoder.decode(Some(&packet), &mut pcm, false);
                record(&mut packets, &packet)?;
                // Do not hash platform-dependent error wording.
                let count = outcome.unwrap_or(0);
                writeln!(&mut outcomes, "{parsed}:{count}")?;
                writeln!(&mut details, "{toc:02x},{length},{parsed},{count}")?;
            }
        }
        packets.flush()?;
        outcomes.flush()?;
        details.flush()?;
        Ok(())
    }

    /// Encode and decode the deterministic integer-origin stream, including flush packets.
    ///
    /// # Errors
    /// * Returns codec, conversion or filesystem errors, or a duration/nonzero-signal failure.
    pub fn stream(
        directory: &Path,
        mode: &str,
        hz: u32,
        channels: usize,
        frames: usize,
    ) -> ProbeResult<()> {
        let name = format!("{mode}-{hz}-{channels}-{frames}");
        let mut packets = BufWriter::new(File::create(directory.join(format!("{name}.packets")))?);
        let mut pcm_file = BufWriter::new(File::create(directory.join(format!("{name}.pcm")))?);
        let mut signal = BufWriter::new(File::create(directory.join(format!("{name}.input")))?);
        let rate = SampleRate::from_hz(hz)?;
        let channel_mode = if channels == 1 {
            Channels::Mono
        } else {
            Channels::Stereo
        };
        let frame_size = usize::try_from(hz / 50)?;
        let config = EncoderConfig {
            sample_rate: rate,
            channels: channel_mode,
            bitrate: match mode {
                "silk" => 64_000,
                "hybrid" => 192_000,
                _ => 96_000,
            },
            frame_size,
        };
        let mut encoder = match mode {
            #[cfg(feature = "silk")]
            "silk" => Encoder::new_silk(config)?,
            #[cfg(feature = "hybrid")]
            "hybrid" => Encoder::new_hybrid(config)?,
            _ => Encoder::new(config)?,
        };
        let mut decoder = Decoder::new(rate, channel_mode)?;
        let mut input = vec![0_f32; frame_size * channels];
        let mut output = vec![0_i16; 5760 * channels];
        let mut state = 0x1234_5678_u32;
        let mut encode_ns = 0_u128;
        let mut decode_ns = 0_u128;
        let mut samples = 0_usize;
        let mut packet_count = 0_usize;
        let mut nonzero = false;
        let started = Instant::now();
        for frame in 0..frames {
            for (index, value) in input.iter_mut().enumerate() {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = i32::from(state.to_le_bytes()[3]) - 128;
                let position = (frame * frame_size + index / channels) % 256;
                let triangle = i32::try_from(position.min(256 - position))? - 64;
                let integer = i16::try_from(triangle * 128 + noise * 8)?;
                signal.write_all(&integer.to_le_bytes())?;
                // Power-of-two scaling is exactly representable in f32. No sin/libm.
                *value = f32::from(integer) / 32768.0;
            }
            let clock = Instant::now();
            let packet = encoder.encode(&input)?;
            encode_ns += clock.elapsed().as_nanos();
            let clock = Instant::now();
            let count = decoder.decode(Some(&packet), &mut output, false)?;
            decode_ns += clock.elapsed().as_nanos();
            if count != frame_size {
                return Err(format!("{name}: decoded {count}, expected {frame_size}").into());
            }
            record(&mut packets, &packet)?;
            for sample in &output[..count * channels] {
                nonzero |= *sample != 0;
                pcm_file.write_all(&sample.to_le_bytes())?;
            }
            samples += count;
            packet_count += 1;
        }
        let clock = Instant::now();
        let tail = encoder.finish()?;
        encode_ns += clock.elapsed().as_nanos();
        for packet in tail {
            let clock = Instant::now();
            let count = decoder.decode(Some(&packet), &mut output, false)?;
            decode_ns += clock.elapsed().as_nanos();
            record(&mut packets, &packet)?;
            for sample in &output[..count * channels] {
                pcm_file.write_all(&sample.to_le_bytes())?;
            }
            samples += count;
            packet_count += 1;
        }
        if !nonzero || packet_count < frames {
            return Err(format!("{name}: missing nonzero decoded stream").into());
        }
        packets.flush()?;
        pcm_file.flush()?;
        signal.flush()?;
        let lookahead = encoder.lookahead();
        let mut metadata = File::create(directory.join(format!("{name}.metadata")))?;
        writeln!(metadata, "{hz},{channels},{frame_size},{lookahead},{mode}")?;
        println!(
            "{{\"case\":\"{name}\",\"mode\":\"{mode}\",\"rate_hz\":{hz},\"channels\":{channels},\"frame_samples\":{frame_size},\"input_frames\":{frames},\"packets\":{packet_count},\"decoded_samples_per_channel\":{samples},\"encode_ns\":{encode_ns},\"decode_ns\":{decode_ns},\"wall_ns\":{}}}",
            started.elapsed().as_nanos()
        );
        Ok(())
    }

    /// Compute a diagnostic, non-cryptographic native-regression fingerprint.
    ///
    /// This only reads recorded bytes. It never updates checked-in expectations.
    ///
    /// # Errors
    /// * Returns filesystem read errors or a byte-count overflow.
    pub fn fingerprint(path: &Path) -> ProbeResult<(usize, u64)> {
        let mut reader = BufReader::new(File::open(path)?);
        let mut buffer = [0_u8; 8192];
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut bytes = 0_usize;
        loop {
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            bytes = bytes
                .checked_add(count)
                .ok_or("fingerprint length overflow")?;
            for byte in &buffer[..count] {
                hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        Ok((bytes, hash))
    }

    #[cfg(not(test))]
    fn report_fingerprints(directory: &Path) -> ProbeResult<()> {
        let mut paths = std::fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()?;
        paths.sort();
        for path in paths {
            if path.extension().is_some_and(|ext| {
                matches!(ext.to_str(), Some("packets" | "pcm" | "input" | "outcomes"))
            }) {
                let (bytes, hash) = fingerprint(&path)?;
                println!(
                    "diagnostic-native-fingerprint {} {bytes} {hash:016x}",
                    path.display()
                );
            }
        }
        Ok(())
    }

    /// Run explicit local diagnostics; printed fingerprints require human review.
    ///
    /// # Errors
    /// * Returns invalid-argument, codec or filesystem errors.
    #[cfg(not(test))]
    pub fn run() -> ProbeResult<()> {
        let arguments: Vec<_> = std::env::args().skip(1).collect();
        if arguments.len() != 2 {
            return Err(
                "usage: qualification_probe OUTPUT_DIRECTORY LONG_STREAM_FRAMES (50 frames/s)"
                    .into(),
            );
        }
        let directory = Path::new(&arguments[0]);
        let frames: usize = arguments[1].parse()?;
        if !(1..=30_000).contains(&frames) {
            return Err("long stream frames must be in 1..=30000".into());
        }
        std::fs::create_dir(directory)?;
        malformed(directory)?;
        for hz in [8000, 12000, 16000, 24000, 48000] {
            for channels in [1, 2] {
                stream(directory, "celt", hz, channels, 100)?;
            }
        }
        #[cfg(feature = "silk")]
        stream(directory, "silk", 16000, 1, 100)?;
        #[cfg(feature = "hybrid")]
        stream(directory, "hybrid", 48000, 2, 100)?;
        stream(directory, "celt", 48000, 2, frames)?;
        report_fingerprints(directory)?;
        Ok(())
    }
}

#[cfg(all(test, feature = "celt"))]
pub use implementation::{fingerprint, malformed, stream};

#[cfg(all(not(test), feature = "celt"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    implementation::run()
}

#[cfg(all(not(test), not(feature = "celt")))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    Err("qualification_probe requires the celt feature; use decoder fixture tests for SILK-only builds".into())
}
