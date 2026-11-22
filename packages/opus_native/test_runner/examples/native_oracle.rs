//! Explicit native-only JSON-lines adapter for independent codec qualification.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
use moosicbox_opus_native::{Channels, Decoder, Encoder, EncoderConfig, SampleRate};
use serde_json::{Value, json};
use std::io::{self, BufRead, Read, Write};
type Error = Box<dyn std::error::Error>;
fn number(v: &Value, key: &str) -> Result<usize, Error> {
    v[key]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("invalid {key}").into())
}
fn rate(v: &Value) -> Result<SampleRate, Error> {
    Ok(match number(v, "rate")? {
        8000 => SampleRate::Hz8000,
        12000 => SampleRate::Hz12000,
        16000 => SampleRate::Hz16000,
        24000 => SampleRate::Hz24000,
        48000 => SampleRate::Hz48000,
        _ => return Err("invalid rate".into()),
    })
}
fn channels(v: &Value) -> Result<Channels, Error> {
    Ok(match number(v, "channels")? {
        1 => Channels::Mono,
        2 => Channels::Stereo,
        _ => return Err("invalid channels".into()),
    })
}
fn array<T: TryFrom<i64>>(v: &Value, key: &str, max: usize) -> Result<Vec<T>, Error> {
    let values = v[key]
        .as_array()
        .filter(|a| a.len() <= max)
        .ok_or("invalid array")?;
    values
        .iter()
        .map(|n| {
            n.as_i64()
                .and_then(|x| T::try_from(x).ok())
                .ok_or_else(|| "invalid array value".into())
        })
        .collect()
}
#[derive(Default)]
struct NativeEncoder {
    config: Option<EncoderConfig>,
    state: Option<Encoder>,
    mode: i64,
}
impl NativeEncoder {
    fn create(&mut self, size: usize) -> Result<(), Error> {
        let mut config = self.config.ok_or("create encoder first")?;
        config.frame_size = size;
        self.state = Some(match self.mode {
            0 => Encoder::new_auto(config)?,
            1000 => Encoder::new_silk(config)?,
            1001 => Encoder::new_hybrid(config)?,
            1002 => Encoder::new(config)?,
            _ => return Err("unsupported native mode".into()),
        });
        self.config = Some(config);
        Ok(())
    }
}
#[allow(clippy::too_many_lines)]
fn main() -> Result<(), Error> {
    match std::env::args().nth(1).as_deref() {
        Some("provenance") => {
            println!(
                "{}",
                json!({"schema":"moosicbox.opus.native-adapter.v1","implementation":"pure-rust","package_version":env!("CARGO_PKG_VERSION")})
            );
            return Ok(());
        }
        None => {}
        _ => return Err("usage: native_oracle [provenance]".into()),
    }
    let mut encoder = NativeEncoder::default();
    let mut decoder = None;
    let mut decoder_channels = 1;
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    loop {
        let mut bytes = Vec::new();
        let count = input
            .by_ref()
            .take(1_048_577)
            .read_until(b'\n', &mut bytes)?;
        if count == 0 {
            break;
        }
        if count > 1_048_576 {
            return Err("request exceeds 1 MiB".into());
        }
        let v: Value = serde_json::from_slice(&bytes)?;
        let reply = match v["op"].as_str().ok_or("missing op")? {
            "encoder" => {
                if v["application"].as_i64().unwrap_or(2049) != 2049 {
                    return Err("native adapter supports audio application only".into());
                }
                encoder = NativeEncoder {
                    config: Some(EncoderConfig {
                        sample_rate: rate(&v)?,
                        channels: channels(&v)?,
                        ..EncoderConfig::default()
                    }),
                    state: None,
                    mode: 0,
                };
                json!({"ok":true})
            }
            "configure" => {
                if encoder.state.is_some() {
                    return Err(
                        "configure before encode; active reconfiguration not emulated".into(),
                    );
                }
                let config = encoder.config.as_mut().ok_or("create encoder first")?;
                let controls = v["controls"]
                    .as_array()
                    .filter(|a| a.len() <= 32)
                    .ok_or("controls array required")?;
                for control in controls {
                    let pair = control
                        .as_array()
                        .filter(|a| a.len() == 2)
                        .ok_or("control must be pair")?;
                    let request = pair[0].as_i64().ok_or("invalid request")?;
                    let value = pair[1].as_i64().ok_or("invalid value")?;
                    match (request, value) {
                        (11002, 1000..=1002) => encoder.mode = value,
                        (4002, 1..=512_000) => config.bitrate = u32::try_from(value)?,
                        // CBR is native's current explicit-mode default, not a simulated VBR control.
                        (4006, 0) => {}
                        _ => {
                            return Err(
                                format!("unsupported native control {request}={value}").into()
                            );
                        }
                    }
                }
                json!({"ok":true})
            }
            "encode" => {
                let size = number(&v, "frame_size")?;
                if encoder.state.is_none() {
                    encoder.create(size)?;
                }
                let config = encoder.config.ok_or("create encoder first")?;
                if size != config.frame_size {
                    return Err("native adapter requires fixed duration per state".into());
                }
                let pcm = array::<i16>(&v, "pcm", 11520)?;
                let pcm: Vec<f32> = pcm.into_iter().map(|x| f32::from(x) / 32768.0).collect();
                let packet = encoder
                    .state
                    .as_mut()
                    .ok_or("create encoder first")?
                    .encode(&pcm)?;
                json!({"packet":packet})
            }
            "decoder" => {
                let layout = channels(&v)?;
                decoder_channels = if matches!(layout, Channels::Mono) {
                    1
                } else {
                    2
                };
                decoder = Some(Decoder::new(rate(&v)?, layout)?);
                json!({"ok":true})
            }
            "decoder_configure" => {
                let controls = v["controls"]
                    .as_array()
                    .filter(|a| a.len() <= 32)
                    .ok_or("controls array required")?;
                let state = decoder.as_mut().ok_or("create decoder first")?;
                for control in controls {
                    let pair = control
                        .as_array()
                        .filter(|a| a.len() == 2)
                        .ok_or("control must be pair")?;
                    if pair[0].as_i64() != Some(4034) {
                        return Err("unsupported decoder control".into());
                    }
                    state.set_gain(i16::try_from(pair[1].as_i64().ok_or("invalid gain")?)?);
                }
                json!({"ok":true})
            }
            "decode" => {
                let size = number(&v, "frame_size")?;
                if size == 0 || size > 5760 {
                    return Err("invalid frame size".into());
                }
                let state = decoder.as_mut().ok_or("create decoder first")?;
                let mut pcm = vec![0; size * decoder_channels];
                let count = if v["packet"].is_null() {
                    state.decode_loss(&mut pcm, size)?
                } else {
                    let packet = array::<u8>(&v, "packet", 65535)?;
                    state.decode(Some(&packet), &mut pcm, v["fec"].as_bool().unwrap_or(false))?
                };
                pcm.truncate(count * decoder_channels);
                json!({"samples":count,"pcm":pcm})
            }
            "reset" => {
                match v["state"].as_str() {
                    Some("encoder") => {
                        let size = encoder.config.ok_or("create encoder first")?.frame_size;
                        encoder.create(size)?;
                    }
                    Some("decoder") => decoder
                        .as_mut()
                        .ok_or("create decoder first")?
                        .reset_state()?,
                    _ => return Err("invalid reset state".into()),
                }
                json!({"ok":true})
            }
            "lookahead" => {
                json!({"samples":encoder.state.as_ref().ok_or("encode before lookahead")?.lookahead()})
            }
            _ => return Err("unknown operation".into()),
        };
        writeln!(output, "{reply}")?;
        output.flush()?;
    }
    Ok(())
}
