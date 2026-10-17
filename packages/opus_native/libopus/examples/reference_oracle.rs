//! Explicit, bounded, persistent public-libopus test oracle. See the package README.
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
use moosicbox_opus_native_libopus::{
    reference::verify_reference_build,
    safe::{Decoder, Encoder},
};
use serde_json::{Value, json};
use std::io::{self, BufRead, Read, Write};

type Error = Box<dyn std::error::Error>;
fn number<T: TryFrom<u64>>(v: &Value, key: &str) -> Result<T, Error> {
    v[key]
        .as_u64()
        .and_then(|n| T::try_from(n).ok())
        .ok_or_else(|| format!("invalid {key}").into())
}
fn array<T: TryFrom<i64>>(v: &Value, key: &str, max: usize) -> Result<Vec<T>, Error> {
    let a = v[key].as_array().ok_or("expected array")?;
    if a.len() > max {
        return Err("array exceeds bound".into());
    }
    a.iter()
        .map(|n| {
            n.as_i64()
                .and_then(|x| T::try_from(x).ok())
                .ok_or_else(|| "invalid array value".into())
        })
        .collect()
}
fn manifest_only() -> Result<bool, Error> {
    let manifest = verify_reference_build()?;
    match std::env::args().nth(1).as_deref() {
        Some("provenance") => {
            println!("{manifest}");
            Ok(true)
        }
        None => Ok(false),
        _ => Err("usage: reference_oracle [provenance]".into()),
    }
}

fn main() -> Result<(), Error> {
    if manifest_only()? {
        return Ok(());
    }
    let mut encoder = None;
    let mut decoder = None;
    let mut channels = 1_usize;
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    loop {
        // take() bounds allocation even for an unterminated malicious line.
        let mut bytes = Vec::new();
        let read = input
            .by_ref()
            .take(1_048_577)
            .read_until(b'\n', &mut bytes)?;
        if read == 0 {
            break;
        }
        if read > 1_048_576 {
            return Err("request exceeds 1 MiB".into());
        }
        let v: Value = serde_json::from_slice(&bytes)?;
        let reply = match v["op"].as_str().ok_or("missing op")? {
            "encoder" => {
                let application = v["application"].as_i64().unwrap_or(2049);
                encoder = Some(Encoder::new(
                    number(&v, "rate")?,
                    number(&v, "channels")?,
                    i32::try_from(application)?,
                )?);
                json!({"ok":true})
            }
            "decoder" => {
                let count: u8 = number(&v, "channels")?;
                decoder = Some(Decoder::new(number(&v, "rate")?, count)?);
                channels = usize::from(count);
                json!({"ok":true})
            }
            "configure" => {
                let e = encoder.as_mut().ok_or("create encoder first")?;
                let settings = v["controls"].as_array().ok_or("controls array required")?;
                if settings.len() > 32 {
                    return Err("too many controls".into());
                }
                for setting in settings {
                    let pair = setting
                        .as_array()
                        .filter(|a| a.len() == 2)
                        .ok_or("control must be [request,value]")?;
                    e.reference_control(
                        i32::try_from(pair[0].as_i64().ok_or("request")?)?,
                        i32::try_from(pair[1].as_i64().ok_or("value")?)?,
                    )?;
                }
                json!({"ok":true})
            }
            "encode" => {
                let pcm = array::<i16>(&v, "pcm", 11520)?;
                let mut packet = vec![0; 4000];
                let size = encoder.as_mut().ok_or("create encoder first")?.encode(
                    &pcm,
                    number(&v, "frame_size")?,
                    &mut packet,
                )?;
                packet.truncate(size);
                json!({"packet":packet})
            }
            "decode" => {
                let size: usize = number(&v, "frame_size")?;
                if size == 0 || size > 5760 {
                    return Err("invalid decode frame size".into());
                }
                let packet = if v["packet"].is_null() {
                    Vec::new()
                } else {
                    array::<u8>(&v, "packet", 65535)?
                };
                let mut pcm = vec![0; size * channels];
                let count = decoder.as_mut().ok_or("create decoder first")?.decode(
                    &packet,
                    &mut pcm,
                    size,
                    v["fec"].as_bool().unwrap_or(false),
                )?;
                pcm.truncate(count * channels);
                json!({"samples":count,"pcm":pcm})
            }
            "reset" => {
                match v["state"].as_str() {
                    Some("encoder") => encoder.as_mut().ok_or("create encoder first")?.reset()?,
                    Some("decoder") => decoder.as_mut().ok_or("create decoder first")?.reset()?,
                    _ => return Err("reset state must be encoder or decoder".into()),
                }
                json!({"ok":true})
            }
            _ => return Err("unknown operation".into()),
        };
        writeln!(output, "{reply}")?;
        output.flush()?;
    }
    Ok(())
}
