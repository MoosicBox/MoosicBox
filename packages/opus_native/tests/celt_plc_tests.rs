//! Native CELT loss contracts and opt-in qualified live-reference comparison.
#![cfg(feature = "celt")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
use moosicbox_opus_native::{Channels, SampleRate, celt::CeltDecoder, range::RangeDecoder};
use std::process::Command;
const ORACLE: &str = r"
import math, struct, sys, pathlib
sys.path.insert(0, str(pathlib.Path(sys.argv[1])/'libopus/src'))
from oracle import Oracle
channels=int(sys.argv[3]); n=int(sys.argv[4]); burst=int(sys.argv[5]); output_channels=int(sys.argv[6])
with Oracle(sys.argv[2]) as oracle:
 oracle.request('encoder',rate=48000,channels=channels,application=2051)
 oracle.request('decoder',rate=48000,channels=output_channels)
 oracle.request('configure',controls=[[4002,128000],[4010,0]])
 for index in range(2*(8+burst)):
  step=index%(8+burst)
  if index==8+burst:
   oracle.request('reset',state='encoder');oracle.request('reset',state='decoder')
  lost=5 <= step < 5+burst
  packet=[]
  if not lost:
   pcm=[int(7000*math.sin(2*math.pi*(220+ch*113)*(step*n+i)/48000)+1900*math.sin(2*math.pi*731*(step*n+i)/48000)) for i in range(n) for ch in range(channels)]
   packet=oracle.request('encode',frame_size=n,pcm=pcm)['packet']
   assert packet and packet[0]>>3>=16
  result=oracle.request('decode',frame_size=n,packet=packet if packet else None)
  assert result['samples']==n
  pcm=result['pcm'];assert len(pcm)==n*output_channels
  sys.stdout.buffer.write(struct.pack('<II',len(packet),len(pcm))+bytes(packet)+struct.pack('<'+'h'*len(pcm),*pcm))
";

fn word(bytes: &[u8], offset: &mut usize) -> usize {
    let value = u32::from_le_bytes(bytes[*offset..*offset + 4].try_into().unwrap());
    *offset += 4;
    value as usize
}
#[test]
fn invalid_conceal_sizes_preserve_decoder_state() {
    for channels in [Channels::Mono, Channels::Stereo] {
        let packet = include_bytes!("../test_vectors/frozen/celt_fb_mono/packet.bin");
        let mut actual = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 960).unwrap();
        let mut reference = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 960).unwrap();
        actual.configure_synthesis_channels(channels);
        reference.configure_synthesis_channels(channels);
        for decoder in [&mut actual, &mut reference] {
            let mut entropy = RangeDecoder::new(&packet[1..]).unwrap();
            decoder
                .decode_celt_frame(&mut entropy, packet.len() - 1)
                .unwrap();
        }
        for size in [0, 1, 119, 121, 959, 961, 1920, usize::MAX] {
            assert!(actual.conceal(size).is_err());
        }
        assert_eq!(
            actual.conceal(960).unwrap().samples,
            reference.conceal(960).unwrap().samples
        );
    }
}

#[test]
fn reset_clears_loss_history_and_restores_fresh_recovery() {
    let packet = include_bytes!("../test_vectors/frozen/celt_fb_mono/packet.bin");
    for channels in [Channels::Mono, Channels::Stereo] {
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 960).unwrap();
        decoder.configure_synthesis_channels(channels);
        let mut entropy = RangeDecoder::new(&packet[1..]).unwrap();
        let first = decoder
            .decode_celt_frame(&mut entropy, packet.len() - 1)
            .unwrap()
            .samples;
        assert!(first.iter().any(|&sample| sample != 0));
        for _ in 0..45 {
            assert_eq!(
                decoder.conceal(960).unwrap().samples.len(),
                960 * channels as usize
            );
        }
        decoder.reset();
        let mut entropy = RangeDecoder::new(&packet[1..]).unwrap();
        assert_eq!(
            decoder
                .decode_celt_frame(&mut entropy, packet.len() - 1)
                .unwrap()
                .samples,
            first
        );
        let mut fresh = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 960).unwrap();
        fresh.configure_synthesis_channels(channels);
        let mut entropy = RangeDecoder::new(&packet[1..]).unwrap();
        fresh
            .decode_celt_frame(&mut entropy, packet.len() - 1)
            .unwrap();
        assert_eq!(
            decoder.conceal(960).unwrap().samples,
            fresh.conceal(960).unwrap().samples
        );
    }
}

#[test]
#[ignore = "requires explicit MOOSICBOX_OPUS_REFERENCE_ORACLE Cargo-linked runner; see libopus/README.md"]
fn celt_loss_and_recovery_matches_pinned_public_decoder() {
    let executable = std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect(
        "set MOOSICBOX_OPUS_REFERENCE_ORACLE to the qualified Cargo-built reference_oracle executable; see packages/opus_native/libopus/README.md",
    );
    for (channels, output_channels) in [
        (Channels::Mono, Channels::Mono),
        (Channels::Stereo, Channels::Stereo),
        (Channels::Mono, Channels::Stereo),
        (Channels::Stereo, Channels::Mono),
    ] {
        for n in [120, 240, 480, 960] {
            for burst in [1, 7, 45] {
                let output = Command::new("python3")
                    .arg("-B")
                    .args(["-c", ORACLE, env!("CARGO_MANIFEST_DIR")])
                    .arg(&executable)
                    .args([
                        &(channels as usize).to_string(),
                        &n.to_string(),
                        &burst.to_string(),
                        &(output_channels as usize).to_string(),
                    ])
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let mut decoder = CeltDecoder::new(SampleRate::Hz48000, channels, n).unwrap();
                decoder.configure_synthesis_channels(output_channels);
                let mut memory = [0_i32; 2];
                let mut offset = 0;
                let mut step = 0;
                while offset < output.stdout.len() {
                    if step == 8 + burst {
                        decoder.reset();
                        memory = [0; 2];
                    }
                    let len = word(&output.stdout, &mut offset);
                    let count = word(&output.stdout, &mut offset);
                    let packet = &output.stdout[offset..offset + len];
                    offset += len;
                    let expected: Vec<_> = output.stdout[offset..offset + count * 2]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|s| i16::from_le_bytes(*s))
                        .collect();
                    offset += count * 2;
                    let signal = if len == 0 {
                        decoder.conceal(n).unwrap()
                    } else {
                        let mut rd = RangeDecoder::new(&packet[1..]).unwrap();
                        decoder.decode_celt_frame(&mut rd, len - 1).unwrap()
                    };
                    let mut actual = vec![0; count];
                    for (i, &sample) in signal.samples.iter().enumerate() {
                        let ch = i % (output_channels as usize);
                        let value = (sample + memory[ch]).clamp(-536_870_911, 536_870_911);
                        memory[ch] = i32::try_from((27853_i64 * i64::from(value)) >> 15).unwrap();
                        // Verified corpus contract rounds Q12 directly to i16.
                        actual[i] =
                            i16::try_from(((value + 2048) >> 12).clamp(-32768, 32767)).unwrap();
                    }
                    if len == 0 && step % (8 + burst) == 5 {
                        assert!(
                            expected.iter().any(|&sample| sample != 0),
                            "nonzero first-loss oracle"
                        );
                        assert!(
                            actual.iter().any(|&sample| sample != 0),
                            "nonzero native concealment"
                        );
                    }
                    assert_eq!(
                        actual.iter().zip(&expected).position(|(a, b)| a != b),
                        None,
                        "channels={channels:?} n={n} burst={burst} step={step} first actual={:?} expected={:?}",
                        &actual[..8],
                        &expected[..8]
                    );
                    step += 1;
                }
            }
        }
    }
}
