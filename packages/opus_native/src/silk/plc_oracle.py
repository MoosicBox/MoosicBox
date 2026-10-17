"""Pinned silk_PLC update state oracle (no concealment PCM claim)."""

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
 silk_decoder_state s={0}; silk_decoder_control ctl={0}; opus_int16 pcm[320]={0};
 int rates[]={8,8,12,12,16,16};
 for(int f=0;f<6;f++) {
  s.lossCnt=0;
  s.fs_kHz=rates[f];s.LPC_order=s.fs_kHz==16?16:10;s.nb_subfr=f%2?2:4;s.subfr_length=s.fs_kHz*5;s.frame_length=s.subfr_length*s.nb_subfr;
  s.indices.signalType=f%3==1?TYPE_UNVOICED:TYPE_VOICED;
  for(int i=0;i<s.LPC_order;i++) ctl.PredCoef_Q12[1][i]=(i%2?1:-1)*(i+1)*11;
  for(int i=0;i<s.nb_subfr;i++) {
   ctl.Gains_Q16[i]=65536*(i+2+f);ctl.pitchL[i]=s.fs_kHz*(7+i);
   for(int j=0;j<5;j++) ctl.LTPCoef_Q14[i*5+j]=(j==2?60+f*12:3+i)*128;
  }
  ctl.LTP_scale_Q14=12288;
  silk_PLC(&s,&ctl,pcm,0,0);
  printf("%d ",s.sPLC.pitchL_Q8);
  for(int i=0;i<5;i++) printf("%d ",s.sPLC.LTPCoef_Q14[i]);
  for(int i=0;i<16;i++) printf("%d ",s.sPLC.prevLPC_Q12[i]);
  printf("%d %d %d %d %d\n",s.sPLC.prevGain_Q16[0],s.sPLC.prevGain_Q16[1],s.sPLC.prevLTP_scale_Q14,s.sPLC.subfr_length,s.sPLC.nb_subfr);
  s.first_frame_after_reset=0;s.ltp_mem_length=s.fs_kHz*20;
  for(int i=0;i<s.frame_length;i++) s.exc_Q14[i]=(i*71-1000)*16;
  for(int i=0;i<s.ltp_mem_length;i++) s.outBuf[i]=(i*13)%1000-500;
  for(int i=0;i<16;i++) s.sLPC_Q14_buf[i]=(i-8)*1234;
  for(int loss=0;loss<3;loss++) {
   silk_PLC(&s,&ctl,pcm,1,0);
   printf("PCM "); for(int i=0;i<s.frame_length;i++) printf("%d ",pcm[i]); puts("");
   for(int i=0;i<s.ltp_mem_length-s.frame_length;i++) s.outBuf[i]=s.outBuf[i+s.frame_length];
   for(int i=0;i<s.frame_length;i++) s.outBuf[s.ltp_mem_length-s.frame_length+i]=pcm[i];
  }
 }
 return 0;
}
"""
with tempfile.TemporaryDirectory(prefix="silk-plc-update-oracle-") as d:
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
