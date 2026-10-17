#!/usr/bin/env python3
"""Explicit opt-in pinned oracle; stdout wire format is unchanged.

Requires MOOSICBOX_OPUS_REFERENCE_ORACLE pointing to the Cargo-built
reference_oracle example. --decode RATE CHANNELS reads LE u32 packet lengths
and packets, emits LE i16 PCM. --shrink emits upstream entropy probe text.
CHANNELS RATE reads LE i32 signals and emits LE i16 deemphasis output.
Internal probes compile temporary executables, never shared libraries.
"""
import os
import pathlib
import shlex
import struct
import subprocess
import sys
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(root / "libopus/src"))
from oracle import Oracle

executable = os.environ.get("MOOSICBOX_OPUS_REFERENCE_ORACLE")
if not executable:
    raise RuntimeError(
        "Set MOOSICBOX_OPUS_REFERENCE_ORACLE to the explicitly built "
        "moosicbox_opus_native_libopus reference_oracle executable; "
        "see libopus/README.md. No cache/system-library fallback is permitted."
    )

# Both public API calls and internal probes begin with central qualification.
with Oracle(executable) as oracle:
    manifest = oracle.manifest
    if len(sys.argv) > 1 and sys.argv[1] == "--decode":
        rate, channels = map(int, sys.argv[2:])
        oracle.request("decoder", rate=rate, channels=channels)
        raw = sys.stdin.buffer.read()
        offset = 0
        pcm = []
        while offset < len(raw):
            if offset + 4 > len(raw):
                raise ValueError("truncated packet length")
            (length,) = struct.unpack_from("<I", raw, offset)
            offset += 4
            if length > 65535 or offset + length > len(raw):
                raise ValueError("invalid/truncated packet")
            packet = list(raw[offset : offset + length])
            offset += length
            pcm.extend(oracle.request("decode", frame_size=5760, packet=packet)["pcm"])
        sys.stdout.buffer.write(struct.pack("<" + "h" * len(pcm), *pcm))
        sys.exit(0)

source = pathlib.Path(manifest["source"])
archive = pathlib.Path(manifest["archive"])
# Use the exact qualified compiler command, not a guessed PATH compiler.
compiler = shlex.split(manifest["compile_commands"][0]["command"])[0]


def run_probe(program, raw=b"", arguments=(), link_archive=False):
    with tempfile.TemporaryDirectory(prefix="opus-output-oracle-") as directory:
        c = pathlib.Path(directory) / "oracle.c"
        binary = pathlib.Path(directory) / (
            "oracle.exe" if os.name == "nt" else "oracle"
        )
        c.write_text(program)
        command = [
            compiler,
            "-std=c99",
            "-O2",
            "-I",
            str(source / "include"),
            "-I",
            str(source / "celt"),
            str(c),
        ]
        if link_archive:
            command.append(str(archive))
            if os.name != "nt":
                command.append("-lm")
        subprocess.run(command + ["-o", str(binary)], check=True)
        subprocess.run(
            [str(binary), *map(str, arguments)],
            input=raw,
            stdout=sys.stdout.buffer,
            check=True,
        )


if sys.argv[1:] == ["--shrink"]:
    run_probe(
        r"""
#include <stdio.h>
#include "entdec.h"
int main(void) {
  unsigned char data[32]; for(int i=0;i<32;i++) data[i]=(i*73+19)&255;
  for(int buffered=0;buffered<2;buffered++) {
    ec_dec dec; ec_dec_init(&dec,data,32);
    for(int i=0;i<17;i++) printf("%u\n",ec_dec_bit_logp(&dec,1+i%4));
    if(buffered) printf("%u\n",ec_dec_bits(&dec,5));
    /* Exact upstream opus_decoder.c operation; no arithmetic reinit. */
    dec.storage -= 7;
    printf("%u\n%u\n",ec_tell(&dec),dec.rng);
    for(int i=0;i<6;i++) {
      printf("%u\n",ec_dec_bits(&dec,3+i));
      printf("%u\n",ec_dec_uint(&dec,37));
      printf("%u\n%u\n",ec_tell(&dec),dec.rng);
    }
  }
  return 0;
}
""",
        link_archive=True,
    )
    sys.exit(0)

text = (source / "celt/celt_decoder.c").read_text()
start = text.index("static void deemphasis_stereo_simple(")
end = text.index("\n#ifndef RESYNTH", text.index("void deemphasis(", start) + 1)
body = "#if 1\n" + text[start:end]
shim = r"""
#define RESYNTH 1
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>
typedef int32_t celt_sig;
typedef int16_t opus_res;
typedef int16_t opus_val16;
#define OPUS_RESTRICT restrict
#define VERY_SMALL 0
#define SIG_SAT 536870911
#define SATURATE(x,a) ((x)>(a)?(a):((x)<-(a)?-(a):(x)))
#define MULT16_32_Q15(a,b) ((int32_t)(((int64_t)(a)*(b))>>15))
static int16_t sig2res(int32_t x) { x=(x+2048)>>12; return x>32767?32767:(x< -32768?-32768:x); }
#define SIG2RES(x) sig2res(x)
#define ADD_RES(a,b) ((a)+(b))
#define VARDECL(t,n) t *n
#define SAVE_STACK
#define ALLOC(n,size,t) t storage[size]; n=storage
#define RESTORE_STACK
"""
channels, rate = map(int, sys.argv[1:])
raw = sys.stdin.buffer.read()
assert channels in (1, 2) and rate in (8000, 12000, 16000, 24000, 48000)
assert len(raw) % (4 * channels) == 0
n = len(raw) // (4 * channels)
assert n > 0
# Explicit LE transport rather than host-endian ctypes or native fwrite.
main = r"""
static int read_i32(int32_t *value) {
  unsigned char b[4]; if(fread(b,1,4,stdin)!=4) return 0;
  uint32_t u=(uint32_t)b[0]|((uint32_t)b[1]<<8)|((uint32_t)b[2]<<16)|((uint32_t)b[3]<<24);
  *value=(int32_t)u; return 1;
}
int main(int argc, char **argv) {
  if(argc!=4) return 2;
  int channels=atoi(argv[1]), rate=atoi(argv[2]), n=atoi(argv[3]);
  celt_sig *inputs[2];
  for(int ch=0;ch<channels;ch++) { inputs[ch]=calloc((size_t)n,sizeof(celt_sig)); if(!inputs[ch]) return 3; }
  for(int i=0;i<n;i++) for(int ch=0;ch<channels;ch++) if(!read_i32(&inputs[ch][i])) return 4;
  int count=n/(48000/rate)*channels;
  opus_res *output=calloc((size_t)count,sizeof(opus_res)); if(!output) return 3;
  celt_sig memory[2]={0,0}; opus_val16 coef[4]={27853,0,0,0};
  deemphasis(inputs,output,n,channels,48000/rate,coef,memory,0);
  for(int i=0;i<count;i++) { uint16_t v=(uint16_t)output[i]; putchar(v&255); putchar(v>>8); }
  for(int ch=0;ch<channels;ch++) free(inputs[ch]); free(output); return 0;
}
"""
run_probe(shim + body + main, raw, (channels, rate, n))
