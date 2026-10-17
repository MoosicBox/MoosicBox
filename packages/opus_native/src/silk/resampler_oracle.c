/* Pinned C resampler oracle: 3 persistent 20ms packets per rate pair. */
#include "config.h"
#include "resampler_private.h"
#include <stdio.h>
int main(void) {
    const int in_rates[3]={8000,12000,16000}, out_rates[5]={8000,12000,16000,24000,48000};
    for(int i=0;i<3;i++) for(int o=0;o<5;o++) {
        silk_resampler_state_struct state;
        if(silk_resampler_init(&state,in_rates[i],out_rates[o],0)) return 1;
        for(int packet=0;packet<3;packet++) {
            opus_int16 input[320],output[960];
            int n=in_rates[i]/50;
            for(int k=0;k<n;k++) {
                unsigned int v=(unsigned int)(k+packet*n)*1664525u+1013904223u;
                input[k]=(opus_int16)((int)(v>>16)-32768);
            }
            if(silk_resampler(&state,output,input,n)) return 2;
            for(int k=0;k<out_rates[o]/50;k++) {
                unsigned short v=(unsigned short)output[k];
                putchar(v&255);putchar(v>>8);
            }
        }
    }
}
