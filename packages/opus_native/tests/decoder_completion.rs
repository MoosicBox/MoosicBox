#![cfg(all(feature = "silk", feature = "celt"))]

use moosicbox_opus_native::{Channels, Decoder, SampleRate};
use std::process::Command;

const ORACLE: &str = r"
import math,os,pathlib,struct,sys
sys.path.insert(0,str(pathlib.Path(sys.argv[1])/'libopus/src'))
from oracle import Oracle
with Oracle(os.environ['MOOSICBOX_OPUS_REFERENCE_ORACLE']) as o:
 def req(op,**kw):
  result=o.request(op,**kw)
  assert 'error' not in result,(op,result)
  return result
 def record(packet,count,fec=False):
  result=req('decode',packet=packet,frame_size=count,fec=fec)
  assert result['samples']==count,result
  data=bytes(packet or [])
  sys.stdout.buffer.write(struct.pack('<IIII',len(data),count,int(fec),len(result['pcm'])))
  sys.stdout.buffer.write(data)
  sys.stdout.buffer.write(struct.pack('<'+'h'*len(result['pcm']),*result['pcm']))
 channels=int(sys.argv[2]); scenario=sys.argv[3]
 if scenario=='short_fec':
  root=pathlib.Path(sys.argv[1])
  req('decoder',rate=48000,channels=channels)
  for path in ['src/silk/reference/nb_sine_mono.packet',
               'test_vectors/frozen/hybrid_fb_mono/packet.bin',
               'test_vectors/frozen/celt_fb_mono/packet.bin']:
   packet=list((root/path).read_bytes())
   for size in [0,1]:
    for toc in [8,120,248]:
     for count in [960,1920]:
      short=[toc]+[0]*size
      record(short,count,True)
      record(packet,960)
      record(short,count,True)
      record(packet,960)
  sys.exit(0)
 if scenario=='one_byte':
  root=pathlib.Path(sys.argv[1])
  req('decoder',rate=48000,channels=channels)
  packets=[list((root/path).read_bytes()) for path in [
   'src/silk/reference/nb_sine_mono.packet',
   'test_vectors/frozen/hybrid_fb_mono/packet.bin',
   'test_vectors/frozen/celt_fb_mono/packet.bin']]
  for packet in packets:
   toc=packet[0]&252
   record([toc,0],960)
   record(packet,960)
   for short_toc in [8,120,248]:
    record([short_toc,0],960); record(packet,960)
   record([toc|2,1,0]+packet[1:],1920)
   record(packet,960)
  sys.exit(0)
 if scenario.startswith('fixture_'):
  root=pathlib.Path(sys.argv[1])
  mono=list((root/'test_vectors/frozen/hybrid_fb_mono/packet.bin').read_bytes())
  req('decoder',rate=8000 if scenario in ['fixture_long40','fixture_long60'] else 48000,channels=channels)
  if scenario in ['fixture_long40','fixture_long60']:
   packet=list((root/'src/silk/reference/nb_sine_mono.packet').read_bytes())
   record(packet,160)
   long=packet.copy(); long[0]=(long[0]&7)|((2 if scenario=='fixture_long40' else 3)<<3)
   record(long,160,True); record(packet,160)
  elif scenario=='fixture_hybrid_fec':
   record(mono,960); record(mono,960,True); record(None,960,True); record(mono,960)
  else:
   stereo=list((root/'test_vectors/frozen/corpus/sequences/hybrid_swb_stereo/packet.bin').read_bytes())
   celt=list((root/'test_vectors/frozen/celt_fb_mono/packet.bin').read_bytes())
   for packet in [mono,stereo,celt,mono]: record(packet,960)
   record(None,960)
  sys.exit(0)
 gain=int(sys.argv[4]) if len(sys.argv)>4 else 0
 req('encoder',rate=48000,channels=2,application=2048)
 req('decoder',rate=48000,channels=channels)
 if gain: req('decoder_configure',controls=[[4034,gain]])
 req('configure',controls=[[4002,48000],[4010,0],[4006,0],[4012,1],[4014,35]])
 position=0
 observed=set(); coded_seen=set(); fec_calls=0
 for i in range(36 if scenario=='cross' else 18):
  mode=([1000,1001,1002][(i//6)%3] if scenario=='cross' else ([1000,1001,1000,1002,1000,1001][i%6] if scenario=='stereo_fec' else (1002 if scenario=='channels' else (1001 if scenario=='hybrid' else 1000))))
  coded=[2,2,1,1,2,1][i%6]
  ms=([20,40,60][i%3] if scenario=='fec' else 20)
  if scenario!='stereo_fec':
   req('configure',controls=[[4008,1103 if mode==1000 else 1105],[4002,48000 if mode==1000 else 128000]])
  req('configure',controls=[[11002,mode],[4022,coded],[4004,1105 if mode!=1000 else 1103]])
  n=48*ms
  pcm=[int(9000*math.sin(2*math.pi*(173+67*c)*(position+j)/48000)) for j in range(n) for c in range(2)]
  position+=n
  packet=req('encode',frame_size=n,pcm=pcm)['packet']
  actual=1002 if packet[0]>>3>=16 else (1001 if packet[0]>>3>=12 else 1000)
  observed.add(actual); coded_seen.add(1+((packet[0]>>2)&1))
  if scenario!='cross' and scenario!='stereo_fec': assert actual==mode,(scenario,actual,mode)
  if (scenario=='fec' and i%3==2) or scenario in ['cross','stereo_fec']:
   record(packet,n,True); fec_calls+=1
  record(packet,n)
  if scenario=='short' or scenario=='hybrid':
   for count in [120,240,360,600,840]: record(None,count)
 assert coded_seen=={1,2},coded_seen
 if scenario=='cross': assert observed=={1000,1001,1002},observed
 if scenario in ['fec','cross','stereo_fec']: assert fec_calls>0
";

#[test]
#[ignore = "requires explicitly selected MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn short_fec_constituents_match_reference() {
    for channels in [Channels::Mono, Channels::Stereo] {
        compare_reference("short_fec", channels, 0);
    }
}

#[test]
fn short_fec_constituents_match_explicit_loss_and_recovery() {
    for channels in [Channels::Mono, Channels::Stereo] {
        for packet in [
            include_bytes!("../src/silk/reference/nb_sine_mono.packet").as_slice(),
            include_bytes!("../test_vectors/frozen/hybrid_fb_mono/packet.bin").as_slice(),
            include_bytes!("../test_vectors/frozen/celt_fb_mono/packet.bin").as_slice(),
        ] {
            for payload in [&[][..], &[0][..]] {
                for toc in [8, 120, 248] {
                    for count in [960, 1920] {
                        let mut decoder = Decoder::new(SampleRate::Hz48000, channels).unwrap();
                        let mut control = Decoder::new(SampleRate::Hz48000, channels).unwrap();
                        let mut short = vec![toc];
                        short.extend_from_slice(payload);
                        let width = channels as usize;
                        let mut actual = vec![123; count * width];
                        let mut expected = vec![0; count * width];
                        for _ in 0..2 {
                            assert_eq!(
                                decoder.decode(Some(&short), &mut actual, true).unwrap(),
                                count
                            );
                            assert_eq!(control.decode_loss(&mut expected, count).unwrap(), count);
                            assert_eq!(actual, expected);
                            let mut recovered = vec![0; 960 * width];
                            let mut reference = vec![0; 960 * width];
                            decoder.decode(Some(packet), &mut recovered, false).unwrap();
                            control.decode(Some(packet), &mut reference, false).unwrap();
                            assert_eq!(recovered, reference);
                        }
                    }
                }
            }
        }
    }
}

#[test]
#[ignore = "requires explicitly selected MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn one_byte_constituents_match_reference() {
    for channels in [Channels::Mono, Channels::Stereo] {
        compare_reference("one_byte", channels, 0);
    }
}

#[test]
fn one_byte_constituents_match_explicit_loss_and_recovery() {
    let packets: [&[u8]; 3] = [
        include_bytes!("../src/silk/reference/nb_sine_mono.packet"),
        include_bytes!("../test_vectors/frozen/hybrid_fb_mono/packet.bin"),
        include_bytes!("../test_vectors/frozen/celt_fb_mono/packet.bin"),
    ];
    for channels in [Channels::Mono, Channels::Stereo] {
        for packet in packets {
            let mut decoder = Decoder::new(SampleRate::Hz48000, channels).unwrap();
            let mut control = Decoder::new(SampleRate::Hz48000, channels).unwrap();
            let width = channels as usize;
            let mut actual = vec![123; 960 * width];
            let mut expected = vec![0; 960 * width];
            for short_toc in [8, 120, 248] {
                assert_eq!(
                    decoder
                        .decode(Some(&[short_toc, 0]), &mut actual, false)
                        .unwrap(),
                    960
                );
                assert_eq!(control.decode_loss(&mut expected, 960).unwrap(), 960);
                assert_eq!(actual, expected);
                decoder.decode(Some(packet), &mut actual, false).unwrap();
                control.decode(Some(packet), &mut expected, false).unwrap();
                assert_eq!(actual, expected, "short-constituent recovery");
            }
            // A one-byte first constituent must not select a new codec mode
            // before the following real constituent is decoded.
            let mut combined = vec![(packet[0] & !3) | 2, 1, 0];
            combined.extend_from_slice(&packet[1..]);
            let mut actual = vec![123; 1920 * width];
            let mut expected = vec![0; 1920 * width];
            assert_eq!(
                decoder.decode(Some(&combined), &mut actual, false).unwrap(),
                1920
            );
            assert_eq!(
                control
                    .decode_loss(&mut expected[..960 * width], 960)
                    .unwrap(),
                960
            );
            assert_eq!(
                control
                    .decode(Some(packet), &mut expected[960 * width..], false)
                    .unwrap(),
                960
            );
            assert_eq!(actual, expected);
            let mut recovery = vec![0; 960 * width];
            decoder.decode(Some(packet), &mut recovery, false).unwrap();
            control
                .decode(Some(packet), &mut expected[..960 * width], false)
                .unwrap();
            assert_eq!(recovery, expected[..960 * width]);
        }
    }
}

#[test_case::test_case("fixture_long40")]
#[test_case::test_case("fixture_long60")]
#[test_case::test_case("fixture_hybrid_fec")]
#[test_case::test_case("fixture_transitions")]
#[ignore = "requires explicitly selected MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn supported_output_fixtures_match_reference(scenario: &str) {
    compare_reference(scenario, Channels::Mono, 0);
}

#[test_case::test_case("channels")]
#[test_case::test_case("fec")]
#[test_case::test_case("short")]
#[test_case::test_case("hybrid")]
#[test_case::test_case("cross")]
#[test_case::test_case("stereo_fec")]
#[ignore = "requires explicitly selected MOOSICBOX_OPUS_REFERENCE_ORACLE"]
fn legal_channel_fec_and_short_loss_match_reference(scenario: &str) {
    for channels in [Channels::Mono, Channels::Stereo] {
        compare_reference(scenario, channels, 0);
    }
}

#[test_case::test_case("channels", -32768; "channels_minus_32768")]
#[test_case::test_case("channels", -1536; "channels_minus_1536")]
#[test_case::test_case("channels", -1; "channels_minus_1")]
#[test_case::test_case("channels", 1; "channels_plus_1")]
#[test_case::test_case("channels", 1536; "channels_plus_1536")]
#[test_case::test_case("channels", 32767; "channels_plus_32767")]
#[test_case::test_case("short", -32768; "short_minus_32768")]
#[test_case::test_case("short", -1536; "short_minus_1536")]
#[test_case::test_case("short", -1; "short_minus_1")]
#[test_case::test_case("short", 1; "short_plus_1")]
#[test_case::test_case("short", 1536; "short_plus_1536")]
#[test_case::test_case("short", 32767; "short_plus_32767")]
#[test_case::test_case("hybrid", -32768; "hybrid_minus_32768")]
#[test_case::test_case("hybrid", -1536; "hybrid_minus_1536")]
#[test_case::test_case("hybrid", -1; "hybrid_minus_1")]
#[test_case::test_case("hybrid", 1; "hybrid_plus_1")]
#[test_case::test_case("hybrid", 1536; "hybrid_plus_1536")]
#[test_case::test_case("hybrid", 32767; "hybrid_plus_32767")]
#[test_case::test_case("cross", -32768; "cross_minus_32768")]
#[test_case::test_case("cross", -1536; "cross_minus_1536")]
#[test_case::test_case("cross", -1; "cross_minus_1")]
#[test_case::test_case("cross", 1; "cross_plus_1")]
#[test_case::test_case("cross", 1536; "cross_plus_1536")]
#[test_case::test_case("cross", 32767; "cross_plus_32767")]
#[ignore = "requires explicitly selected MOOSICBOX_OPUS_REFERENCE_ORACLE with decoder_configure"]
fn nonzero_gain_matches_reference(scenario: &str, gain: i16) {
    compare_reference(scenario, Channels::Stereo, gain);
}

fn compare_reference(scenario: &str, channels: Channels, gain: i16) {
    let oracle = Command::new("python3")
        .args(["-B", "-c", ORACLE, env!("CARGO_MANIFEST_DIR")])
        .arg((channels as usize).to_string())
        .arg(scenario)
        .arg(gain.to_string())
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let mut bytes = oracle.stdout.as_slice();
    let rate = if matches!(scenario, "fixture_long40" | "fixture_long60") {
        SampleRate::Hz8000
    } else {
        SampleRate::Hz48000
    };
    let mut decoder = Decoder::new(rate, channels).unwrap();
    decoder.set_gain(gain);
    let mut step = 0;
    while !bytes.is_empty() {
        let header: Vec<_> = bytes[..16]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|x| u32::from_le_bytes(*x) as usize)
            .collect();
        bytes = &bytes[16..];
        let packet = &bytes[..header[0]];
        bytes = &bytes[header[0]..];
        let expected: Vec<_> = bytes[..header[3] * 2]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|x| i16::from_le_bytes(*x))
            .collect();
        bytes = &bytes[header[3] * 2..];
        let mut actual = vec![123; expected.len()];
        let result = if packet.is_empty() {
            decoder.decode_loss(&mut actual, header[1])
        } else {
            decoder.decode(Some(packet), &mut actual, header[2] != 0)
        };
        assert_eq!(
            result.unwrap(),
            header[1],
            "{scenario} {channels:?} step {step}"
        );
        assert_eq!(
            actual.iter().zip(&expected).position(|(a, b)| a != b),
            None,
            "{scenario} {channels:?} step {step}, gain {gain}, toc {:?}, mismatch {:?}",
            packet.first(),
            actual
                .iter()
                .zip(&expected)
                .enumerate()
                .find(|(_, (a, b))| a != b)
                .map(|(i, (a, b))| (i, *a, *b))
        );
        step += 1;
    }
}

#[test]
fn malformed_celt_postfilter_wraps_reference_signal_arithmetic() {
    // Qualification seed 2457, stereo 12 kHz, packet length 79. Keep the
    // independent packet exactly reproducible without a live C dependency.
    let mut seed = 2457_u32;
    let mut packet = Vec::new();
    for length in 0..=79 {
        packet = (0..length)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                seed.to_le_bytes()[3]
            })
            .collect();
    }
    assert_eq!(&packet[..8], &[237, 164, 59, 32, 115, 170, 67, 100]);
    let mut decoder = Decoder::new(SampleRate::Hz12000, Channels::Stereo).unwrap();
    let mut output = vec![1234; 11536];
    let (audio, guard) = output.split_at_mut(11520);
    let _ = decoder.decode(Some(&packet), audio, false);
    assert_eq!(guard, &[1234; 16]);
    decoder.reset_state().unwrap();
}

#[test]
fn gain_preserves_reset_and_does_not_change_prediction() {
    let packet = include_bytes!("../src/silk/reference/nb_sine_mono.packet");
    let mut decoder = Decoder::new(SampleRate::Hz8000, Channels::Mono).unwrap();
    let mut control = Decoder::new(SampleRate::Hz8000, Channels::Mono).unwrap();
    decoder.set_gain(-1536);
    decoder.reset_state().unwrap();
    assert_eq!(decoder.gain(), -1536);
    let mut actual = [0; 160];
    let mut expected = [0; 160];
    decoder.decode(Some(packet), &mut actual, false).unwrap();
    control.decode(Some(packet), &mut expected, false).unwrap();
    assert!(actual.iter().zip(&expected).any(|(a, b)| a != b));
    assert!(
        actual
            .iter()
            .zip(&expected)
            .all(|(a, b)| i32::from(*a).abs() <= i32::from(*b).abs())
    );
    decoder.set_gain(0);
    decoder.decode(Some(packet), &mut actual, false).unwrap();
    control.decode(Some(packet), &mut expected, false).unwrap();
    assert_eq!(actual, expected);
}
