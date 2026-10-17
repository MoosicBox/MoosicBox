#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_opus_native::range::{RangeDecoder, RangeEncoder, RangeEncoderError};
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

// This is an oracle driver, not an entropy implementation: compile actual pinned
// upstream entenc.c/entdec.c/entcode.c with fixed-point configuration. No library
// FFI or unsafe Rust is used in the production encoder or this test.
const ORACLE: &str = r#"
#include <stdio.h>
#include <stdlib.h>
#include "entenc.h"
#include "entdec.h"
static const unsigned char icdf8[]={200,100,40,0};
static const opus_uint16 icdf16[]={30000,16000,1000,0};
typedef struct {unsigned op,a,b,c;} item;
int main(void){
 unsigned size,n,i; if(scanf("%u %u",&size,&n)!=2)return 2;
 unsigned char *buf=calloc(size?size:1,1); item *ops=calloc(n?n:1,sizeof(item)); ec_enc enc;
 ec_enc_init(&enc,buf,size);
 for(i=0;i<n;i++){
  item *p=ops+i; if(scanf("%u %u %u %u",&p->op,&p->a,&p->b,&p->c)!=4)return 3;
  switch(p->op){
   case 0:ec_encode(&enc,p->a,p->a+1,p->b);break;
   case 1:ec_encode_bin(&enc,p->a,p->a+1,p->b);break;
   case 2:ec_enc_bit_logp(&enc,p->a,p->b);break;
   case 3:ec_enc_icdf(&enc,p->a,icdf8,8);break;
   case 4:ec_enc_uint(&enc,p->a,p->b);break;
   case 5:ec_enc_bits(&enc,p->a,p->b);break;
   case 6:ec_enc_icdf16(&enc,p->a,icdf16,15);break;
   default:return 4;
  }
  printf("%d %u %u\n",ec_tell(&enc),ec_tell_frac(&enc),enc.rng);
 }
 ec_enc_done(&enc); printf("%d\n",ec_get_error(&enc));
 for(i=0;i<size;i++)printf("%02x",buf[i]); puts("");
 if(!ec_get_error(&enc)){
  ec_dec dec; ec_dec_init(&dec,buf,size);
  for(i=0;i<n;i++){
   item *p=ops+i; unsigned value=0;
   switch(p->op){
    case 0:value=ec_decode(&dec,p->b);ec_dec_update(&dec,value,value+1,p->b);break;
    case 1:value=ec_decode_bin(&dec,p->b);ec_dec_update(&dec,value,value+1,1U<<p->b);break;
    case 2:value=ec_dec_bit_logp(&dec,p->b);break;
    case 3:value=ec_dec_icdf(&dec,icdf8,8);break;
    case 4:value=ec_dec_uint(&dec,p->b);break;
    case 5:value=ec_dec_bits(&dec,p->b);break;
    case 6:value=ec_dec_icdf16(&dec,icdf16,15);break;
   }
   if(value!=p->a){fprintf(stderr,"reference decode mismatch op %u: %u != %u\n",i,value,p->a);return 5;}
  }
 }
 free(buf);free(ops);return 0;
}
"#;

fn oracle() -> PathBuf {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("libopus/opus");
    let revision = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&source)
        .output()
        .unwrap();
    assert!(revision.status.success());
    assert_eq!(
        String::from_utf8(revision.stdout).unwrap().trim(),
        "34bba701ae97c913de719b1f7c10686f62cddb15"
    );
    assert!(
        Command::new("git")
            .args(["diff", "--quiet", "HEAD", "--", "."])
            .current_dir(&source)
            .status()
            .unwrap()
            .success(),
        "reference tracked source changed"
    );
    for (name, hash) in [
        (
            "entenc.c",
            "7a31fd300f15b48757b77176c8067e15563318e56b114596f88a07672331fd7d",
        ),
        (
            "entdec.c",
            "f2d74695ceeb927210dcf76d653241ebdfd465a1c23128604640d218cdd93c38",
        ),
        (
            "entcode.c",
            "fb52778fb752126b6da6366d29d532ee98d66eb366a28e913562eb9520ba9208",
        ),
    ] {
        let output = Command::new("sha256sum")
            .arg(source.join("celt").join(name))
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(
            String::from_utf8(output.stdout)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap(),
            hash
        );
    }
    let directory =
        std::env::temp_dir().join(format!("moosicbox-range-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let driver = directory.join("oracle.c");
    let executable = directory.join("oracle");
    std::fs::write(&driver, ORACLE).unwrap();
    let output = Command::new("cc")
        .args(["-std=c99", "-O2", "-DFIXED_POINT", "-DOPUS_BUILD"])
        .arg("-I")
        .arg(source.join("include"))
        .arg("-I")
        .arg(source.join("celt"))
        .arg(&driver)
        .arg(source.join("celt/entenc.c"))
        .arg(source.join("celt/entdec.c"))
        .arg(source.join("celt/entcode.c"))
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Record the exact independently built oracle identity in --nocapture output.
    let hash = Command::new("sha256sum").arg(&executable).output().unwrap();
    assert!(hash.status.success());
    eprintln!(
        "entropy reference {}",
        String::from_utf8_lossy(&hash.stdout).trim()
    );
    executable
}

fn apply(
    encoder: &mut RangeEncoder,
    [op, value, param, _]: [u32; 4],
) -> Result<(), RangeEncoderError> {
    match op {
        0 => encoder.ec_encode(value, value + 1, param),
        1 => encoder.ec_encode_bin(value, value + 1, param),
        2 => encoder.ec_enc_bit_logp(value != 0, param),
        3 => encoder.ec_enc_icdf(value as usize, &[200, 100, 40, 0], 8),
        4 => encoder.ec_enc_uint(value, param),
        5 => encoder.ec_enc_bits(value, param),
        6 => encoder.ec_enc_icdf_u16(value as usize, &[30000, 16000, 1000, 0], 15),
        _ => panic!("unknown operation"),
    }
}

fn compare(executable: &PathBuf, capacity: usize, operations: &[[u32; 4]]) {
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = format!("{capacity} {}\n", operations.len());
    let mut encoder = RangeEncoder::new(capacity).unwrap();
    let mut traces = Vec::new();
    for operation in operations {
        let [op, value, param, extra] = operation;
        input.push_str(&format!("{op} {value} {param} {extra}\n"));
        let result = apply(&mut encoder, *operation);
        assert!(result.is_ok() || result == Err(RangeEncoderError::BufferTooSmall));
        traces.push(format!(
            "{} {} {}",
            encoder.tell(),
            encoder.tell_frac(),
            encoder.range()
        ));
    }
    // Write on a separate thread to avoid deadlock on long traces and pipe limits.
    let mut stdin = child.stdin.take().unwrap();
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()).unwrap());
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap();
    assert!(
        output.status.success(),
        "reference failure: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let mut lines = stdout.lines();
    for (index, expected) in traces.iter().enumerate() {
        assert_eq!(
            lines.next(),
            Some(expected.as_str()),
            "state mismatch at operation {index}"
        );
    }
    let failed = lines.next().unwrap() != "0";
    let packet = encoder.finish();
    assert_eq!(
        packet.is_err(),
        failed,
        "finalization status, capacity {capacity}"
    );
    let Some(packet) = packet.ok() else {
        return;
    };
    let hex: String = packet.iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(
        lines.next(),
        Some(hex.as_str()),
        "final packet, capacity {capacity}"
    );
    // Native decoder currently rejects <2-byte initialization; C decoder above
    // still verifies legal zero/one-byte packets independently.
    if packet.len() < 2 {
        return;
    }
    let mut decoder = RangeDecoder::new(&packet).unwrap();
    for (index, [op, value, param, _]) in operations.iter().copied().enumerate() {
        let decoded = match op {
            0 => {
                let symbol = decoder.ec_decode(param).unwrap();
                decoder.ec_dec_update(symbol, symbol + 1, param).unwrap();
                symbol
            }
            1 => {
                let symbol = decoder.ec_decode_bin(param).unwrap();
                decoder
                    .ec_dec_update(symbol, symbol + 1, 1 << param)
                    .unwrap();
                symbol
            }
            2 => u32::from(decoder.ec_dec_bit_logp(param).unwrap()),
            3 => decoder.ec_dec_icdf(&[200, 100, 40, 0], 8).unwrap(),
            4 => decoder.ec_dec_uint(param).unwrap(),
            5 => decoder.ec_dec_bits(param).unwrap(),
            6 => u32::from(
                decoder
                    .ec_dec_icdf_u16(&[30000, 16000, 1000, 0], 15)
                    .unwrap(),
            ),
            _ => unreachable!(),
        };
        assert_eq!(decoded, value, "native decode operation {index}");
    }
}

#[test]
fn pinned_libopus_bytes_tell_and_decoder_interop() {
    let executable = oracle();
    let mixed = [
        [0, 0, 3, 0],
        [0, 2, 3, 0],
        [1, 123, 8, 0],
        [2, 1, 15, 0],
        [5, 0x1ff_ffff, 25, 0],
        [3, 2, 0, 0],
        [6, 3, 0, 0],
        [5, 0x1ff_ffff, 25, 0],
        [4, u32::MAX - 1, u32::MAX, 0],
        [5, 0, 1, 0],
        [2, 0, 1, 0],
        [4, 0, 2, 0],
    ];
    for capacity in 0..40 {
        compare(&executable, capacity, &mixed);
        compare(&executable, capacity, &[]);
    }
    for total in [2, 3, 255, 256, 257, 65535, 65536, 65537, u32::MAX] {
        let operations = [[4, 0, total, 0], [4, total - 1, total, 0]];
        compare(&executable, 32, &operations);
    }
    for bits in 1..=15 {
        compare(
            &executable,
            32,
            &[
                [1, 0, bits, 0],
                [1, (1 << bits) - 1, bits, 0],
                [2, 0, bits, 0],
                [2, 1, bits, 0],
            ],
        );
    }
    // Exhaustive short sequences force front/tail shared-byte termination at
    // capacities around the exact bit budget, including termination-only errors.
    for bits in 1..=8 {
        for value in 0..(1 << bits) {
            let operations = [[2, value & 1, 1, 0], [5, value, bits, 0]];
            for capacity in 1..=2 {
                compare(&executable, capacity, &operations);
            }
        }
    }
    for bits in 1..=25 {
        compare(
            &executable,
            16,
            &[[5, (1 << bits) - 1, bits, 0], [5, 0, bits, 0]],
        );
    }
    // Long all-ones/near-boundary arithmetic traces exercise delayed 0xff carry
    // propagation and carry resolution, interspersed with backwards raw writes.
    let mut carry = Vec::new();
    for i in 0..2048 {
        carry.push([0, if i % 257 == 0 { 0 } else { 255 }, 256, 0]);
        if i % 13 == 0 {
            carry.push([5, 31, 5, 0]);
        }
    }
    compare(&executable, 4096, &carry);
    let mut seed = 0x6d2b_79f5_u32;
    let mut operations = Vec::new();
    for _ in 0..8000 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let value = seed.rotate_left(13);
        operations.push(match seed % 7 {
            0 => [0, value % 32767, 32767, 0],
            1 => [1, value & 32767, 15, 0],
            2 => [2, value & 1, 1 + (value >> 1) % 15, 0],
            3 => [3, value % 4, 0, 0],
            4 => [4, value % 1_000_003, 1_000_003, 0],
            5 => [5, value & 0x1ff_ffff, 25, 0],
            _ => [6, value % 4, 0, 0],
        });
    }
    compare(&executable, 16384, &operations);
}

#[test]
fn frozen_mixed_trace_from_pinned_entenc() {
    // Generated by the hash-verified oracle; retained independently of a local C
    // compiler for a small byte-level regression with front/back interleaving.
    let operations = [
        [0, 2, 3, 0],
        [2, 1, 5, 0],
        [5, 21, 5, 0],
        [3, 2, 0, 0],
        [4, 65536, 65537, 0],
        [6, 3, 0, 0],
    ];
    let states = [
        (3, 21, 715827882),
        (8, 61, 22369621),
        (13, 101, 22369621),
        (15, 118, 1342172160),
        (31, 246, 10404435),
        (36, 286, 81152000),
    ];
    let mut encoder = RangeEncoder::new(8).unwrap();
    for (operation, expected) in operations.into_iter().zip(states) {
        apply(&mut encoder, operation).unwrap();
        assert_eq!(
            (encoder.tell(), encoder.tell_frac(), encoder.range()),
            expected
        );
    }
    assert_eq!(
        encoder.finish().unwrap(),
        [0xff, 0x95, 0x4c, 0, 0, 0, 0, 0x15]
    );
}

#[test]
fn invalid_arguments_are_rejected_without_state_changes() {
    let mut encoder = RangeEncoder::new(16).unwrap();
    assert_eq!(encoder.tell(), 1);
    assert_eq!(encoder.tell_frac(), 8);
    assert_eq!(encoder.storage(), 16);
    assert!(!encoder.has_error());
    for result in [
        encoder.ec_encode(0, 0, 0),
        encoder.ec_encode(0, 2, 1),
        encoder.ec_encode_bin(0, 1, 32),
        encoder.ec_enc_bit_logp(true, 0),
        encoder.ec_enc_icdf(0, &[0, 0], 8),
        encoder.ec_enc_icdf(0, &[1], 8),
        encoder.ec_enc_icdf_u16(0, &[32768, 0], 15),
        encoder.ec_enc_uint(0, 1),
        encoder.ec_enc_uint(2, 2),
        encoder.ec_enc_bits(0, 0),
        encoder.ec_enc_bits(1, 26),
        encoder.ec_enc_bits(4, 2),
    ] {
        assert_eq!(result, Err(RangeEncoderError::InvalidArgument));
    }
    assert_eq!(encoder.tell(), 1);
    assert_eq!(encoder.tell_frac(), 8);
    assert_eq!(encoder.finish().unwrap(), vec![0; 16]);
    let mut tiny = RangeEncoder::new(0).unwrap();
    for i in 0..100 {
        let _ = tiny.ec_enc_bit_logp(i % 2 == 0, 15);
    }
    assert!(tiny.has_error());
    assert_eq!(tiny.finish(), Err(RangeEncoderError::BufferTooSmall));
}
