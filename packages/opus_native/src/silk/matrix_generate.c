#include <stdio.h>
#include <stdlib.h>
#include <stdint.h>
#include <math.h>
#include "opus.h"
int main(int argc,char**argv){int r=atoi(argv[1]),ch=atoi(argv[2]),ms=atoi(argv[3]),err;OpusEncoder*e=opus_encoder_create(r,ch,2048,&err);OpusDecoder*d=opus_decoder_create(r,ch,&err);opus_encoder_ctl(e,11002,1000);opus_encoder_ctl(e,4008,r==8000?1101:r==12000?1102:1103);opus_encoder_ctl(e,4002,ch==2?24000:16000);opus_encoder_ctl(e,4006,0);opus_encoder_ctl(e,4010,10);opus_encoder_ctl(e,4016,1);int n=r*ms/1000;int16_t in[5760*2],out[5760*2];unsigned char p[4000];for(int f=0;f<8;f++){if(argc>4){opus_encoder_ctl(e,4022,2);opus_encoder_ctl(e,4002,f<3?8000:28000);}for(int i=0;i<n;i++)for(int c=0;c<ch;c++){double t=(double)(f*n+i)/r;in[i*ch+c]=(int16_t)(11000*sin(6.28318530718*(c && !(argc>4 && (f<3 || (f==3 && i<n/3)))?347:221)*t)+3500*sin(6.28318530718*913*t));}int len=opus_encode(e,in,n,p,4000);if(len<0)return 2;uint32_t l=len;fwrite(&l,4,1,stdout);fwrite(p,1,len,stdout);int got=opus_decode(d,p,len,out,5760,0);if(got<0)return 3;fwrite(&got,4,1,stderr);fwrite(out,2,got*ch,stderr);}return 0;}
