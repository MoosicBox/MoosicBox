#!/usr/bin/env python3
"""Dev-only oracle: compile actual pinned deemphasis bodies, feed LE i32 signal.
Usage: output_oracle.py CHANNELS RATE < signal.i32 > output.i16
Requires cc; no network or persistent generated files.
"""
import ctypes
import hashlib
import json
import pathlib
import struct
import subprocess
import sys
import tempfile

root = pathlib.Path(__file__).resolve().parents[1]
source = root / "libopus/opus"
revision = subprocess.check_output(
    ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
).strip()
assert revision == "34bba701ae97c913de719b1f7c10686f62cddb15", revision
if len(sys.argv) > 1 and sys.argv[1] in ("--decode", "--shrink"):
    # Actual pinned public C API, linked only in a temporary dev oracle process.
    rate, channels = (48000, 1) if sys.argv[1] == "--shrink" else map(int, sys.argv[2:])
    repository = root.parents[1]
    libraries = list(
        (repository / "target/debug/build").glob(
            "moosicbox_opus_native_libopus-*/out/lib*/libopus.a"
        )
    )
    library = max(libraries, key=lambda path: path.stat().st_mtime)
    config = (library.parents[1] / "build/CMakeCache.txt").read_text()
    assert (
        "OPUS_FIXED_POINT:BOOL=ON" in config
        and "OPUS_DISABLE_INTRINSICS:BOOL=ON" in config
    )
    assert "OPUS_ENABLE_FLOAT_API:BOOL=OFF" in config
    effective = (library.parents[1] / "build/config.h").read_text()
    flags = (library.parents[1] / "build/CMakeFiles/opus.dir/flags.make").read_text()
    assert (
        "#define ENABLE_RES24" not in effective and "-DENABLE_RES24" not in flags
    ), "oracle currently targets verified non-RES24 build"
    print(
        json.dumps(
            {
                "reference_revision": revision,
                "archive": str(library),
                "archive_sha256": hashlib.sha256(library.read_bytes()).hexdigest(),
                "config_sha256": hashlib.sha256(config.encode()).hexdigest(),
                "res24": False,
            }
        ),
        file=sys.stderr,
    )
    program = r"""
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include "opus.h"
int main(int argc, char **argv) {
  int error, rate=atoi(argv[1]), channels=atoi(argv[2]);
  OpusDecoder *decoder=opus_decoder_create(rate,channels,&error);
  if (!decoder || error) return 2;
  uint32_t length;
  unsigned char packet[65536];
  opus_int16 output[5760*2];
  while (fread(&length,4,1,stdin)==1) {
    if (length>sizeof(packet) || fread(packet,1,length,stdin)!=length) return 3;
    int count=opus_decode(decoder,packet,length,output,5760,0);
    if (count<0) return 4;
    fwrite(output,sizeof(opus_int16),count*channels,stdout);
  }
  opus_decoder_destroy(decoder);
  return 0;
}
"""
    if sys.argv[1] == "--shrink":
        program = r"""
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
"""
    with tempfile.TemporaryDirectory(prefix="opus-api-output-oracle-") as directory:
        c = pathlib.Path(directory) / "oracle.c"
        executable = pathlib.Path(directory) / "oracle"
        c.write_text(program)
        subprocess.run(
            [
                "cc",
                "-O2",
                "-I",
                str(source / "include"),
                "-I",
                str(source / "celt"),
                str(c),
                str(library),
                "-lm",
                "-o",
                str(executable),
            ],
            check=True,
        )
        subprocess.run(
            [str(executable), str(rate), str(channels)],
            input=sys.stdin.buffer.read(),
            stdout=sys.stdout.buffer,
            check=True,
        )
    sys.exit(0)

text = (source / "celt/celt_decoder.c").read_text()
start = text.index("static void deemphasis_stereo_simple(")
end = text.index("\n#ifndef RESYNTH", text.index("void deemphasis(", start) + 1)
body = text[start:end]
# The extracted first body ends its surrounding !CUSTOM_MODES conditional.
body = "#if 1\n" + body
shim = r"""
#define RESYNTH 1
#include <stdint.h>
#include <stddef.h>
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
values = struct.unpack("<" + "i" * (len(raw) // 4), raw)
assert len(values) % channels == 0
n = len(values) // channels
with tempfile.TemporaryDirectory(prefix="opus-output-oracle-") as directory:
    c = pathlib.Path(directory) / "oracle.c"
    so = pathlib.Path(directory) / "oracle.so"
    c.write_text(shim + body)
    subprocess.run(
        ["cc", "-shared", "-fPIC", "-std=c99", "-O2", "-o", str(so), str(c)], check=True
    )
    lib = ctypes.CDLL(str(so))
    arrays = [(ctypes.c_int32 * n)(*values[ch::channels]) for ch in range(channels)]
    inputs = (ctypes.POINTER(ctypes.c_int32) * channels)(*arrays)
    output = (ctypes.c_int16 * (n // (48000 // rate) * channels))()
    memory = (ctypes.c_int32 * 2)()
    coef = (ctypes.c_int16 * 4)(27853, 0, 0, 0)
    lib.deemphasis(inputs, output, n, channels, 48000 // rate, coef, memory, 0)
    pcm = list(output)
    sys.stdout.buffer.write(struct.pack("<" + "h" * len(pcm), *pcm))
