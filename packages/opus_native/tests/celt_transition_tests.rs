//! Native transition contracts and explicit live-reference comparisons.
#![cfg(feature = "celt")]
#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]
use moosicbox_opus_native::{
    Channels, SampleRate,
    celt::{CeltDecoder, smooth_transition},
    range::RangeDecoder,
};
use moosicbox_opus_native::{Error, Result};
use std::process::Command;

#[test]
fn transition_validation_and_reset_contracts() {
    let mut decoder = CeltDecoder::new(SampleRate::Hz48000, Channels::Mono, 960).unwrap();
    assert!(decoder.decode_transition_redundancy(&[], 21).is_err());
    assert!(
        decoder
            .decode_transition_redundancy(&[0xff, 0xff], 18)
            .is_err()
    );
    let packet = include_bytes!("../test_vectors/frozen/celt_fb_mono/packet.bin");
    let mut entropy = RangeDecoder::new(&packet[1..]).unwrap();
    decoder
        .decode_celt_frame(&mut entropy, packet.len() - 1)
        .unwrap();
    let tail = decoder.decode_transition_silence().unwrap();
    assert_eq!(tail.samples.len(), 120);
    assert!(tail.samples.iter().any(|&x| x != 0));
    decoder.reset();
    assert!(
        decoder
            .decode_transition_silence()
            .unwrap()
            .samples
            .iter()
            .all(|&x| x == 0)
    );
    // Helper restores the 20-ms configuration, not its own 2.5-ms duration.
    let mut entropy = RangeDecoder::new(&packet[1..]).unwrap();
    assert_eq!(
        decoder
            .decode_celt_frame(&mut entropy, packet.len() - 1)
            .unwrap()
            .samples
            .len(),
        960
    );
    let mut output = [7; 120];
    assert!(smooth_transition(&[], &[], &mut output, SampleRate::Hz48000, Channels::Mono).is_err());
    assert_eq!(output, [7; 120]);
}

const ORACLE: &str = r"
import math,struct,sys,pathlib
sys.path.insert(0,str(pathlib.Path(sys.argv[1])/'libopus/src'))
from oracle import Oracle
with Oracle(sys.argv[2]) as o:
 channels=int(sys.argv[3])
 o.request('encoder',rate=48000,channels=channels,application=2051)
 o.request('decoder',rate=48000,channels=channels)
 o.request('configure',controls=[[4002,128000],[4010,0]])
 for i in range(9):
  if i==8:
   packet=[toc,255,255]
  else:
   pcm=[int(7000*math.sin(2*math.pi*(220+113*c)*(i*240+j)/48000)) for j in range(240) for c in range(channels)]
   packet=o.request('encode',frame_size=240,pcm=pcm)['packet']
   assert packet[0]&128
   toc=(packet[0]&0xfc)-8 # same bandwidth/channels, 2.5-ms config
  n=120 if i==8 else 240
  r=o.request('decode',frame_size=n,packet=packet)
  assert r['samples']==n
  sys.stdout.buffer.write(struct.pack('<II',len(packet),len(r['pcm']))+bytes(packet)+struct.pack('<'+'h'*len(r['pcm']),*r['pcm']))
";

#[test]
#[ignore = "requires MOOSICBOX_OPUS_REFERENCE_ORACLE qualified Cargo runner"]
fn transition_payload_and_silence_match_public_reference() {
    let executable = std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect(
        "set MOOSICBOX_OPUS_REFERENCE_ORACLE to qualified reference_oracle; see libopus/README.md",
    );
    for channels in [Channels::Mono, Channels::Stereo] {
        let result = Command::new("python3")
            .args(["-B", "-c", ORACLE, env!("CARGO_MANIFEST_DIR")])
            .arg(&executable)
            .arg((channels as usize).to_string())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let mut data = result.stdout.as_slice();
        let mut decoder = CeltDecoder::new(SampleRate::Hz48000, channels, 960).unwrap();
        let mut memory = [0_i32; 2];
        for i in 0..9 {
            let len = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
            let count = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
            let packet = &data[8..8 + len];
            let expected: Vec<_> = data[8 + len..8 + len + count * 2]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|x| i16::from_le_bytes(*x))
                .collect();
            data = &data[8 + len + count * 2..];
            let frame = if i == 8 {
                decoder.decode_transition_silence().unwrap()
            } else {
                decoder
                    .decode_transition_redundancy(&packet[1..], 21)
                    .unwrap()
            };
            let actual: Vec<_> = frame
                .samples
                .iter()
                .enumerate()
                .map(|(j, &x)| {
                    let c = j % channels as usize;
                    let filtered =
                        (i64::from(x) + i64::from(memory[c])).clamp(-536_870_911, 536_870_911);
                    memory[c] = i32::try_from((27853 * filtered) >> 15).unwrap();
                    i16::try_from(((filtered + 2048) >> 12).clamp(-32768, 32767)).unwrap()
                })
                .collect();
            assert_eq!(actual, expected, "channels={channels:?} frame={i}");
        }
        assert_eq!(data, []);
    }
}

const FADE_ORACLE: &str = r#"
import json,sys,pathlib,subprocess,tempfile,re,shlex
m=json.loads(subprocess.check_output([sys.argv[1],'provenance']))
assert m['fixed_point'] and not m['res24'] and m['tracked_clean']
root=pathlib.Path(m['source'])
s=(root/'src/opus_decoder.c').read_text()
functions=re.findall(r'static void smooth_fade\(.*?\n}',s,re.S)
assert len(functions)==2
window=re.search(r'static const celt_coef window120\[120\] = \{.*?\};',(root/'celt/static_modes_fixed.h').read_text(),re.S).group()
program='#include <stdio.h>\n#include "arch.h"\n'+window+'\n'+functions[1]+r'''
int main(void) {
 int rates[5]={8000,12000,16000,24000,48000};
 for(int r=0;r<5;r++) for(int c=1;c<=2;c++) {
  opus_res a[240],b[240],o[240]; int n=rates[r]/400*c;
  for(int i=0;i<n;i++){a[i]=(i*7919%65536)-32768;b[i]=32767-(i*3571%65536);}
  smooth_fade(a,b,o,rates[r]/400,c,window120,rates[r]);
  for(int i=0;i<n;i++) printf("%d\n",o[i]);
 }
}
'''
compiler=shlex.split(m['compile_commands'][0]['command'])[0]
with tempfile.TemporaryDirectory(prefix='opus-fade-') as d:
 src=pathlib.Path(d)/'fade.c';exe=pathlib.Path(d)/'fade'
 src.write_text(program)
 subprocess.run([compiler,'-std=c99','-DFIXED_POINT=1','-DVAR_ARRAYS=1','-I'+str(root/'include'),'-I'+str(root/'celt'),str(src),'-o',str(exe)],check=True)
 sys.stdout.buffer.write(subprocess.check_output([str(exe)]))
"#;

#[test]
#[ignore = "requires qualified MOOSICBOX_OPUS_REFERENCE_ORACLE and its C compiler/source"]
fn smooth_fade_matches_pinned_c_arithmetic() {
    let executable = std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect(
        "set MOOSICBOX_OPUS_REFERENCE_ORACLE to qualified Cargo runner; see libopus/README.md",
    );
    let result = Command::new("python3")
        .args(["-B", "-c", FADE_ORACLE])
        .arg(executable)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let expected: Vec<i16> = String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .map(|x| x.parse().unwrap())
        .collect();
    let mut actual = Vec::new();
    for rate in [
        SampleRate::Hz8000,
        SampleRate::Hz12000,
        SampleRate::Hz16000,
        SampleRate::Hz24000,
        SampleRate::Hz48000,
    ] {
        for channels in [Channels::Mono, Channels::Stereo] {
            let count = rate as usize / 400 * channels as usize;
            let old: Vec<_> = (0..count)
                .map(|i| i16::try_from(i32::try_from(i * 7919 % 65536).unwrap() - 32768).unwrap())
                .collect();
            let new: Vec<_> = (0..count)
                .map(|i| i16::try_from(32767 - i32::try_from(i * 3571 % 65536).unwrap()).unwrap())
                .collect();
            let mut out = vec![0; count];
            smooth_transition(&old, &new, &mut out, rate, channels).unwrap();
            actual.extend(out);
        }
    }
    assert_eq!(actual, expected);
}

#[path = "../src/output.rs"]
mod output;

const ACCUM_ORACLE: &str = r#"
import json,sys,pathlib,subprocess,tempfile,shlex,hashlib,re
m=json.loads(subprocess.check_output([sys.argv[1],'provenance']))
assert m['fixed_point'] and not m['res24'] and m['tracked_clean']
archive=pathlib.Path(m['archive']);assert hashlib.sha256(archive.read_bytes()).hexdigest()==m['archive_sha256']
root=pathlib.Path(m['source'])
source=(root/'celt/celt_decoder.c').read_text()
simple=re.search(r'static void deemphasis_stereo_simple\(.*?\n}',source,re.S).group()
function=re.search(r'void deemphasis\(.*?\n}',source,re.S).group()
program='#include <stdio.h>\n#include <string.h>\n#include "arch.h"\n#include "stack_alloc.h"\n'+simple+'\n'+function+r'''
#include <stdio.h>
#include "arch.h"

int main(void) {
 int rates[5]={8000,12000,16000,24000,48000};opus_val16 coef[4]={27853,0,0,0};
 for(int r=0;r<5;r++) for(int c=1;c<=2;c++) {
  celt_sig mem[2]={0,0};
  for(int f=0;f<3;f++) {
   celt_sig a[240],b[240],*in[2]={a,b};opus_res out[480];int n=240*rates[r]/48000*c;
   for(int i=0;i<240;i++)for(int ch=0;ch<c;ch++) in[ch][i]=((i*c+ch+f)%7==0)?400000000:-800000;
   for(int i=0;i<n;i++)out[i]=(i%2==0)?30000:-30000;
   deemphasis(in,out,240,c,48000/rates[r],coef,mem,1);
   for(int i=0;i<n;i++)printf("%d\n",out[i]);
  }
 }
}
'''
compiler=shlex.split(m['compile_commands'][0]['command'])[0]
with tempfile.TemporaryDirectory(prefix='opus-accum-') as d:
 src=pathlib.Path(d)/'accum.c';exe=pathlib.Path(d)/'accum';src.write_text(program)
 subprocess.run([compiler,'-std=c99','-DFIXED_POINT=1','-DVAR_ARRAYS=1','-I'+str(root/'include'),'-I'+str(root/'celt'),str(src),str(archive),'-lm','-o',str(exe)],check=True)
 sys.stdout.buffer.write(subprocess.check_output([str(exe)]))
"#;

#[test]
#[ignore = "requires qualified MOOSICBOX_OPUS_REFERENCE_ORACLE archive/compiler"]
fn accumulation_matches_qualified_c_deemphasis() {
    let executable = std::env::var_os("MOOSICBOX_OPUS_REFERENCE_ORACLE").expect(
        "set MOOSICBOX_OPUS_REFERENCE_ORACLE to qualified Cargo runner; see libopus/README.md",
    );
    let result = Command::new("python3")
        .args(["-B", "-c", ACCUM_ORACLE])
        .arg(executable)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let expected: Vec<i16> = String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .map(|x| x.parse().unwrap())
        .collect();
    let mut actual = Vec::new();
    for rate in [8000, 12000, 16000, 24000, 48000] {
        for channels in [1, 2] {
            let mut stage = output::CeltOutput::default();
            for frame in 0..3 {
                let signal: Vec<_> = (0..240 * channels)
                    .map(|i| {
                        if (i + frame) % 7 == 0 {
                            400_000_000
                        } else {
                            -800_000
                        }
                    })
                    .collect();
                let mut out: Vec<i16> = (0..240 * rate as usize / 48000 * channels)
                    .map(|i| if i % 2 == 0 { 30000 } else { -30000 })
                    .collect();
                stage
                    .process_accumulate(&signal, 240, channels, rate, &mut out)
                    .unwrap();
                actual.extend(out);
            }
        }
    }
    assert_eq!(actual, expected);
}
