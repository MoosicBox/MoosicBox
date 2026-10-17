"""Dev-only pinned CNG estimate oracle. Compiles actual CNG.c, not a model."""

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
#include <string.h>
int main(void) {
 silk_decoder_state s = {0}; silk_decoder_control ctl = {0}; opus_int16 pcm[320] = {0};
 int rates[] = {8,8,12,12,16,16};
 for(int f=0;f<6;f++) {
  s.lossCnt=0;memset(pcm,0,sizeof(pcm));
  s.fs_kHz=rates[f];s.LPC_order=s.fs_kHz==16?16:10;s.nb_subfr=f%2?2:4;s.subfr_length=s.fs_kHz*5;
  s.prevSignalType=f%3==1?TYPE_VOICED:TYPE_NO_VOICE_ACTIVITY;
  for(int i=0;i<s.LPC_order;i++) s.prevNLSF_Q15[i]=(i+1)*32767/(s.LPC_order+1)+((i%3)-1)*33;
  for(int i=0;i<s.nb_subfr;i++) ctl.Gains_Q16[i]=65536*(i+2+f);
  for(int i=0;i<s.nb_subfr*s.subfr_length;i++) s.exc_Q14[i]=(i*31-1000)*16;
  silk_CNG(&s,&ctl,pcm,s.nb_subfr*s.subfr_length);
  printf("%d ",s.sCNG.CNG_smth_Gain_Q16);
  for(int i=0;i<16;i++) printf("%d ",s.sCNG.CNG_smth_NLSF_Q15[i]);
  for(int i=0;i<320;i++) printf("%d ",s.sCNG.CNG_exc_buf_Q14[i]);
  puts("");
  s.lossCnt=1;s.sPLC.randScale_Q14=200;s.sPLC.prevGain_Q16[1]=65536;
  silk_CNG(&s,&ctl,pcm,s.nb_subfr*s.subfr_length);
  printf("PCM ");
  for(int i=0;i<s.nb_subfr*s.subfr_length;i++) printf("%d ",pcm[i]);
  puts("");
 }
 return 0;
}
"""
with tempfile.TemporaryDirectory(prefix="silk-cng-state-oracle-") as d:
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
