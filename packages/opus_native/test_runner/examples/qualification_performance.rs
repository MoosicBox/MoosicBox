//! Offline integer-origin PCM benchmark. Timing evidence, not quality certification.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "celt")]
mod enabled {
    use moosicbox_opus_native::{Channels, Decoder, Encoder, EncoderConfig, SampleRate};
    use serde_json::json;
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    use std::time::Instant;

    type Error = Box<dyn std::error::Error>;

    fn hex(bytes: &[u8]) -> Result<String, std::fmt::Error> {
        bytes.iter().try_fold(
            String::with_capacity(bytes.len() * 2),
            |mut output, byte| {
                write!(output, "{byte:02x}")?;
                Ok(output)
            },
        )
    }

    fn frame(seed: &mut u32, samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|_| {
                *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let sample = i16::from_le_bytes(seed.to_le_bytes()[..2].try_into().unwrap());
                // Exact binary conversion of bounded signed integer PCM, no libm dependence.
                f32::from(sample) / 131_072.0
            })
            .collect()
    }

    fn encoder(mode: &str, config: EncoderConfig) -> Result<Encoder, Error> {
        Ok(match mode {
            "celt" => Encoder::new(config)?,
            "auto" => Encoder::new_auto(config)?,
            #[cfg(feature = "silk")]
            "silk" => Encoder::new_silk(config)?,
            #[cfg(feature = "hybrid")]
            "hybrid" => Encoder::new_hybrid(config)?,
            _ => return Err("requested benchmark codec feature is unavailable".into()),
        })
    }

    fn summary(times: &mut [u128]) -> serde_json::Value {
        times.sort_unstable();
        let sum: u128 = times.iter().sum();
        json!({"total_ns":sum,"median_ns":times[times.len()/2],
               "p95_ns":times[(times.len()-1)*95/100],"max_ns":times[times.len()-1]})
    }

    #[allow(clippy::too_many_lines)]
    pub fn run() -> Result<(), Error> {
        if cfg!(debug_assertions) {
            return Err("benchmark requires a release build (debug assertions disabled)".into());
        }
        let args: Vec<String> = std::env::args().collect();
        if args.len() != 3 {
            return Err("usage: qualification_performance CASE FRAMES".into());
        }
        let frames: usize = args[2].parse()?;
        if !(10..=30_000).contains(&frames) {
            return Err("frames must be 10..=30000".into());
        }
        let (mode, rate, hz, channels, bitrate) = match args[1].as_str() {
            "celt-mono" => ("celt", SampleRate::Hz48000, 48_000, Channels::Mono, 128_000),
            "celt-stereo" => (
                "celt",
                SampleRate::Hz48000,
                48_000,
                Channels::Stereo,
                128_000,
            ),
            "celt-highrate" => (
                "celt",
                SampleRate::Hz48000,
                48_000,
                Channels::Stereo,
                256_000,
            ),
            "silk-mono" => ("silk", SampleRate::Hz16000, 16_000, Channels::Mono, 128_000),
            "hybrid-stereo" => (
                "hybrid",
                SampleRate::Hz48000,
                48_000,
                Channels::Stereo,
                128_000,
            ),
            "auto-stereo" => (
                "auto",
                SampleRate::Hz48000,
                48_000,
                Channels::Stereo,
                128_000,
            ),
            "silk-mono-48k" => ("silk", SampleRate::Hz16000, 16_000, Channels::Mono, 48_000),
            "silk-stereo-96k" => (
                "silk",
                SampleRate::Hz16000,
                16_000,
                Channels::Stereo,
                96_000,
            ),
            // Candidate-only diagnostics below current public guards; constructors still reject.
            "silk-mono-16k" => ("silk", SampleRate::Hz16000, 16_000, Channels::Mono, 16_000),
            "silk-mono-32k" => ("silk", SampleRate::Hz16000, 16_000, Channels::Mono, 32_000),
            "hybrid-stereo-32k" => (
                "hybrid",
                SampleRate::Hz48000,
                48_000,
                Channels::Stereo,
                32_000,
            ),
            "hybrid-stereo-64k" => (
                "hybrid",
                SampleRate::Hz48000,
                48_000,
                Channels::Stereo,
                64_000,
            ),
            "celt-stereo-32k" => (
                "celt",
                SampleRate::Hz48000,
                48_000,
                Channels::Stereo,
                32_000,
            ),
            "celt-stereo-64k" => (
                "celt",
                SampleRate::Hz48000,
                48_000,
                Channels::Stereo,
                64_000,
            ),
            "auto-stereo-32k" => (
                "auto",
                SampleRate::Hz48000,
                48_000,
                Channels::Stereo,
                32_000,
            ),
            "auto-stereo-64k" => (
                "auto",
                SampleRate::Hz48000,
                48_000,
                Channels::Stereo,
                64_000,
            ),
            _ => return Err("unknown standard benchmark case".into()),
        };
        let config = EncoderConfig {
            sample_rate: rate,
            channels,
            bitrate,
            frame_size: hz / 50,
        };
        let channel_count = if channels == Channels::Mono { 1 } else { 2 };
        let mut encoder = encoder(mode, config)?;
        let mut decoder = Decoder::new(rate, channels)?;
        let mut seed = 2457_u32;
        let mut pcm = vec![0_i16; config.frame_size * channel_count];
        // Fixed warm-up count; excluded from timings/hashes, retained codec history.
        for _ in 0..10 {
            let packet = encoder.encode(&frame(&mut seed, pcm.len()))?;
            if decoder.decode(Some(&packet), &mut pcm, false)? != config.frame_size {
                return Err("warm-up decode duration mismatch".into());
            }
        }
        let mut input_hash = Sha256::new();
        let mut packet_hash = Sha256::new();
        let mut pcm_hash = Sha256::new();
        let mut encode_times = Vec::with_capacity(frames);
        let mut decode_times = Vec::with_capacity(frames);
        let mut packet_bytes = 0_usize;
        for _ in 0..frames {
            let input = frame(&mut seed, pcm.len());
            for sample in &input {
                input_hash.update(sample.to_le_bytes());
            }
            let start = Instant::now();
            let packet = encoder.encode(&input)?;
            encode_times.push(start.elapsed().as_nanos());
            let start = Instant::now();
            let count = decoder.decode(Some(&packet), &mut pcm, false)?;
            decode_times.push(start.elapsed().as_nanos());
            if count != config.frame_size {
                return Err("decode duration mismatch".into());
            }
            packet_bytes += packet.len();
            packet_hash.update(u64::try_from(packet.len())?.to_le_bytes());
            packet_hash.update(&packet);
            for sample in &pcm {
                pcm_hash.update(sample.to_le_bytes());
            }
        }
        let encode = summary(&mut encode_times);
        let decode = summary(&mut decode_times);
        let audio_ns = u64::try_from(frames)? * 20_000_000;
        println!(
            "{}",
            json!({
                "schema":"moosicbox.opus.performance-case.v1", "case":args[1],
                "package_version":env!("CARGO_PKG_VERSION"), "debug_assertions":cfg!(debug_assertions),
                "seed":2457,"generator":"lcg-i16-div131072-v1", "warmup_frames":10,
                "frames":frames,"rate":hz,"channels":channel_count,"bitrate":bitrate,
                "audio_ns":audio_ns,"encode":encode,"decode":decode,"packet_bytes":packet_bytes,
                "input_sha256":hex(&input_hash.finalize())?,
                "packets_sha256":hex(&packet_hash.finalize())?,
                "pcm_sha256":hex(&pcm_hash.finalize())?,
                "timing_scope":"codec calls only; construction, PCM generation, hashing and JSON excluded"
            })
        );
        Ok(())
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "celt")]
    {
        enabled::run()
    }
    #[cfg(not(feature = "celt"))]
    {
        Err("qualification_performance requires the celt feature".into())
    }
}
