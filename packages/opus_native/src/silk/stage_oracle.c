/* Dev-only stage oracle. Source revision checked by stage_oracle.py. */
#include "config.h"
#include "opus.h"
#include "main.h"
#include <stdio.h>
#include <stdlib.h>

static void samples(const char *stage, const opus_int16 *pcm, int n) {
    printf("{\"stage\":\"%s\",\"samples\":[", stage);
    for (int i = 0; i < n; ++i) printf("%s%d", i ? "," : "", pcm[i]);
    puts("]}");
}
extern void __real_silk_NLSF_stabilize(opus_int16 *, const opus_int16 *, int);
void __wrap_silk_NLSF_stabilize(opus_int16 *nlsf, const opus_int16 *spacing, int order) {
    samples("nlsf_before_stabilize",nlsf,order);
    __real_silk_NLSF_stabilize(nlsf,spacing,order);
    samples("nlsf_after_stabilize",nlsf,order);
}
extern int __real_silk_decode_frame(silk_decoder_state *, ec_dec *, opus_int16 *, opus_int32 *, int, int, int);
int __wrap_silk_decode_frame(silk_decoder_state *state, ec_dec *range, opus_int16 *out, opus_int32 *n, int loss, int cond, int arch) {
    printf("{\"stage\":\"channel_before\",\"conditional\":%d,\"first_reset\":%d,\"previous_nlsf\":[",cond,state->first_frame_after_reset);
    for(int i=0;i<state->LPC_order;i++) printf("%s%d",i?",":"",state->prevNLSF_Q15[i]);
    puts("]}");
    int result = __real_silk_decode_frame(state, range, out, n, loss, cond, arch);
    printf("{\"stage\":\"frame_parameters\",\"signal_type\":%d,\"quant_offset\":%d,\"gain_indices\":[%d,%d,%d,%d],\"seed\":%d,\"tell\":%d}\n",
        state->indices.signalType, state->indices.quantOffsetType,
        state->indices.GainsIndices[0],state->indices.GainsIndices[1],state->indices.GainsIndices[2],state->indices.GainsIndices[3],state->indices.Seed,ec_tell(range));
    samples("decoded_channel", out, *n);
    return result;
}
extern void __real_silk_stereo_decode_mid_only(ec_dec *, opus_int *);
void __wrap_silk_stereo_decode_mid_only(ec_dec *range, opus_int *only) {
    __real_silk_stereo_decode_mid_only(range,only);
    printf("{\"stage\":\"mid_only\",\"value\":%d}\n",*only);
}
extern void __real_silk_stereo_decode_pred(ec_dec *, opus_int32 *);
void __wrap_silk_stereo_decode_pred(ec_dec *range, opus_int32 *pred) {
    __real_silk_stereo_decode_pred(range,pred);
    printf("{\"stage\":\"stereo_prediction\",\"weights\":[%d,%d],\"tell\":%d}\n",pred[0],pred[1],ec_tell(range));
}
extern void __real_silk_stereo_MS_to_LR(stereo_dec_state *, opus_int16 *, opus_int16 *, const opus_int32 *, int, int);
void __wrap_silk_stereo_MS_to_LR(stereo_dec_state *state, opus_int16 *mid, opus_int16 *side, const opus_int32 *pred, int rate, int n) {
    __real_silk_stereo_MS_to_LR(state,mid,side,pred,rate,n);
    opus_int16 interleaved[1920];
    for(int i=0;i<n;i++) { interleaved[2*i]=mid[i+1]; interleaved[2*i+1]=side[i+1]; }
    samples("stereo_lr",interleaved,n*2);
}
extern void __real_silk_decode_core(silk_decoder_state *, silk_decoder_control *, opus_int16 *, const opus_int16 *, int);
void __wrap_silk_decode_core(silk_decoder_state *state, silk_decoder_control *control, opus_int16 *out, const opus_int16 *pulses, int arch) {
    printf("{\"stage\":\"core_input\",\"gains\":[%d,%d,%d,%d],\"coefficients\":[",control->Gains_Q16[0],control->Gains_Q16[1],control->Gains_Q16[2],control->Gains_Q16[3]);
    for(int i=0;i<2*state->LPC_order;i++) printf("%s%d",i?",":"",control->PredCoef_Q12[i/state->LPC_order][i%state->LPC_order]);
    puts("]}");
    printf("{\"stage\":\"ltp_parameters\",\"scale\":%d,\"lags\":[%d,%d,%d,%d],\"taps\":[", control->LTP_scale_Q14,control->pitchL[0],control->pitchL[1],control->pitchL[2],control->pitchL[3]);
    for(int i=0;i<20;i++) printf("%s%d",i?",":"",control->LTPCoef_Q14[i]);
    puts("]}");
    samples("pulses",pulses,state->frame_length);
    __real_silk_decode_core(state,control,out,pulses,arch);
}
int main(int argc, char **argv) {
    if(argc < 4) return 2;
    int error;
    OpusDecoder *decoder = opus_decoder_create(atoi(argv[1]),atoi(argv[2]),&error);
    if (!decoder || error) return 3;
    for (int a = 3; a < argc; ++a) {
        FILE *f = fopen(argv[a],"rb");
        if(!f) return 4;
        unsigned char packet[4000];
        size_t length = fread(packet,1,sizeof(packet),f); fclose(f);
        opus_int16 output[5760 * 2];
        int n = opus_decode(decoder,packet,(int)length,output,5760,0);
        if(n < 0) return 5;
        samples("api_output",output,n * atoi(argv[2]));
    }
    opus_decoder_destroy(decoder);
    return 0;
}
