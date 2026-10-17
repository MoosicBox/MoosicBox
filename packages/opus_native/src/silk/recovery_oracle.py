"""Actual pinned silk_PLC_glue_frames oracle, synthetic loss/recovery sequences."""

from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[2]
import os
import sys
import shlex

sys.path.insert(0, str(root / "libopus/src"))
from oracle import Oracle

with Oracle(os.environ["MOOSICBOX_OPUS_REFERENCE_ORACLE"]) as oracle:
    manifest = oracle.manifest
source = Path(manifest["source"])
archive = Path(manifest["archive"])
build = Path(manifest["build"])
command = shlex.split(manifest["compile_commands"][0]["command"])
compiler = command[0]
definitions = [
    word for word in command[1:] if word.startswith("-D") or word.startswith("-U")
]
assert manifest["fixed_point"] and not manifest["res24"] and not manifest["intrinsics"]
program = r"""
#include "config.h"
#include "main.h"
#include <stdio.h>
int main(void) {
 for(int n=80;n<=320;n*=2) {
  silk_decoder_state s={0}; opus_int16 pcm[320];
  for(int f=0;f<8;f++) {
   int gain=f==0?0:f<3?17:f==3?101:37;
   for(int i=0;i<n;i++) pcm[i]=((i*13)%71-35)*gain;
   s.lossCnt=f<3 || f==5;
   silk_PLC_glue_frames(&s,pcm,n);
   for(int i=0;i<n;i++) printf("%d ",pcm[i]);
   puts("");
  }
 }
 return 0;
}
"""
with tempfile.TemporaryDirectory(prefix="silk-recovery-oracle-") as d:
    c = Path(d) / "oracle.c"
    exe = Path(d) / "oracle"
    c.write_text(program)
    subprocess.run(
        [
            compiler,
            "-O1",
            *definitions,
            *[
                "-I" + str(p)
                for p in [build, source / "include", source / "silk", source / "celt"]
            ],
            str(c),
            str(archive),
            "-lm",
            "-o",
            str(exe),
        ],
        check=True,
    )
    subprocess.run([str(exe)], check=True)
