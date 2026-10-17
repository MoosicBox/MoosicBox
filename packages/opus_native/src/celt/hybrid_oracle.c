/* Development oracle: actual frozen hybrid packets, linked pinned CMake libopus. */
#include "config.h"
#include "opus.h"
#include "API.h"
#include "celt.h"
#include "bands.h"
#include <stdio.h>
#include <stdlib.h>
static int active, depth, prefix;

static void entropy(const char *stage,ec_dec *ec) {
 printf("{\"stage\":\"%s\",\"range\":%u,\"value\":%u,\"tell\":%d,\"storage\":%u}\n",stage,ec->rng,ec->val,ec_tell(ec),ec->storage);
}
extern int __real_ec_dec_bit_logp(ec_dec *,unsigned);
int __wrap_ec_dec_bit_logp(ec_dec *ec,unsigned logp) {int outer=depth++==0;int v=__real_ec_dec_bit_logp(ec,logp);depth--;if(prefix&&outer)printf("{\"stage\":\"op\",\"kind\":0,\"args\":[%u],\"value\":%d}\n",logp,v);return v;}
extern int __real_ec_dec_icdf(ec_dec *,const unsigned char *,unsigned);
int __wrap_ec_dec_icdf(ec_dec *ec,const unsigned char *pdf,unsigned ftb) {int outer=depth++==0;int v=__real_ec_dec_icdf(ec,pdf,ftb);depth--;if(prefix&&outer){printf("{\"stage\":\"op\",\"kind\":1,\"args\":[%u",ftb);for(int i=0;;i++){printf(",%u",pdf[i]);if(pdf[i]==0)break;}printf("],\"value\":%d}\n",v);}return v;}
extern opus_uint32 __real_ec_dec_uint(ec_dec *,opus_uint32);
opus_uint32 __wrap_ec_dec_uint(ec_dec *ec,opus_uint32 total){int outer=depth++==0;opus_uint32 v=__real_ec_dec_uint(ec,total);depth--;if(prefix&&outer)printf("{\"stage\":\"op\",\"kind\":2,\"args\":[%u],\"value\":%u}\n",total,v);return v;}
extern opus_uint32 __real_ec_dec_bits(ec_dec *,unsigned);
opus_uint32 __wrap_ec_dec_bits(ec_dec *ec,unsigned bits){int outer=depth++==0;opus_uint32 v=__real_ec_dec_bits(ec,bits);depth--;if(prefix&&outer)printf("{\"stage\":\"op\",\"kind\":3,\"args\":[%u],\"value\":%u}\n",bits,v);return v;}
static void values(const char *stage,const opus_int32 *x,int n) {printf("{\"stage\":\"%s\",\"values\":[",stage);for(int i=0;i<n;i++)printf("%s%d",i?",":"",x[i]);puts("]}");}
extern int __real_silk_Decode(void *,silk_DecControlStruct *,int,int,ec_dec *,opus_res *,opus_int32 *,int);
int __wrap_silk_Decode(void *s,silk_DecControlStruct *ctl,int loss,int first,ec_dec *ec,opus_res *pcm,opus_int32 *n,int arch) {
 int r=__real_silk_Decode(s,ctl,loss,first,ec,pcm,n,arch);entropy("after_silk",ec);return r;
}
extern int __real_celt_decode_with_ec_dred(CELTDecoder *,const unsigned char *,int,opus_res *,int,ec_dec *,int);
int __wrap_celt_decode_with_ec_dred(CELTDecoder *s,const unsigned char *data,int len,opus_res *pcm,int n,ec_dec *ec,int accum) {
 if(ec) {prefix=0;printf("{\"stage\":\"entry\",\"bytes\":%d,\"n\":%d,\"accum\":%d}\n",len,n,accum);entropy("entry_entropy",ec);active=1;}
 int r=__real_celt_decode_with_ec_dred(s,data,len,pcm,n,ec,accum);
 if(ec){entropy("final_entropy",ec);active=0;}return r;
}
extern void __real_quant_all_bands(int,const CELTMode *,int,int,celt_norm *,celt_norm *,unsigned char *,const celt_ener *,int *,int,int,int,int,int *,opus_int32,opus_int32,ec_ctx *,int,int,opus_uint32 *,int,int,int);
void __wrap_quant_all_bands(int encode,const CELTMode *m,int start,int end,celt_norm *x,celt_norm *y,unsigned char *masks,const celt_ener *e,int *p,int blocks,int spread,int dual,int intensity,int *tf,opus_int32 total,opus_int32 balance,ec_ctx *ec,int lm,int coded,opus_uint32 *seed,int complexity,int arch,int disable) {
 __real_quant_all_bands(encode,m,start,end,x,y,masks,e,p,blocks,spread,dual,intensity,tf,total,balance,ec,lm,coded,seed,complexity,arch,disable);
 if(active){entropy("residual_entropy",ec);printf("{\"stage\":\"residual\",\"start\":%d,\"end\":%d,\"values\":[",start,end);int n=120<<lm;for(int ch=0;ch<(y?2:1);ch++)for(int i=0;i<n;i++)printf("%s%d",ch||i?",":"",ch?y[i]:x[i]);puts("]}");}
}
extern void __real_comb_filter(opus_val32 *,opus_val32 *,int,int,int,opus_val16,opus_val16,int,int,const opus_val16 *,int,int);
void __wrap_comb_filter(opus_val32 *y,opus_val32 *x,int t0,int t1,int n,opus_val16 g0,opus_val16 g1,int tap0,int tap1,const opus_val16 *w,int overlap,int arch) {
 __real_comb_filter(y,x,t0,t1,n,g0,g1,tap0,tap1,w,overlap,arch);if(active)values("synthesis",y,n);
}
int main(int argc,char **argv) {
 if(argc<3)return 2;int channels=atoi(argv[1]),error;int rate=getenv("C8_OUTPUT_RATE")?atoi(getenv("C8_OUTPUT_RATE")):48000;OpusDecoder *d=opus_decoder_create(rate,channels,&error);if(!d||error)return 3;
 for(int i=2;i<argc;i++){unsigned char packet[4000];FILE *f=fopen(argv[i],"rb");if(!f)return 4;int len=fread(packet,1,sizeof(packet),f);fclose(f);printf("{\"stage\":\"packet\",\"index\":%d}\n",i-2);prefix=1;opus_int16 pcm[11520];int n=opus_decode(d,packet,len,pcm,5760,0);if(n<0)return 5;}
 opus_decoder_destroy(d);return 0;
}
