#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include "opus.h"
int main(int argc,char**argv){int rate=atoi(argv[1]),channels=atoi(argv[2]),error,fec=argc>3;OpusDecoder*d=opus_decoder_create(rate,channels,&error);uint32_t l;unsigned char p[65536];int16_t out[11520];while(fread(&l,4,1,stdin)==1){if(l>65536||fread(p,1,l,stdin)!=l)return 2;int n=opus_decode(d,l?p:NULL,l,out,5760,fec);if(n<0){fprintf(stderr,"C error %d\n",n);return 3;}fwrite(&n,4,1,stdout);fwrite(out,2,n*channels,stdout);}return 0;}
